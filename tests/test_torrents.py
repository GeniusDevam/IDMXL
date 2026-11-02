"""Real local BitTorrent peers, generated data only; no external trackers or payloads."""
import gc
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "torrent"))
try:
    import libtorrent as lt
    import service
except ImportError:
    lt = service = None

LOCAL = dict(listen_interfaces="127.0.0.1:0", enable_dht=False, enable_lsd=False,
             enable_upnp=False, enable_natpmp=False, enable_outgoing_utp=False,
             enable_incoming_utp=False, ignore_limits_on_local_network=False)


@unittest.skipUnless(lt and os.environ.get("IDMXL_SKIP_TORRENTS")!="1", "Run the separate torrent qualification suite")
class Torrents(unittest.TestCase):
    def setUp(self):
        (ROOT/".test-output").mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="idmxl-torrent-",dir=ROOT/".test-output")
        self.root = Path(self.temp.name)
        self.seed_dir = self.root / "seed"
        self.seed_dir.mkdir()
        self.out = self.root / "out"
        self.out.mkdir()
        self.worker = service.Worker(self.root / "state", LOCAL)
        self.seed = None
        self.process = None

    def tearDown(self):
        if self.process and self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=10)
        if self.worker:
            self.worker.close()
            self.worker.handles.clear()
            self.worker.session = None
            self.worker = None
        if self.seed:
            self.seed.pause()
            self.seed = None
        gc.collect()
        for attempt in range(30):
            try:
                self.temp.cleanup()
                break
            except PermissionError:
                time.sleep(.1)

    def generate(self, version="hybrid", multiple=False, size=16*1024*1024):
        path = self.seed_dir / ("bundle" if multiple else "fixture.bin")
        if multiple:
            path.mkdir()
            (path / "wanted.bin").write_bytes(hashlib.shake_256(b"wanted").digest(size))
            (path / "skip.bin").write_bytes(hashlib.shake_256(b"skip").digest(size//2))
        else:
            path.write_bytes(hashlib.shake_256(version.encode()).digest(size))
        torrent = self.root / "fixture.torrent"
        self.worker.create(dict(path=str(path), output=str(torrent), version=version,
                                trackers=["http://127.0.0.1:1/announce?passkey=private-test-secret"]))
        return torrent, path

    def start_seed(self, torrent, limit=8*1024*1024, utp=False):
        self.seed = lt.session(LOCAL | dict(upload_rate_limit=limit,enable_incoming_utp=utp,enable_outgoing_utp=utp,allow_multiple_connections_per_ip=True))
        params = lt.load_torrent_file(str(torrent))
        params.save_path = str(self.seed_dir)
        params.flags &= ~lt.torrent_flags.auto_managed
        params.flags &= ~lt.torrent_flags.paused
        self.seed.add_torrent(params)

    def wait(self, predicate, seconds=45, peer=None):
        end = time.monotonic()+seconds
        next_peer = 0
        while time.monotonic()<end:
            self.worker.tick()
            if peer and time.monotonic()>next_peer:
                self.worker.handles[peer].connect_peer(("127.0.0.1",self.seed.listen_port()))
                next_peer=time.monotonic()+1
            value=predicate()
            if value:
                return value
            time.sleep(.05)
        self.fail("Local torrent fixture did not reach expected state")

    def test_v1_v2_and_hybrid_metadata_creation_and_encrypted_state(self):
        for version in ("v1","v2","hybrid"):
            with self.subTest(version=version):
                torrent,path=self.generate(version)
                view=self.worker.inspect(str(torrent))
                job=self.worker.jobs[view["id"]]
                detail=self.worker.details(job)
                self.assertEqual(detail["version"],version)
                self.assertEqual(detail["state"],"selecting")
                self.assertEqual(detail["received_bytes"],"0")
                self.assertEqual(len(detail["files"]),1)
                self.assertNotIn("private-test-secret",json.dumps(detail))
                self.worker.control(job,"remove",{})
                torrent.unlink();path.unlink()
        if os.name=="nt":
            for p in (self.root/"state").glob("torrents.db*"):
                self.assertNotIn(b"private-test-secret",p.read_bytes())

    def test_confirmed_folder_conflicts_and_delete_confirmation(self):
        torrent,path=self.generate()
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        output=self.out/path.name;output.write_bytes(b"keep this existing file")
        with self.assertRaisesRegex(ValueError,"torrent_files_exist"):
            self.worker.confirm(job,dict(folder=str(self.out)))
        self.assertEqual(output.read_bytes(),b"keep this existing file")
        with self.assertRaisesRegex(ValueError,"confirmation_required"):
            self.worker.control(job,"delete",{})
        self.assertEqual(output.read_bytes(),b"keep this existing file")
        with self.assertRaisesRegex(ValueError,"torrent_location_not_confirmed"):
            self.worker.control(job,"start",{})

    def test_real_v2_peer_transfer_and_hash(self):
        torrent,path=self.generate("v2")
        self.start_seed(torrent)
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:self.worker.view(job)["progress"]==1,peer=job["id"])
        self.assertEqual(hashlib.sha256((self.out/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())
        self.worker.control(job,"pause",{})
        self.wait(lambda:self.worker.handles[job["id"]].status().paused)
        self.assertEqual(self.worker.view(job)["state"],"paused")
        self.worker.control(job,"resume",{})
        self.assertEqual(job["intent"],"running")

    def test_multifile_selection_skips_unwanted_data_and_delete_is_scoped(self):
        torrent,path=self.generate("v1",multiple=True,size=8*1024*1024)
        self.start_seed(torrent)
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        files=self.worker.details(job)["files"]
        priorities=[0 if f["path"].endswith("skip.bin") else 4 for f in files]
        self.worker.confirm(job,dict(folder=str(self.out),priorities=priorities))
        self.wait(lambda:self.worker.view(job)["progress"]==1,peer=job["id"])
        self.assertEqual(hashlib.sha256((self.out/"bundle/wanted.bin").read_bytes()).hexdigest(),hashlib.sha256((path/"wanted.bin").read_bytes()).hexdigest())
        unrelated=self.out/"unrelated.txt";unrelated.write_text("keep")
        self.worker.control(job,"delete",dict(confirmed=True))
        self.wait(lambda:not (self.out/"bundle/wanted.bin").exists())
        self.assertEqual(unrelated.read_text(),"keep")

    def test_magnet_metadata_does_not_download_payload_before_confirmation(self):
        torrent,path=self.generate("hybrid")
        self.start_seed(torrent)
        uri=lt.make_magnet_uri(lt.torrent_info(str(torrent)))
        view=self.worker.inspect(uri);job=self.worker.jobs[view["id"]]
        self.wait(lambda:self.worker.view(job)["has_metadata"],peer=job["id"])
        for _ in range(15):self.worker.tick();time.sleep(.05)
        self.assertEqual(self.worker.view(job)["received_bytes"],"0")
        self.assertEqual(self.worker.view(job)["state"],"selecting")
        self.assertFalse(list(self.out.iterdir()))
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:self.worker.view(job)["progress"]==1,peer=job["id"])
        self.assertEqual(hashlib.sha256((self.out/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())

    def test_pause_restart_preserves_progress_selection_and_intent_without_original_torrent(self):
        torrent,path=self.generate("hybrid",size=32*1024*1024)
        self.start_seed(torrent,limit=2*1024*1024)
        self.seed.apply_settings({"alert_mask":int(lt.alert.category_t.all_categories)})
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:int(self.worker.view(job)["received_bytes"])>=3*1024*1024,peer=job["id"])
        self.worker.control(job,"pause",{})
        self.wait(lambda:self.worker.handles[job["id"]].status().paused)
        self.worker.close();self.worker.handles.clear();self.worker.session=None;self.worker=None;gc.collect()
        db=sqlite3.connect(self.root/"state/torrents.db")
        resume=db.execute("SELECT resume FROM jobs WHERE id=?",(job["id"],)).fetchone()[0];db.close()
        checkpoint=lt.read_resume_data(service.protected(resume,True))
        verified=sum(checkpoint.ti.piece_size(i) for i,have in enumerate(checkpoint.have_pieces) if have)
        self.assertGreater(verified,0)
        self.assertFalse(checkpoint.flags & lt.torrent_flags.upload_mode)
        torrent.unlink()
        self.worker=service.Worker(self.root/"state",LOCAL)
        restored=self.worker.jobs[job["id"]]
        self.wait(lambda:int(self.worker.view(restored)["received_bytes"])>=verified)
        self.assertEqual(self.worker.view(restored)["state"],"paused")
        self.worker.control(restored,"retry",{})
        self.wait(lambda:self.worker.view(restored)["progress"]==1,peer=job["id"])
        self.assertEqual(hashlib.sha256((self.out/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())

    def test_settings_and_sources_are_validated(self):
        with self.assertRaisesRegex(ValueError,"invalid_torrent_settings"):
            self.worker.dispatch(dict(kind="settings",settings=dict(active_downloads=0)))
        for source in ("magnet:?xt=bad", "not-a-torrent"):
            with self.assertRaises(Exception):self.worker.inspect(source)

    def test_completed_torrent_history_stays_complete_after_restart(self):
        torrent,path=self.generate("v1",size=2*1024*1024)
        self.start_seed(torrent)
        self.worker.settings["seed_ratio"]=0
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:self.worker.view(job)["state"]=="completed",peer=job["id"])
        self.worker.close();self.worker.handles.clear();self.worker.session=None;self.worker=None;gc.collect()
        self.worker=service.Worker(self.root/"state",LOCAL)
        restored=self.worker.jobs[job["id"]]
        self.wait(lambda:self.worker.view(restored)["progress"]==1)
        self.assertEqual(self.worker.view(restored)["state"],"completed")
        self.assertEqual(hashlib.sha256((self.out/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())

    def test_unclean_restart_rechecks_and_repairs_corrupted_pieces(self):
        torrent,path=self.generate("hybrid",size=16*1024*1024)
        self.start_seed(torrent)
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:self.worker.view(job)["progress"]==1,peer=job["id"])
        self.worker.control(job,"pause",{})
        self.worker.close();self.worker.handles.clear();self.worker.session=None;self.worker=None;gc.collect()
        output=self.out/path.name
        with output.open("r+b") as f:f.seek(1024*1024);f.write(b"damaged piece"*1024)
        db=sqlite3.connect(self.root/"state/torrents.db")
        db.execute("UPDATE settings SET value='false' WHERE key='clean_shutdown'");db.commit();db.close()
        self.worker=service.Worker(self.root/"state",LOCAL)
        restored=self.worker.jobs[job["id"]]
        self.worker.control(restored,"resume",{})
        self.wait(lambda:self.worker.view(restored)["progress"]==1,peer=job["id"])
        self.assertEqual(hashlib.sha256(output.read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())

    def test_relocation_export_and_cancel_retry_preserve_bytes(self):
        torrent,path=self.generate("v1",size=8*1024*1024)
        self.start_seed(torrent)
        view=self.worker.inspect(str(torrent));job=self.worker.jobs[view["id"]]
        self.worker.confirm(job,dict(folder=str(self.out)))
        self.wait(lambda:self.worker.view(job)["progress"]==1,peer=job["id"])
        target=self.root/"moved";target.mkdir()
        self.worker.control(job,"relocate",dict(folder=str(target),confirmed=True))
        self.wait(lambda:job["id"] not in self.worker.pending_moves)
        self.assertEqual(hashlib.sha256((target/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())
        export=self.root/"exported.torrent"
        self.worker.control(job,"export",dict(path=str(export)))
        self.assertEqual(str(lt.torrent_info(str(export)).info_hashes().v1),job["hash"])
        self.worker.control(job,"cancel",{})
        self.assertEqual(job["intent"],"canceled")
        self.worker.control(job,"retry",{})
        self.wait(lambda:self.worker.view(job)["progress"]==1)
        self.assertEqual(self.worker.view(job)["folder"],str(target))

    def test_packaged_worker_authentication_process_loss_and_resume(self):
        self.packaged_recovery(False)

    def test_packaged_worker_utp_transfer_and_process_loss(self):
        self.packaged_recovery(True)

    def packaged_recovery(self, utp):
        binary=ROOT/"apps/desktop/src-tauri/torrent/idmxl-torrent.exe"
        if not binary.is_file():self.skipTest("Stage the packaged torrent worker")
        torrent,path=self.generate("hybrid",size=24*1024*1024)
        self.start_seed(torrent,limit=2*1024*1024,utp=utp)
        self.seed.apply_settings({"alert_mask":int(lt.alert.category_t.error_notification|lt.alert.category_t.peer_notification|lt.alert.category_t.connect_notification|lt.alert.category_t.status_notification)})
        state=self.root/"packaged-state"
        def launch():
            command=[sys.executable,str(ROOT/"torrent/service.py")] if os.environ.get("IDMXL_TORRENT_TEST_PYTHON") else [str(binary)]
            self.process=subprocess.Popen([*command,"--state",str(state)],creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
            end=time.monotonic()+15
            while time.monotonic()<end:
                try:
                    endpoint=json.loads((state/"worker.json").read_text())
                    if endpoint["pid"]==self.process.pid:return endpoint
                except (OSError,ValueError):pass
                time.sleep(.1)
            self.fail("Packaged worker did not start")
        endpoint=launch()
        def rpc(value,token=None):
            data=json.dumps(dict(token=endpoint["token"] if token is None else token,**value)).encode()
            with socket.create_connection(("127.0.0.1",endpoint["port"]),timeout=10) as sock:
                sock.sendall(struct.pack("<I",len(data))+data)
                size=struct.unpack("<I",service.receive(sock,4))[0]
                return json.loads(service.receive(sock,size))
        self.assertFalse(rpc(dict(kind="list"),token="wrong")["ok"])
        self.assertTrue(rpc(dict(kind="settings",settings=dict(dht=False,lsd=False,port_mapping=False,utp=utp,seed_ratio=0,listen_address="127.0.0.1")))["ok"])
        result=rpc(dict(kind="inspect",source=str(torrent)))
        self.assertTrue(result["ok"],result)
        jid=result["result"]["id"]
        self.assertTrue(rpc(dict(kind="confirm",id=jid,folder=str(self.out)))["ok"])
        def snapshot():return rpc(dict(kind="details",id=jid))["result"]
        end=time.monotonic()+45
        peer_alerts=[]
        while time.monotonic()<end:
            reply=rpc(dict(kind="control",id=jid,action="peer",address="127.0.0.1",port=self.seed.listen_port()))
            self.assertTrue(reply["ok"],reply)
            peer_alerts.extend(a.message() for a in self.seed.pop_alerts())
            if int(snapshot()["received_bytes"])>=2*1024*1024:break
            time.sleep(.2)
        else:self.fail(f"Packaged worker did not receive peer data: {snapshot()} seed alerts {peer_alerts[-10:]}")
        self.assertTrue(rpc(dict(kind="control",id=jid,action="pause"))["ok"])
        time.sleep(1)
        db=sqlite3.connect(state/"torrents.db")
        resume=db.execute("SELECT resume FROM jobs WHERE id=?",(jid,)).fetchone()[0]
        self.assertTrue(any(lt.read_resume_data(service.protected(resume,True)).have_pieces))
        db.close()
        self.assertTrue(rpc(dict(kind="control",id=jid,action="resume"))["ok"])
        self.process.kill();self.process.wait(timeout=10)
        endpoint=launch()
        end=time.monotonic()+60
        while time.monotonic()<end:
            rpc(dict(kind="control",id=jid,action="peer",address="127.0.0.1",port=self.seed.listen_port()))
            peer_alerts.extend(a.message() for a in self.seed.pop_alerts())
            if snapshot()["progress"]==1:break
            time.sleep(.2)
        else:self.fail(f"Packaged worker did not recover from process loss: {snapshot()} seed alerts {peer_alerts[-20:]}")
        self.assertEqual(hashlib.sha256((self.out/path.name).read_bytes()).hexdigest(),hashlib.sha256(path.read_bytes()).hexdigest())
        db=sqlite3.connect(state/"torrents.db")
        self.assertEqual(db.execute("PRAGMA integrity_check").fetchone()[0],"ok");db.close()
        self.assertTrue(rpc(dict(kind="shutdown"))["ok"])
        self.process.wait(timeout=15)


if __name__=="__main__":unittest.main()
