"""IDMXL's native BitTorrent worker. Authenticated, framed loopback IPC; no HTTP UI.

libtorrent owns protocol scheduling, piece verification and disk IO. SQLite owns
user intent; encrypted fast-resume checkpoints own validated transfer progress.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import hmac
import json
import os
from pathlib import Path, PureWindowsPath
import secrets
import socket
import sqlite3
import struct
import time
import urllib.parse
import urllib.request
import uuid

import libtorrent as lt

MAX_FRAME = 2 * 1024 * 1024
MAX_META = 16 * 1024 * 1024
DEFAULTS = dict(download_limit=0, upload_limit=524288, connections=500,
                per_torrent_connections=100, active_downloads=3, active_seeds=5,
                seed_ratio=1.0, listen_port=0, listen_address="all", dht=True, pex=True, lsd=True,
                port_mapping=True, utp=True, encryption="enabled")


def protected(data: bytes, decrypt=False) -> bytes:
    if os.name != "nt":
        return data  # Non-Windows test builds only.
    class Blob(ctypes.Structure):
        _fields_ = [("size", ctypes.c_ulong), ("data", ctypes.POINTER(ctypes.c_ubyte))]
    buffer = ctypes.create_string_buffer(data)
    source = Blob(len(data), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)))
    output = Blob()
    if decrypt:
        ok = ctypes.windll.crypt32.CryptUnprotectData(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(output))
    else:
        ok = ctypes.windll.crypt32.CryptProtectData(ctypes.byref(source), "IDMXL torrent state", None, None, None, 1, ctypes.byref(output))
    if not ok:
        raise ValueError("protected_state_unavailable")
    try:
        return ctypes.string_at(output.data, output.size)
    finally:
        ctypes.windll.kernel32.LocalFree(output.data)


def atomic(path: Path, data: bytes):
    temp = path.with_suffix(path.suffix + ".new")
    with temp.open("wb") as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(temp, path)


def checked_metadata(ti):
    files = ti.files()
    if files.num_files() > 100_000 or ti.total_size() > 2**63 - 1:
        raise ValueError("torrent_too_large")
    seen = set()
    for i in range(files.num_files()):
        name = files.file_path(i)
        p = PureWindowsPath(name)
        if p.is_absolute() or p.drive or any(c in ("..", "") or ":" in c for c in p.parts):
            raise ValueError("unsafe_torrent_path")
        if files.file_flags(i) & lt.file_storage.flag_symlink:
            raise ValueError("torrent_symlinks_unsupported")
        folded = str(p).casefold()
        if folded in seen:
            raise ValueError("duplicate_torrent_path")
        seen.add(folded)
    return ti


def load_source(source: str):
    if not isinstance(source, str) or len(source) > 16384:
        raise ValueError("invalid_torrent_source")
    if source.lower().startswith("magnet:?"):
        params = lt.parse_magnet_uri(source)
        if not params.info_hashes.has_v1() and not params.info_hashes.has_v2():
            raise ValueError("invalid_magnet")
        return params
    if source.startswith(("https://", "http://")):
        url = urllib.parse.urlsplit(source)
        if url.username or url.password:
            raise ValueError("invalid_torrent_source")
        request = urllib.request.Request(source, headers={"User-Agent": "IDMXL/0.2"})
        with urllib.request.urlopen(request, timeout=20) as response:
            data = response.read(MAX_META + 1)
    else:
        path = Path(source)
        if not path.is_file() or path.stat().st_size > MAX_META:
            raise ValueError("invalid_torrent_file")
        data = path.read_bytes()
    if len(data) > MAX_META:
        raise ValueError("torrent_metadata_too_large")
    params = lt.load_torrent_buffer(data)
    checked_metadata(params.ti)
    return params


class Worker:
    def __init__(self, directory: Path, test_settings=None):
        self.directory = directory.resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(self.directory / "torrents.db")
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA synchronous=FULL")
        self.db.executescript("""CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, record BLOB NOT NULL, resume BLOB);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);""")
        saved = self.db.execute("SELECT value FROM settings WHERE key='preferences'").fetchone()
        self.settings = DEFAULTS | (json.loads(saved[0]) if saved else {})
        clean = self.db.execute("SELECT value FROM settings WHERE key='clean_shutdown'").fetchone()
        unclean = clean is not None and clean[0] != "true"
        self.db.execute("INSERT OR REPLACE INTO settings VALUES('clean_shutdown','false')")
        self.db.commit()
        self.test_settings = test_settings or {}
        self.session = lt.session(self.session_settings())
        self.jobs, self.handles, self.pending_moves = {}, {}, {}
        self.finishing, self.finished, self.checking = set(), set(), set()
        self.checking_started = set()
        self.pending_resumes = set()
        self.resume_again = set()
        self.closing = False
        self.checkpoint_failed = False
        self.last_save, self.last_queue = time.monotonic(), 0.0
        self.stopping = False
        for jid, record, resume in self.db.execute("SELECT id,record,resume FROM jobs").fetchall():
            job = json.loads(protected(record, True))
            self.jobs[jid] = job
            if job["intent"] in ("canceled", "removed"):
                continue
            try:
                params = lt.read_resume_data(protected(resume, True)) if resume else load_source(job["source"])
                if params.ti:
                    checked_metadata(params.ti)
                if unclean and job["confirmed"]:
                    params.have_pieces = []
                    params.verified_pieces = []
                    params.unfinished_pieces = {}
                    params.flags &= ~(lt.torrent_flags.seed_mode | lt.torrent_flags.no_verify_files)
                h = self.attach(job, params)
                if unclean and job["confirmed"]:
                    self.checking.add(jid)
                    h.force_recheck()
            except Exception:
                job["error"] = "torrent_restore_failed"
                job["intent"] = "paused"
                self.persist(job)

    def session_settings(self):
        s = self.settings
        encryption = {"enabled": lt.enc_policy.enabled, "forced": lt.enc_policy.forced, "disabled": lt.enc_policy.disabled}[s["encryption"]]
        address=s["listen_address"]
        interface=f"[{address}]:{s['listen_port']}" if ":" in address else f"{address}:{s['listen_port']}"
        return dict(listen_interfaces=f"0.0.0.0:{s['listen_port']},[::]:{s['listen_port']}" if address=="all" else interface,
                    user_agent="IDMXL/0.2 libtorrent/" + lt.version,
                    download_rate_limit=s["download_limit"], upload_rate_limit=s["upload_limit"],
                    connections_limit=s["connections"], active_downloads=-1, active_seeds=-1,
                    enable_dht=s["dht"], enable_lsd=s["lsd"], enable_upnp=s["port_mapping"],
                    enable_natpmp=s["port_mapping"], enable_incoming_utp=s["utp"], enable_outgoing_utp=s["utp"],
                    in_enc_policy=int(encryption), out_enc_policy=int(encryption),
                    max_queued_disk_bytes=16 * 1024 * 1024, alert_queue_size=2000,
                    alert_mask=int(lt.alert.category_t.error_notification | lt.alert.category_t.status_notification | lt.alert.category_t.storage_notification),
                    ) | self.test_settings

    def persist(self, job):
        self.db.execute("INSERT INTO jobs(id,record) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record",
                        (job["id"], protected(json.dumps(job, ensure_ascii=False).encode())))
        self.db.commit()

    def attach(self, job, params):
        params.save_path = job["folder"]
        params.flags &= ~lt.torrent_flags.auto_managed
        params.flags |= lt.torrent_flags.paused
        if job["intent"] == "selecting":
            params.flags |= lt.torrent_flags.upload_mode
        elif job["confirmed"]:
            params.flags &= ~lt.torrent_flags.upload_mode
        if not self.settings["pex"]:
            params.flags |= lt.torrent_flags.disable_pex
        if job.get("sequential"):
            params.flags |= lt.torrent_flags.sequential_download
        h = self.session.add_torrent(params)
        h.set_max_connections(self.settings["per_torrent_connections"])
        self.handles[job["id"]] = h
        if job.get("priorities") and h.status().has_metadata:
            h.prioritize_files(job["priorities"])
        if job["intent"] in ("running", "selecting"):
            h.resume()
        return h

    def inspect(self, source):
        params = load_source(source)
        hashes = params.ti.info_hashes() if params.ti else params.info_hashes
        identity = str(hashes.v1) if hashes.has_v1() else str(hashes.v2)
        for job in self.jobs.values():
            if job["hash"] == identity and job["intent"] not in ("removed", "canceled"):
                return self.view(job) | {"already_added": True}
        jid = str(uuid.uuid4())
        folder = self.directory / "drafts" / jid
        folder.mkdir(parents=True, exist_ok=True)
        job = dict(id=jid, source=source, hash=identity, name=params.ti.name() if params.ti else params.name or "Resolving magnet…",
                   folder=str(folder), intent="selecting", created=int(time.time() * 1000), error=None,
                   sequential=False, priorities=[], seed_ratio=self.settings["seed_ratio"], confirmed=False)
        self.jobs[jid] = job
        self.persist(job)
        # Keep the metadata before acknowledging. Browser inbox files can now be
        # removed immediately, and deleting an original .torrent cannot break restart.
        params.save_path = str(folder)
        self.db.execute("UPDATE jobs SET resume=? WHERE id=?", (protected(bytes(lt.write_resume_data_buf(params))), jid))
        self.db.commit()
        self.attach(job, params)
        return self.view(job)

    def metadata(self, job):
        h = self.handles.get(job["id"])
        if not h or not h.is_valid() or not h.status().has_metadata:
            return None
        return checked_metadata(h.torrent_file())

    def view(self, job):
        h = self.handles.get(job["id"])
        st = h.status() if h and h.is_valid() else None
        ti = self.metadata(job) if st and st.has_metadata else None
        state = job["intent"]
        if st and job["intent"] == "running":
            state = "checking" if st.state in (lt.torrent_status.checking_files, lt.torrent_status.checking_resume_data) else "metadata" if not st.has_metadata else "seeding" if st.is_finished else "queued" if st.paused else "downloading"
        if job["id"] in self.pending_moves:
            state = "moving"
        disk_ready = job["id"] in self.finished and job["id"] not in self.checking
        if job["id"] in self.checking:
            state = "checking" if job["intent"] == "running" else job["intent"]
        if st and st.is_finished and not disk_ready and job["intent"] == "running":
            state = "verifying"
        error = job.get("error") or ("torrent_transfer_error" if st and st.errc.value() else None)
        if error:
            state = "failed"
        return dict(id=job["id"], name=ti.name() if ti else job["name"], folder=job["folder"], state=state,
                    total_bytes=str(st.total_wanted) if ti else None, received_bytes=str(st.total_wanted_done) if st else "0",
                    download_speed=str(st.download_payload_rate) if st else "0", upload_speed=str(st.upload_payload_rate) if st else "0",
                    uploaded_bytes=str(st.all_time_upload) if st else "0", downloaded_bytes=str(st.all_time_download) if st else "0",
                    peers=st.num_peers if st else 0, seeds=st.num_seeds if st else 0,
                    progress=min(st.progress, 0.999999) if st and not disk_ready else st.progress if st else 0, error=error, has_metadata=bool(ti),
                    info_hash=job["hash"], sequential=job["sequential"], seed_ratio=job["seed_ratio"],
                    version="hybrid" if ti and ti.info_hashes().has_v1() and ti.info_hashes().has_v2() else "v2" if ti and ti.info_hashes().has_v2() else "v1",
                    private=ti.priv() if ti else False)

    def details(self, job, offset=0, limit=500):
        if not 0 <= offset <= 100_000 or not 1 <= limit <= 1000:
            raise ValueError("invalid_file_page")
        h = self.handles.get(job["id"])
        ti = self.metadata(job)
        files = []
        if ti:
            fs = ti.files()
            progress = h.file_progress()
            priorities = h.get_file_priorities()
            used = 0
            for i in range(offset,min(fs.num_files(),offset+limit)):
                file = dict(index=i,path=fs.file_path(i),size=str(fs.file_size(i)),received=str(progress[i]),priority=priorities[i],pad=bool(fs.file_flags(i) & lt.file_storage.flag_pad_file))
                used += len(json.dumps(file))
                if used > 1024*1024:
                    break
                files.append(file)
        def redacted_url(url):
            try:
                p = urllib.parse.urlsplit(url)
                return f"{p.scheme}://{p.hostname or ''}" + (f":{p.port}" if p.port else "")
            except ValueError:
                return "Tracker"
        trackers = [dict(url=redacted_url(t["url"]), tier=t["tier"], verified=t.get("verified",False)) for t in h.trackers()] if h else []
        peers = [dict(address=p.ip[0], port=p.ip[1], client=p.client.decode("utf-8",errors="replace") if isinstance(p.client,bytes) else p.client, down_speed=p.down_speed, up_speed=p.up_speed,
                      progress=p.progress, flags=int(p.flags)) for p in h.get_peer_info()][:200] if h else []
        return self.view(job) | dict(files=files,file_count=ti.files().num_files() if ti else 0,file_offset=offset,priorities=h.get_file_priorities() if ti else [],trackers=trackers, peers_detail=peers,
                                    magnet=bool(h and ti))

    def confirm(self, job, request):
        if job["intent"] != "selecting":
            return self.view(job)
        ti = self.metadata(job)
        if not ti:
            raise ValueError("torrent_metadata_pending")
        folder = Path(request["folder"]).resolve(strict=True)
        if not folder.is_dir():
            raise ValueError("invalid_download_directory")
        for i in range(ti.files().num_files()):
            file = folder / ti.files().file_path(i)
            if not file.resolve().is_relative_to(folder):
                raise ValueError("unsafe_torrent_path")
            if file.exists() and not request.get("reuse_existing", False):
                raise ValueError("torrent_files_exist")
        priorities = request.get("priorities") or [4] * ti.files().num_files()
        self.priorities(job, priorities)
        job["folder"] = str(folder)
        job["confirmed"] = True
        job["recheck_after_move"] = bool(request.get("reuse_existing",False))
        job["intent"] = "running" if request.get("start", True) else "paused"
        self.persist(job)
        h = self.handles[job["id"]]
        h.pause()
        self.pending_moves[job["id"]] = True
        h.move_storage(str(folder), lt.move_flags_t.dont_replace)
        return self.view(job)

    def priorities(self, job, priorities):
        ti = self.metadata(job)
        if not ti or len(priorities) != ti.files().num_files() or any(type(p) is not int or not 0 <= p <= 7 for p in priorities) or not any(priorities):
            raise ValueError("invalid_file_selection")
        self.handles[job["id"]].prioritize_files(priorities)
        job["priorities"] = priorities
        self.persist(job)

    def save_resume(self, job):
        h = self.handles.get(job["id"])
        if h and h.is_valid() and job["id"] in self.pending_resumes:
            self.resume_again.add(job["id"])
        elif h and h.is_valid():
            self.pending_resumes.add(job["id"])
            h.save_resume_data(lt.save_resume_flags_t.flush_disk_cache | lt.save_resume_flags_t.save_info_dict)

    def control(self, job, action, request):
        h = self.handles.get(job["id"])
        if not h and action in ("start", "resume", "retry", "delete") and job["confirmed"] and job["intent"] != "removed":
            row = self.db.execute("SELECT resume FROM jobs WHERE id=?", (job["id"],)).fetchone()
            params = lt.read_resume_data(protected(row[0], True)) if row and row[0] else load_source(job["source"])
            h = self.attach(job, params)
        if action in ("cancel", "remove", "delete"):
            if action == "delete" and request.get("confirmed") is not True:
                raise ValueError("confirmation_required")
            job["intent"] = "deleting" if action == "delete" else "removed" if action == "remove" else "canceled"
            self.persist(job)
            if h and action == "cancel":
                h.pause()
                self.save_resume(job)
            elif h:
                h.pause()
                self.session.remove_torrent(h, lt.options_t.delete_files if action == "delete" else 0)
                if action != "delete":
                    del self.handles[job["id"]]
                    self.pending_resumes.discard(job["id"])
            elif action == "delete":
                raise ValueError("torrent_files_unavailable")
            return self.view(job)
        if not h:
            raise ValueError("torrent_not_active")
        if action == "pause":
            job["intent"] = "paused"
            self.persist(job)
            h.pause()
        elif action in ("start", "resume", "retry"):
            if not job["confirmed"]:
                raise ValueError("torrent_location_not_confirmed")
            job["error"] = None
            job["intent"] = "running"
            self.persist(job)
            h.clear_error()
            h.resume()
        elif action == "recheck":
            self.finished.discard(job["id"])
            self.checking.add(job["id"])
            self.checking_started.discard(job["id"])
            h.force_recheck()
        elif action == "reannounce":
            h.force_reannounce()
            h.force_dht_announce()
        elif action == "sequential":
            job["sequential"] = bool(request.get("enabled"))
            h.set_flags(lt.torrent_flags.sequential_download) if job["sequential"] else h.unset_flags(lt.torrent_flags.sequential_download)
            self.persist(job)
        elif action == "priorities":
            self.priorities(job, request["priorities"])
        elif action == "tracker":
            url = request["url"]
            if urllib.parse.urlsplit(url).scheme not in ("http", "https", "udp") or len(url) > 4096:
                raise ValueError("invalid_tracker")
            h.add_tracker({"url": url, "tier": 0})
            h.force_reannounce()
        elif action == "webseed":
            if urllib.parse.urlsplit(request["url"]).scheme not in ("http", "https"):
                raise ValueError("invalid_webseed")
            h.add_url_seed(request["url"])
        elif action == "peer":
            address = request["address"]
            import ipaddress
            ipaddress.ip_address(address)
            port = int(request["port"])
            if not 0 < port < 65536:
                raise ValueError("invalid_peer")
            h.connect_peer((address, port))
        elif action == "limits":
            down, up = int(request["download_limit"]), int(request["upload_limit"])
            ratio = float(request["seed_ratio"])
            if not 0 <= down <= 2**31-1 or not 0 <= up <= 2**31-1 or not 0 <= ratio <= 1000:
                raise ValueError("invalid_torrent_limits")
            h.set_download_limit(down)
            h.set_upload_limit(up)
            job["seed_ratio"] = ratio
            self.persist(job)
        elif action == "relocate":
            if request.get("confirmed") is not True:
                raise ValueError("confirmation_required")
            folder = Path(request["folder"]).resolve(strict=True)
            ti = self.metadata(job)
            if not ti or not folder.is_dir():
                raise ValueError("torrent_metadata_pending")
            if any((folder / ti.files().file_path(i)).exists() for i in range(ti.files().num_files())):
                raise ValueError("torrent_files_exist")
            h.pause()
            self.pending_moves[job["id"]] = True
            h.move_storage(str(folder), lt.move_flags_t.fail_if_exist)
        elif action == "export":
            params = lt.add_torrent_params()
            params.ti = self.metadata(job)
            if not params.ti:
                raise ValueError("torrent_metadata_pending")
            params.trackers = [t["url"] for t in h.trackers()]
            params.url_seeds = list(h.url_seeds())
            output = Path(request["path"])
            with output.open("xb") as f:
                f.write(lt.bencode(lt.write_torrent_file(params)))
        elif action == "magnet":
            return {"magnet": lt.make_magnet_uri(h)}
        else:
            raise ValueError("invalid_torrent_action")
        if action != "peer":
            self.save_resume(job)
        return self.view(job)

    def create(self, request):
        source = Path(request["path"]).resolve(strict=True)
        storage = lt.file_storage()
        lt.add_files(storage, str(source))
        mode = request.get("version", "hybrid")
        flags = {"v1": lt.create_torrent.v1_only, "v2": lt.create_torrent.v2_only, "hybrid": 0}.get(mode)
        if flags is None:
            raise ValueError("invalid_torrent_version")
        creator = lt.create_torrent(storage, 0, flags)
        creator.set_creator("IDMXL")
        creator.set_priv(bool(request.get("private", False)))
        for index, tracker in enumerate(request.get("trackers", [])):
            if urllib.parse.urlsplit(tracker).scheme not in ("http", "https", "udp"):
                raise ValueError("invalid_tracker")
            creator.add_tracker(tracker, index)
        lt.set_piece_hashes(creator, str(source.parent))
        data = lt.bencode(creator.generate())
        checked_metadata(lt.torrent_info(data))
        output = Path(request["output"])
        with output.open("xb") as f:
            f.write(data)
        return {"path": str(output), "bytes": len(data)}

    def dispatch(self, request):
        kind = request.get("kind")
        if kind == "list":
            return {"jobs": [self.view(j) for j in self.jobs.values() if j["intent"] != "removed"],
                    "settings": self.settings, "version": lt.version, "listen_port": self.session.listen_port()}
        if kind == "inspect":
            return self.inspect(request["source"])
        if kind == "create":
            return self.create(request)
        if kind == "settings":
            candidate = self.settings | request["settings"]
            if set(candidate) != set(DEFAULTS):
                raise ValueError("invalid_torrent_settings")
            for key in ("download_limit", "upload_limit"):
                if type(candidate[key]) is not int or not 0 <= candidate[key] <= 2**31-1:
                    raise ValueError("invalid_torrent_settings")
            for key, low, high in (("connections", 10, 2000), ("per_torrent_connections", 5, 500), ("active_downloads", 1, 20), ("active_seeds", 1, 50), ("listen_port", 0, 65535)):
                if type(candidate[key]) is not int or not low <= candidate[key] <= high:
                    raise ValueError("invalid_torrent_settings")
            if not 0 <= candidate["seed_ratio"] <= 1000 or candidate["encryption"] not in ("enabled", "forced", "disabled"):
                raise ValueError("invalid_torrent_settings")
            if candidate["listen_address"] != "all":
                import ipaddress
                ipaddress.ip_address(candidate["listen_address"])
            self.settings = candidate
            self.db.execute("INSERT OR REPLACE INTO settings VALUES('preferences',?)", (json.dumps(candidate),))
            self.db.commit()
            self.session.apply_settings(self.session_settings())
            for h in self.handles.values():
                h.set_max_connections(candidate["per_torrent_connections"])
                h.unset_flags(lt.torrent_flags.disable_pex) if candidate["pex"] else h.set_flags(lt.torrent_flags.disable_pex)
            return self.settings
        if kind == "shutdown":
            self.stopping = True
            return {"stopping": True}
        job = self.jobs.get(request.get("id"))
        if not job:
            raise ValueError("torrent_not_found")
        if kind == "details":
            return self.details(job,int(request.get("offset",0)),int(request.get("limit",500)))
        if kind == "confirm":
            return self.confirm(job, request)
        if kind == "control":
            return self.control(job, request["action"], request)
        raise ValueError("invalid_torrent_message")

    def tick(self):
        for alert in self.session.pop_alerts():
            h = getattr(alert, "handle", None)
            jid = next((jid for jid, candidate in self.handles.items() if candidate == h), None)
            if not jid:
                continue
            job = self.jobs[jid]
            if isinstance(alert, lt.save_resume_data_alert):
                self.db.execute("UPDATE jobs SET resume=? WHERE id=?", (protected(bytes(lt.write_resume_data_buf(alert.params))), jid))
                self.db.commit()
                self.pending_resumes.discard(jid)
                if jid in self.resume_again:
                    self.resume_again.discard(jid)
                    self.save_resume(job)
            elif isinstance(alert, lt.save_resume_data_failed_alert):
                self.pending_resumes.discard(jid)
                self.resume_again.discard(jid)
                if job["confirmed"]:
                    self.checkpoint_failed = True
            elif isinstance(alert, lt.cache_flushed_alert) and jid in self.finishing:
                self.finishing.discard(jid)
                self.finished.add(jid)
            elif isinstance(alert, lt.state_changed_alert) and alert.state == lt.torrent_status.checking_files:
                self.checking_started.add(jid)
            elif isinstance(alert, lt.torrent_checked_alert) and jid in self.checking_started:
                self.checking.discard(jid)
                self.checking_started.discard(jid)
            elif isinstance(alert, lt.torrent_deleted_alert):
                job["intent"] = "removed"
                self.persist(job)
                self.handles.pop(jid,None)
                self.pending_resumes.discard(jid)
            elif isinstance(alert, lt.torrent_delete_failed_alert):
                job["intent"] = "paused"
                job["error"] = "torrent_file_deletion_failed"
                self.persist(job)
                self.handles.pop(jid,None)
            elif isinstance(alert, lt.metadata_received_alert):
                try:
                    ti = self.metadata(job)
                    job["name"] = ti.name()
                    self.persist(job)
                    self.save_resume(job)
                except Exception:
                    job["error"] = "unsafe_torrent_metadata"
                    job["intent"] = "paused"
                    h.pause()
                    self.session.remove_torrent(h,0)
                    self.handles.pop(jid,None)
                    self.persist(job)
            elif isinstance(alert, lt.storage_moved_alert):
                job["folder"] = h.status().save_path
                self.pending_moves.pop(jid, None)
                self.persist(job)
                if job["confirmed"]:
                    h.unset_flags(lt.torrent_flags.upload_mode)
                if job.pop("recheck_after_move",False):
                    self.checking.add(jid)
                    self.checking_started.discard(jid)
                    h.force_recheck()
                    self.persist(job)
                if job["intent"] == "running":
                    h.resume()
                self.save_resume(job)
            elif isinstance(alert, lt.storage_moved_failed_alert):
                self.pending_moves.pop(jid, None)
                job["folder"] = h.status().save_path
                job["error"] = "torrent_move_failed"
                job["intent"] = "paused"
                h.pause()
                self.persist(job)
            elif isinstance(alert, (lt.torrent_finished_alert, lt.torrent_paused_alert)):
                self.save_resume(job)
            elif isinstance(alert, (lt.file_error_alert, lt.torrent_error_alert)):
                job["error"] = "torrent_storage_or_transfer_error"
                job["intent"] = "paused"
                h.pause()
                self.persist(job)
        now = time.monotonic()
        if not self.closing and now - self.last_save >= 10:
            for job in self.jobs.values():
                self.save_resume(job)
            self.last_save = now
        if now - self.last_queue >= 1:
            downloads = seeds = 0
            for job in sorted(self.jobs.values(), key=lambda j: j["created"]):
                h = self.handles.get(job["id"])
                if not h or not h.is_valid() or job["id"] in self.pending_moves:
                    continue
                st = h.status()
                if job["id"] in self.checking or st.state in (lt.torrent_status.checking_files, lt.torrent_status.checking_resume_data):
                    continue
                if st.is_finished and job["intent"] != "selecting" and job["id"] not in self.finished:
                    if job["id"] not in self.finishing:
                        self.finishing.add(job["id"])
                        h.flush_cache()
                    continue
                if job["intent"] != "running":
                    continue
                if st.is_finished:
                    ratio = st.all_time_upload / max(1, st.total_wanted)
                    if ratio >= job["seed_ratio"]:
                        job["intent"] = "completed"
                        h.pause()
                        self.persist(job)
                        self.save_resume(job)
                        continue
                    seeds += 1
                    allowed = seeds <= self.settings["active_seeds"]
                else:
                    self.finished.discard(job["id"])
                    downloads += 1
                    allowed = downloads <= self.settings["active_downloads"]
                if allowed and st.paused:
                    h.resume()
                elif not allowed and not st.paused:
                    h.pause()
            self.last_queue = now

    def close(self):
        self.closing = True
        self.session.pause()
        for job in self.jobs.values():
            self.save_resume(job)
        end = time.monotonic() + 10
        while self.pending_resumes and time.monotonic() < end:
            self.tick()
            time.sleep(0.05)
        self.db.execute("INSERT OR REPLACE INTO settings VALUES('clean_shutdown',?)",("false" if self.pending_resumes or self.checkpoint_failed else "true",))
        self.db.commit()
        self.db.close()


def receive(sock, size):
    data = bytearray()
    while len(data) < size:
        part = sock.recv(size - len(data))
        if not part:
            raise ValueError("truncated_torrent_message")
        data.extend(part)
    return bytes(data)


def serve(directory: Path):
    directory.mkdir(parents=True, exist_ok=True)
    lock = (directory / "worker.lock").open("a+b")
    if os.name == "nt":
        import msvcrt
        lock.seek(0)
        lock.write(b"0")
        lock.flush()
        lock.seek(0)
        try:
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError:
            return
    worker = Worker(directory)
    token = secrets.token_hex(32)
    descriptor = directory / "worker.json"
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server:
        server.bind(("127.0.0.1", 0))
        server.listen(8)
        server.settimeout(0.2)
        atomic(descriptor, json.dumps(dict(port=server.getsockname()[1], token=token, pid=os.getpid())).encode())
        try:
            while not worker.stopping:
                worker.tick()
                try:
                    conn, _ = server.accept()
                except TimeoutError:
                    continue
                with conn:
                    conn.settimeout(3)
                    try:
                        size = struct.unpack("<I", receive(conn, 4))[0]
                        if not 0 < size <= MAX_FRAME:
                            raise ValueError("invalid_torrent_message")
                        request = json.loads(receive(conn, size))
                        if not hmac.compare_digest(str(request.pop("token", "")), token):
                            raise ValueError("torrent_authentication_failed")
                        response = dict(ok=True, result=worker.dispatch(request))
                    except Exception as error:
                        known = str(error)
                        response = dict(ok=False, error=known if known.replace("_", "").isalnum() and len(known) < 80 else "torrent_operation_failed")
                    try:
                        encoded = json.dumps(response, ensure_ascii=False).encode()
                    except (TypeError,ValueError):
                        encoded = b'{"ok":false,"error":"torrent_response_encoding_failed"}'
                    if len(encoded) > MAX_FRAME:
                        encoded = b'{"ok":false,"error":"torrent_response_too_large"}'
                    try:
                        conn.sendall(struct.pack("<I", len(encoded)) + encoded)
                    except OSError:
                        pass
        finally:
            worker.close()
            descriptor.unlink(missing_ok=True)
            lock.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--state", type=Path, required=True)
    args = parser.parse_args()
    try:
        serve(args.state)
    except Exception as error:
        # Never let the windowed packager open a crash dialog unattended. Do not
        # write exception text: it may include tracker passkeys or source paths.
        args.state.mkdir(parents=True,exist_ok=True)
        atomic(args.state / "worker-error.json",json.dumps({"error":"torrent_worker_stopped","type":type(error).__name__}).encode())
        raise SystemExit(1)
