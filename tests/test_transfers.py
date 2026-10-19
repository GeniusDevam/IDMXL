"""Deterministic end-to-end tests using only Python's standard library.

Build idmxl first, then: python -m unittest discover -s tests -v
No public site, account, external test service, or privileged port is used.
"""
import collections
import hashlib
import http.server
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
import unittest
import uuid
import struct
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
EXE = Path(os.environ.get("IDMXL_EXE", ROOT / "target" / "debug" / ("idmxl.exe" if os.name == "nt" else "idmxl")))
DATA = bytes(range(256)) * (3 * 4096) + b"IDMXL-end"
DIGEST = hashlib.sha256(DATA).hexdigest()


class Fixture(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    counters = collections.Counter()
    guard = threading.Lock()

    def log_message(self, *_):
        pass

    def do_GET(self):
        path = urlsplit(self.path).path
        byte_range = self.headers.get("Range", "")
        with self.guard:
            self.counters[(path, byte_range)] += 1
            attempt = self.counters[(path, byte_range)]
        if path == "/retry-crash" and byte_range not in ("", "bytes=0-0") and not getattr(self.server, "retry_healthy", False):
            self.send_response(429)
            self.send_header("Retry-After", "0")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/file")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path == "/missing":
            self.send_response(404)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path in ("/cooldown","/date-cooldown"):
            import email.utils
            self.send_response(429)
            self.send_header("Retry-After",email.utils.formatdate(time.time()+3600,usegmt=True) if path=="/date-cooldown" else "3600")
            self.send_header("Content-Length","0");self.end_headers();return
        if path == "/throttle" and byte_range and byte_range != "bytes=0-0" and attempt == 1:
            self.send_response(429)
            self.send_header("Retry-After", "0")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if byte_range and path != "/ignore":
            start, end = map(int, byte_range.removeprefix("bytes=").split("-"))
            end = min(end, len(DATA) - 1)
            if path == "/window-gated" and start > 0:
                # Keep a real unfinished lease until the test observes the window stop.
                # A fast runner must not finish the entire file before the control update.
                gate = getattr(self.server, "window_range_gate", None)
                if gate is not None:
                    gate.wait(timeout=20)
            body = DATA[start:end + 1]
            self.send_response(206)
            claimed_end = end + 1 if path == "/malformed" and byte_range != "bytes=0-0" else end
            self.send_header("Content-Range", f"bytes {start}-{claimed_end}/{len(DATA)}")
            tag = 'W/"v1"' if path == "/weak" else ('"v2"' if (path == "/changed" and start > 0) or (path == "/slow" and "changed=1" in self.path) else '"v1"')
            self.send_header("ETag", tag)
        else:
            body = DATA
            self.send_response(200)
        if path == "/encoding":
            self.send_header("Content-Encoding", "gzip")
        if path != "/unknown":
            self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.close_connection = True
        try:
            self.end_headers()
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError, OSError):
            return
        if path == "/truncate" and byte_range != "bytes=0-0" and attempt == 1:
            body = body[:len(body) // 2]
        try:
            for i in range(0, len(body), 32768):
                self.wfile.write(body[i:i + 32768])
                self.wfile.flush()
                if path == "/slow" and len(body) > 1:
                    time.sleep(getattr(self.server, "chunk_delay", 0.025))
        except (BrokenPipeError, ConnectionResetError, ConnectionAbortedError, OSError):
            pass


class TransferTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not EXE.is_file():
            raise RuntimeError("Build the CLI with cargo build --workspace first")
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        scratch = ROOT / ".test-output"
        scratch.mkdir(exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(prefix="idmxl-tests-", dir=scratch)
        self.directory = Path(self.temp.name)
        self.state = self.directory / "state"
        self.processes = []

    def tearDown(self):
        for process in self.processes:
            if process.poll() is None:
                process.kill()
            process.communicate(timeout=10)
        self.temp.cleanup()

    def command(self, *args, success=True, input=None):
        result = subprocess.run([str(EXE), "--state-dir", str(self.state), *args], input=input,
                                text=True, encoding="utf-8", capture_output=True, timeout=35)
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
            return json.loads(result.stdout)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def add(self, path="/file", replayable=True, name="result.bin", digest=DIGEST):
        args = ["add", "--url-stdin", "--output", str(self.directory / name)]
        if replayable:
            args.append("--replayable")
        if digest:
            args += ["--sha256", digest]
        return self.command(*args, input=self.base + path)

    def start(self, job, *options):
        process = subprocess.Popen([str(EXE), "--state-dir", str(self.state), "run", job["id"], *options],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.processes.append(process)
        return process

    def wait_checkpoint(self, job, process):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            current = self.command("show", job["id"])
            if int(current["durable_bytes"]) >= 1048576:
                return current
            self.assertIsNone(process.poll(), process.communicate() if process.poll() is not None else "")
            time.sleep(0.025)
        self.fail("checkpoint timeout")

    def assert_completed(self, job, expected_mode=None):
        result = self.command("run", job["id"])
        self.assertEqual(result["state"], "completed")
        self.assertEqual(result["sha256"], DIGEST)
        self.assertEqual(result["durable_bytes"], str(len(DATA)))
        self.assertEqual(hashlib.sha256(Path(result["destination"]).read_bytes()).hexdigest(), DIGEST)
        if expected_mode:
            self.assertEqual(result["mode"], expected_mode)
        if os.name == "nt":
            self.assertIn("ZoneId=3", Path(result["destination"] + ":Zone.Identifier").read_text())
        return result

    def test_conditional_parallel_ranges(self):
        self.assert_completed(self.add(), "ranged")

    def test_adaptive_ranges_keep_hash_and_identity_contract(self):
        job=self.add()
        result=self.command("run",job["id"],"--connections","4","--adaptive")
        self.assertEqual(result["state"],"completed");self.assertEqual(result["sha256"],DIGEST)
        self.assertEqual(hashlib.sha256(Path(result["destination"]).read_bytes()).hexdigest(),DIGEST)

    def test_adaptive_flag_never_replays_a_one_use_source(self):
        with Fixture.guard:before=sum(n for (path,_),n in Fixture.counters.items() if path=="/unknown")
        job=self.add("/unknown",replayable=False)
        result=self.command("run",job["id"],"--connections","4","--adaptive")
        self.assertEqual(result["sha256"],DIGEST)
        with Fixture.guard:after=sum(n for (path,_),n in Fixture.counters.items() if path=="/unknown")
        self.assertEqual(after-before,1)

    def test_adaptive_ranges_still_reject_changed_resource(self):
        job=self.add("/changed")
        self.command("run",job["id"],"--connections","4","--adaptive",success=False)
        self.assertFalse(Path(job["destination"]).exists())
        self.assertEqual(self.command("show",job["id"])["error"],"resource_changed")

    def test_range_ignored_falls_back_to_sequential(self):
        self.assert_completed(self.add("/ignore"), "sequential")

    def test_weak_validator_is_not_used_for_resume(self):
        self.assert_completed(self.add("/weak"), "sequential")

    def test_non_replayable_unknown_length_uses_one_request(self):
        with Fixture.guard:
            before = sum(n for (path, _), n in Fixture.counters.items() if path == "/unknown")
        self.assert_completed(self.add("/unknown", replayable=False), "sequential")
        with Fixture.guard:
            after = sum(n for (path, _), n in Fixture.counters.items() if path == "/unknown")
        self.assertEqual(after - before, 1)

    def test_redirect_is_explicitly_validated(self):
        self.assert_completed(self.add("/redirect", replayable=False), "sequential")

    def test_changed_resource_never_publishes(self):
        job = self.add("/changed")
        self.command("run", job["id"], success=False)
        self.assertEqual(self.command("show", job["id"])["error"], "resource_changed")
        self.assertFalse(Path(job["destination"]).exists())

    def test_bad_range_rejected_before_publication(self):
        job = self.add("/malformed")
        self.command("run", job["id"], success=False)
        self.assertEqual(self.command("show", job["id"])["error"], "invalid_content_range")
        self.assertFalse(Path(job["destination"]).exists())

    def test_unexpected_encoding_is_rejected(self):
        job = self.add("/encoding")
        self.command("run", job["id"], success=False)
        self.assertEqual(self.command("show", job["id"])["error"], "encoded_response_unsupported")

    def test_truncated_ranges_retry_without_corruption(self):
        self.assert_completed(self.add("/truncate"), "ranged")

    def test_retry_after_throttle(self):
        self.assert_completed(self.add("/throttle"), "ranged")

    def test_pause_and_resume_reuses_verified_extents(self):
        job = self.add("/slow")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        self.command("pause", job["id"])
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 0, stderr)
        self.assertEqual(json.loads(stdout)["state"], "paused")
        with Fixture.guard:
            first_block_before = Fixture.counters[("/slow", "bytes=0-1048575")]
        self.assert_completed(job, "ranged")
        with Fixture.guard:
            self.assertEqual(Fixture.counters[("/slow", "bytes=0-1048575")], first_block_before)

    def test_process_kill_recovers_from_checkpoint(self):
        job = self.add("/slow")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        process.kill()
        process.communicate(timeout=10)
        self.assert_completed(job, "ranged")

    def test_corrupt_local_extent_is_redownloaded(self):
        job = self.add("/slow")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        self.command("pause", job["id"])
        process.communicate(timeout=10)
        staging = next(self.directory.glob(".idmxl-*.part"))
        with staging.open("r+b") as stream:
            stream.write(b"CORRUPTED")
        self.assert_completed(job, "ranged")

    def test_destination_collision_does_not_overwrite_user_file(self):
        job = self.add("/slow")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        destination = Path(job["destination"])
        destination.write_bytes(b"my existing file")
        process.communicate(timeout=15)
        self.assertNotEqual(process.returncode, 0)
        self.assertEqual(destination.read_bytes(), b"my existing file")
        self.assertEqual(self.command("show", job["id"])["state"], "publishing")
        self.command("run", job["id"], success=False)
        self.assertEqual(destination.read_bytes(), b"my existing file")

    def test_digest_mismatch_keeps_partial_and_never_publishes(self):
        job = self.add(digest="0" * 64)
        self.command("run", job["id"], success=False)
        self.assertEqual(self.command("show", job["id"])["error"], "digest_mismatch")
        self.assertFalse(Path(job["destination"]).exists())
        self.assertTrue(list(self.directory.glob(".idmxl-*.part")))

    def test_one_engine_instance_per_state_directory(self):
        first = self.add("/slow")
        second = self.add(name="second.bin")
        process = self.start(first, "--connections", "1")
        self.wait_checkpoint(first, process)
        result = self.command("run", second["id"], success=False)
        self.assertIn("engine_busy", result.stderr)
        self.command("pause", first["id"])
        process.communicate(timeout=10)

    def test_cancel_keeps_user_files_and_blocks_resume(self):
        job = self.add("/slow")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        self.command("cancel", job["id"])
        stdout, stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode, 0, stderr)
        self.assertEqual(json.loads(stdout)["state"], "canceled")
        self.command("run", job["id"], success=False)
        self.assertTrue(list(self.directory.glob(".idmxl-*.part")))

    @unittest.skipUnless(os.name == "nt", "DPAPI token storage is a Windows capability")
    def test_signed_url_is_redacted_and_protected_at_rest(self):
        token = "idmxl-secret-should-not-appear-7391"
        job = self.add("/file?token=" + token)
        self.assertNotIn(token, json.dumps(job))
        self.assertNotIn(token, json.dumps(self.command("list")))
        if os.name == "nt":
            for path in self.state.glob("jobs.db*"):
                self.assertNotIn(token.encode(), path.read_bytes())
        self.assert_completed(job)

    def test_future_database_version_is_rejected(self):
        self.command("list")
        db = sqlite3.connect(self.state / "jobs.db")
        try:
            db.execute("PRAGMA user_version=999")
        finally:
            db.close()
        self.assertIn("database_version_unsupported", self.command("list", success=False).stderr)

    def test_sequential_reports_inflight_bytes_before_checkpoint(self):
        job = self.add("/slow", replayable=False)
        process = self.start(job)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            current = self.command("show", job["id"])
            if int(current["received_bytes"]) > 0:
                self.assertEqual(current["durable_bytes"], "0")
                self.assertGreater(int(current["speed_bytes_per_second"]), 0)
                self.assertEqual(current["total_bytes"], str(len(DATA)))
                break
            time.sleep(.1)
        else:
            self.fail("No live progress before sequential completion")
        self.command("pause",job["id"])
        process.communicate(timeout=10)

    def test_queue_priority_schedule_failure_and_pause_exclusion(self):
        low = self.add(name="low.bin")
        high = self.add(name="high.bin")
        future = self.add(name="future.bin")
        paused = self.add(name="paused.bin")
        broken = self.add("/missing", name="missing.bin")
        self.command("policy",high["id"],input=json.dumps({"priority":10}))
        self.command("policy",broken["id"],input=json.dumps({"priority":20}))
        self.command("policy",future["id"],input=json.dumps({"not_before":int(time.time()*1000)+60000}))
        self.command("pause",paused["id"])
        db=sqlite3.connect(self.state / "jobs.db")
        db.executescript("CREATE TABLE audit(id TEXT); CREATE TRIGGER starts AFTER UPDATE OF state ON jobs WHEN new.state='downloading' AND old.state!='downloading' BEGIN INSERT INTO audit VALUES(new.id); END;")
        db.close()
        self.command("queue-run")
        self.assertEqual(self.command("show",high["id"])["state"],"completed")
        self.assertEqual(self.command("show",low["id"])["state"],"completed")
        self.assertEqual(self.command("show",broken["id"])["state"],"failed")
        self.assertEqual(self.command("show",future["id"])["state"],"queued")
        self.assertEqual(self.command("show",paused["id"])["state"],"paused")
        db=sqlite3.connect(self.state / "jobs.db")
        self.assertEqual([row[0] for row in db.execute("SELECT id FROM audit")],[broken["id"],high["id"],low["id"]])
        db.close()

    def test_queue_waits_for_schedule_and_global_pause_stops_transfer(self):
        job = self.add("/slow")
        start_at = int(time.time()*1000)+1200
        self.command("policy",job["id"],input=json.dumps({"not_before":start_at,"connections":1}))
        process = subprocess.Popen([str(EXE),"--state-dir",str(self.state),"queue-run","--watch"],
                                   stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        self.processes.append(process)
        time.sleep(.3)
        self.assertEqual(self.command("show",job["id"])["state"],"queued")
        self.wait_checkpoint(job,process)
        self.command("queue-pause")
        stdout,stderr = process.communicate(timeout=10)
        self.assertEqual(process.returncode,0,stderr)
        self.assertEqual(self.command("show",job["id"])["state"],"queued")

    def test_v1_migration_preserves_jobs_and_creates_valid_backup(self):
        job = self.add()
        database = self.state / "jobs.db"
        db = sqlite3.connect(database)
        columns = [row[1] for row in db.execute("PRAGMA table_info(jobs)")][:19]
        old_row = db.execute("SELECT " + ",".join(columns) + " FROM jobs").fetchone()
        db.close()
        legacy = self.directory / "legacy"
        legacy.mkdir()
        db = sqlite3.connect(legacy / "jobs.db")
        db.executescript((ROOT / "crates/engine/migrations/001_initial.sql").read_text())
        db.execute("INSERT INTO jobs("+",".join(columns)+") VALUES("+",".join("?" for _ in columns)+")",old_row)
        db.commit(); db.close()
        result = subprocess.run([str(EXE),"--state-dir",str(legacy),"show",job["id"]],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(json.loads(result.stdout)["policy"]["queue_name"],"Default")
        backup = next(legacy.glob("jobs-v1-*.sqlite"))
        db = sqlite3.connect(backup)
        self.assertEqual(db.execute("PRAGMA integrity_check").fetchone()[0],"ok")
        self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0],1)
        self.assertEqual(db.execute("SELECT id FROM jobs").fetchone()[0],job["id"])
        db.close()

    def host(self, frames, origin="chrome-extension://"+"a"*32+"/"):
        config = self.directory / "host.json"
        config.write_text(json.dumps({"allowed_origin":"chrome-extension://"+"a"*32+"/", "state_dir":str(self.state),"download_dir":str(self.directory)}))
        env = dict(os.environ,IDMXL_HOST_CONFIG=str(config))
        return subprocess.run([str(EXE),origin,"--parent-window=0"],input=frames,capture_output=True,env=env,timeout=15)

    @staticmethod
    def frame(value):
        data = json.dumps(value).encode()
        return struct.pack("=I",len(data))+data

    @staticmethod
    def replies(data):
        result=[]
        while data:
            size=struct.unpack("=I",data[:4])[0]
            result.append(json.loads(data[4:4+size])); data=data[4+size:]
        return result

    def test_native_host_durable_idempotent_handoff_and_private_network_block(self):
        request={"version":1,"kind":"enqueue","request_id":str(uuid.uuid4()),"url":self.base+"/file?secret=private","filename":"../../evil:stream.exe"}
        result=self.host(self.frame(request)*2)
        self.assertEqual(result.returncode,0,result.stderr)
        replies=self.replies(result.stdout)
        self.assertTrue(replies[0]["accepted"],replies)
        self.assertEqual(replies[0]["job_id"],replies[1]["job_id"])
        self.assertEqual(len(self.command("list")),1)
        job=self.command("show",replies[0]["job_id"])
        self.assertTrue(os.path.samefile(Path(job["destination"]).parent,self.directory))
        self.assertNotIn("secret",json.dumps(job))

        self.command("run",job["id"],success=False)
        self.assertEqual(self.command("show",job["id"])["error"],"private_network_blocked")

    def test_extension_live_downloads_are_redacted_and_controls_are_strict(self):
        job=self.add()
        request={"version":5,"kind":"jobs","request_id":str(uuid.uuid4())}
        reply=self.replies(self.host(self.frame(request)).stdout)[0]
        self.assertTrue(reply["accepted"],reply)
        self.assertEqual(reply["jobs"][0]["id"],job["id"])
        self.assertNotIn("destination",reply["jobs"][0]);self.assertNotIn("source",reply["jobs"][0])
        request.update(kind="control",job_id=job["id"],action="pause")
        reply=self.replies(self.host(self.frame(request)).stdout)[0]
        self.assertTrue(reply["accepted"],reply);self.assertEqual(self.command("show",job["id"])["state"],"paused")
        for change in [{"action":"delete"},{"url":"https://example.org/secret"},{"confirm_destination":True}]:
            bad=dict(request,**change);reply=self.replies(self.host(self.frame(bad)).stdout)[0]
            self.assertFalse(reply["accepted"],reply)
        self.assertEqual(self.command("show",job["id"])["state"],"paused")

    def test_immediate_retry_from_extension_completes_original_file(self):
        job=self.add()
        with sqlite3.connect(self.state/"jobs.db") as db:
            db.execute("UPDATE jobs SET state='failed',error='fixture' WHERE id=?",(job["id"],))
            db.execute("INSERT INTO origin_cooldowns VALUES(?,?)",(self.base,int(time.time()*1000)+3600000))
        db.close()
        request={"version":5,"kind":"control","request_id":str(uuid.uuid4()),"job_id":job["id"],"action":"retry"}
        reply=self.replies(self.host(self.frame(request)).stdout)[0]
        self.assertTrue(reply["accepted"],reply)
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            current=self.command("show",job["id"])
            if current["state"]=="completed":break
            time.sleep(.05)
        self.assertEqual(current["state"],"completed",current)
        self.assertEqual(hashlib.sha256(Path(current["destination"]).read_bytes()).hexdigest(),DIGEST)
        self.stop_captured_worker()

    def test_native_host_rejects_origin_oversize_and_unknown_fields(self):
        result=self.host(b"",origin="chrome-extension://"+"b"*32+"/")
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(result.stdout,b"")
        self.assertNotEqual(self.host(struct.pack("=I",262145)).returncode,0)
        self.assertNotEqual(self.host(b"\x04\x00").returncode,0)
        request={"version":1,"kind":"enqueue","request_id":str(uuid.uuid4()),"url":self.base,"filename":"x","headers":{"Cookie":"bad"}}
        reply=self.replies(self.host(self.frame(request)).stdout)[0]
        self.assertFalse(reply["accepted"])

    def test_capture_query_urls_still_cannot_reach_private_networks(self):
        for scheme in ("http", "https"):
            request={"version":2,"kind":"prepare","request_id":str(uuid.uuid4()),
                     "url":self.base.replace("http:",scheme+":")+"/file?download=1",
                     "filename":"capture.bin","etag":'"one"',"total":str(len(DATA))}
            result=self.host(self.frame(request))
            self.assertEqual(result.returncode,0,result.stderr)
            reply=self.replies(result.stdout)[0]
            self.assertFalse(reply["accepted"])
            self.assertEqual(reply["error"],"private_network_blocked")
        self.assertEqual(self.command("list"),[])

    def seed_capture(self, name="captured.bin"):
        # Seed a held ownership record around our original localhost fixture.
        # Public-only preparation is tested separately; it still rejects localhost.
        job = self.add(name=name)
        request_id = str(uuid.uuid4())
        with sqlite3.connect(self.state / "jobs.db") as db:
            db.execute("INSERT INTO handoffs VALUES(?,?)", (request_id, job["id"]))
            db.execute("INSERT INTO capture_handoffs VALUES(?,'ready',?,'\"v1\"',?)", (request_id, int(time.time()*1000)+60000, len(DATA)))
            db.execute("UPDATE jobs SET state='awaiting_browser',requested='paused' WHERE id=?", (job["id"],))
        db.close()
        return job, {"version":2,"kind":"commit","request_id":request_id,"browser_cancelled":True}

    def browser_message(self, client, reports=None, acknowledgements=None):
        return {"version":4,"kind":"browser_sync","request_id":str(uuid.uuid4()),"client_id":client,"reports":reports or [],"acknowledgements":acknowledgements or []}

    def test_browser_assisted_protocol_stores_progress_without_replaying_a_source(self):
        client=str(uuid.uuid4())
        report={"id":str(uuid.uuid4()),"name":"generated.zip","origin":"https://example.org","state":"downloading","total":None,"received":123,"speed":10,"can_resume":True,"exists":False}
        message=self.browser_message(client,[report])
        reply=self.replies(self.host(self.frame(message)).stdout)[0]
        self.assertTrue(reply["accepted"],reply);self.assertEqual(reply["version"],4);self.assertEqual(reply["client_id"],client)
        self.assertEqual(self.command("list"),[])
        report.update(state="completed",total=123,exists=True)
        reply=self.replies(self.host(self.frame(self.browser_message(client,[report]))).stdout)[0]
        self.assertTrue(reply["accepted"],reply)
        with sqlite3.connect(self.state/"jobs.db") as db:
            saved=json.loads(db.execute("SELECT value FROM settings WHERE key=?",("browser_transfer:"+report["id"],)).fetchone()[0])
            self.assertEqual(saved["report"]["state"],"completed")
            self.assertEqual(db.execute("SELECT value FROM settings WHERE key='queue_running'").fetchone()[0],"false")
        db.close()
        for fields in [{"url":self.base+"/file"},{"reports":[dict(report,cookies="secret")]},{"reports":[dict(report,origin="https://example.org/file?secret=token")]},{"reports":[dict(report,name="C:\\private\\file.zip")]}]:
            bad=dict(self.browser_message(client,[report]),**fields)
            reply=self.replies(self.host(self.frame(bad)).stdout)[0]
            self.assertFalse(reply["accepted"],reply)
        self.assertFalse((self.directory/"generated.zip").exists())

    def test_browser_control_protocol_scopes_commands_to_the_client_and_acknowledges_once(self):
        client=str(uuid.uuid4());other=str(uuid.uuid4());transfer=str(uuid.uuid4())
        report={"id":transfer,"name":"file.zip","origin":None,"state":"paused","total":None,"received":10,"speed":0,"can_resume":True,"exists":False}
        self.host(self.frame(self.browser_message(client,[report])))
        command={"id":str(uuid.uuid4()),"transfer_id":transfer,"action":"resume","created":int(time.time()*1000)}
        with sqlite3.connect(self.state/"jobs.db") as db:
            db.execute("INSERT INTO settings VALUES(?,?)",("browser_command:"+client+":"+command["id"],json.dumps(command)))
        db.close()
        reply=self.replies(self.host(self.frame(self.browser_message(other))).stdout)[0];self.assertEqual(reply["commands"],[])
        reply=self.replies(self.host(self.frame(self.browser_message(client))).stdout)[0];self.assertEqual(reply["commands"],[command])
        ack={"id":command["id"],"success":True}
        reply=self.replies(self.host(self.frame(self.browser_message(client,acknowledgements=[ack]))*2).stdout)
        self.assertTrue(all(not response["commands"] for response in reply));self.assertEqual(self.command("list"),[])

    def stop_captured_worker(self):
        self.command("queue-pause")
        deadline = time.monotonic() + 15
        while True:
            try:
                # A scheduler can release its lock just before its Store drops.
                # Wait for all profile connections before deleting test files.
                for name in ["scheduler.lock", "engine.lock", "maintenance.lock"]:
                    lock_path=self.state/name
                    if not lock_path.exists():
                        continue
                    with open(lock_path, "r+b") as handle:
                        if os.name == "nt":
                            import msvcrt
                            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                            msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
                        else:
                            import fcntl
                            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
                            fcntl.flock(handle, fcntl.LOCK_UN)
                break
            except OSError:
                if time.monotonic() >= deadline:
                    self.fail("captured download worker did not close its profile")
            time.sleep(0.05)
        time.sleep(0.1)

    def test_capture_commit_starts_a_worker_that_survives_host_exit_and_completes(self):
        first, commit = self.seed_capture()
        second, other = self.seed_capture(name="captured-two.bin")
        try:
            result = self.host(self.frame(commit) + self.frame(other))
            self.assertEqual(result.returncode, 0, result.stderr)
            replies = self.replies(result.stdout)
            self.assertTrue(all(reply["activation"]["queue_started"] for reply in replies), replies)
            deadline = time.monotonic() + 20
            for job in [first, second]:
                while True:
                    current = self.command("show", job["id"])
                    if current["state"] == "completed":
                        break
                    self.assertNotIn(current["state"], ["failed", "canceled"],
                                     f"automatic capture stopped: {current['state']} ({current['error']})")
                    if time.monotonic() >= deadline:
                        self.fail(f"automatic capture did not complete after native host exited: {current['state']} ({current['error']})")
                    time.sleep(0.05)
                self.assertEqual(hashlib.sha256(Path(job["destination"]).read_bytes()).hexdigest(), DIGEST)
            self.assertEqual(len(self.command("list")), 2)
        finally:
            self.stop_captured_worker()

    def test_capture_requires_cancellation_and_duplicate_commit_preserves_manual_pause(self):
        job, commit = self.seed_capture()
        policy = job["policy"]
        policy["not_before"] = int(time.time()*1000)+60000
        self.command("policy", job["id"], input=json.dumps(policy))
        invalid = dict(commit, browser_cancelled=False)
        reply = self.replies(self.host(self.frame(invalid)).stdout)[0]
        self.assertFalse(reply["accepted"])
        self.assertEqual(self.command("show", job["id"])["state"], "awaiting_browser")
        with sqlite3.connect(self.state / "jobs.db") as db:
            self.assertEqual(db.execute("SELECT value FROM settings WHERE key='queue_running'").fetchone()[0], "false")
        db.close()
        try:
            reply = self.replies(self.host(self.frame(commit)).stdout)[0]
            self.assertTrue(reply["activation"]["queue_started"], reply)
            self.command("pause", job["id"])
        finally:
            self.stop_captured_worker()
        reply = self.replies(self.host(self.frame(commit)).stdout)[0]
        self.assertEqual(reply["job_id"], job["id"])
        self.assertEqual(self.command("show", job["id"])["state"], "paused")
        with sqlite3.connect(self.state / "jobs.db") as db:
            self.assertEqual(db.execute("SELECT value FROM settings WHERE key='queue_running'").fetchone()[0], "false")
        db.close()

    def test_retry_after_persists_and_blocks_other_jobs_on_same_origin(self):
        for path in ("/cooldown","/date-cooldown"):
            with self.subTest(path=path):
                # Distinct state profiles keep the first origin cooldown out of the second case.
                self.state=self.directory / path.removeprefix("/")
                job=self.add(path,name=path.removeprefix("/")+".bin")
                result=self.command("run",job["id"],success=False)
                self.assertIn("server_throttled",result.stderr)
                view=self.command("show",job["id"])
                self.assertGreater(int(view["retry_not_before"]),int(time.time()*1000)+3500000)
                second=self.add(name=path.removeprefix("/")+"-other.bin")
                self.assertIn("server_retry_pending",self.command("run",second["id"],success=False).stderr)
                self.command("queue-run")
                self.assertEqual(self.command("show",second["id"])["state"],"queued")

    def test_batch_is_bounded_deduplicated_and_does_not_start_jobs(self):
        request={"url":self.base+"/file?token=batch-secret","destination":str(self.directory / "batch.bin")}
        result=self.command("batch",input=json.dumps([request,request,{"url":"file:///C:/private","destination":str(self.directory / "bad.bin")}]))
        self.assertEqual([r["status"] for r in result["results"]],["queued","duplicate","rejected"])
        self.assertNotIn("batch-secret",json.dumps(result))
        self.assertEqual(self.command("list")[0]["state"],"queued")
        self.command("batch",input="[]",success=False)

    def test_link_import_categories_collisions_dedup_and_integrity(self):
        root=self.directory / "downloads"; root.mkdir()
        documents=root / "Documents"; documents.mkdir()
        existing=documents / "report one.PDF"; existing.write_bytes(b"preserve original")
        folders={"directory":str(root),"organize":True,"rules":[{"folder":"Documents","extensions":[".PDF"]},{"folder":"Archives","extensions":["zip"]}]}
        self.command("folders","--set",input=json.dumps(folders))
        persisted=self.command("folders")["folders"]
        self.assertEqual(persisted["rules"][0]["extensions"],["pdf"])
        first=self.base+"/report%20one.PDF?token=private-one"
        batch={"links":[first,first+"#duplicate",self.base+"/report%20one.PDF?token=private-two",self.base+"/CON.zip",self.base+"/folder%2Fescape.bin","file:///C:/private"],"folders":folders,"replayable":True,"queue_name":"Imported"}
        result=self.command("add-links",input=json.dumps(batch))["results"]
        self.assertEqual([r["status"] for r in result],["queued","duplicate","queued","queued","queued","rejected"])
        self.assertNotIn("private-one",json.dumps(result)); self.assertNotIn("private-two",json.dumps(result))
        jobs=[r["job"] for r in result if r["status"]=="queued"]
        self.assertEqual([j["name"] for j in jobs],["report one (2).PDF","report one (3).PDF","_CON.zip","folder_escape.bin"])
        self.assertEqual([Path(j["destination"]).parent.name for j in jobs],["Documents","Documents","Archives","Other"])
        self.assertTrue(all(j["state"]=="queued" and j["policy"]["queue_name"]=="Imported" for j in jobs))
        self.command("add",self.base+"/file","--output",jobs[0]["destination"],success=False)
        self.command("queue-run")
        for job in jobs:
            final=self.command("show",job["id"])
            self.assertEqual(final["state"],"completed")
            self.assertEqual(hashlib.sha256(Path(final["destination"]).read_bytes()).hexdigest(),DIGEST)
        self.assertEqual(existing.read_bytes(),b"preserve original")

    def test_folder_rules_reject_traversal_ambiguity_and_invalid_settings(self):
        root=self.directory / "downloads"; root.mkdir()
        for rules in [
            [{"folder":"../outside","extensions":["zip"]}],
            [{"folder":"NUL","extensions":["zip"]}],
            [{"folder":"A","extensions":["PDF"]},{"folder":"B","extensions":[".pdf"]}],
            [{"folder":"A","extensions":["zip"]},{"folder":"a","extensions":["rar"]}],
            [{"folder":"A","extensions":["..exe"]}],
        ]:
            settings={"directory":str(root),"organize":True,"rules":rules}
            self.command("folders","--set",input=json.dumps(settings),success=False)
        self.assertEqual(list(root.iterdir()),[])
        settings={"directory":str(root),"organize":False,"rules":[]}
        self.command("folders","--set",input=json.dumps(settings))
        self.assertFalse(self.command("folders")["folders"]["organize"])
        self.command("add-links",input=json.dumps({"links":[self.base+"/file"]*1001,"folders":settings}),success=False)

    def test_concurrent_link_import_reserves_distinct_filenames(self):
        root=self.directory / "downloads"; root.mkdir()
        batch=json.dumps({"links":[self.base+"/file.zip"],"folders":{"directory":str(root),"organize":False,"rules":[]}})
        processes=[subprocess.Popen([str(EXE),"--state-dir",str(self.state),"add-links"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for _ in range(2)]
        self.processes.extend(processes)
        for process in processes:
            process.stdin.write(batch); process.stdin.close(); process.stdin=None
        names=[]
        for process in processes:
            stdout,stderr=process.communicate(timeout=15)
            self.assertEqual(process.returncode,0,stderr)
            names.append(json.loads(stdout)["results"][0]["job"]["name"])
        self.assertEqual(set(names),{"file.zip","file (2).zip"})
        self.command("queue-run")
        for name in names: self.assertEqual(hashlib.sha256((root / name).read_bytes()).hexdigest(),DIGEST)

    def test_recurring_queue_window_does_not_starve_other_queues(self):
        blocked=self.add(name="night.bin"); allowed=self.add(name="day.bin")
        self.command("policy",blocked["id"],input=json.dumps({"queue_name":"Night","priority":100}))
        other_day=1 << ((time.localtime().tm_wday+3)%7)
        window={"days":other_day,"start_minute":0,"end_minute":0}
        self.command("queue-window","Night","--set",input=json.dumps(window))
        self.assertFalse(self.command("queue-window","Night")["open_now"])
        self.command("queue-run")
        self.assertEqual(self.command("show",blocked["id"])["state"],"queued")
        self.assertFalse(self.command("show",blocked["id"])["queue_window_open"])
        self.assertEqual(self.command("show",allowed["id"])["state"],"completed")
        self.command("queue-window","Night","--set",input="null")
        self.command("queue-run")
        self.assertEqual(self.command("show",blocked["id"])["state"],"completed")
        for bad in [{"days":0,"start_minute":0,"end_minute":0},{"days":127,"start_minute":1440,"end_minute":0},{"days":127,"start_minute":0,"end_minute":0,"unknown":True}]:
            self.command("queue-window","Night","--set",input=json.dumps(bad),success=False)

    def test_recurring_window_change_checkpoints_and_resumes_active_ranges(self):
        gate=threading.Event()
        self.server.window_range_gate=gate
        self.addCleanup(gate.set)
        job=self.add("/window-gated",name="window-ranges.bin")
        closed={"days":1 << ((time.localtime().tm_wday+3)%7),"start_minute":0,"end_minute":0}
        self.command("queue-window","Default","--set",input=json.dumps(closed))
        process=subprocess.Popen([str(EXE),"--state-dir",str(self.state),"queue-run","--watch"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        self.processes.append(process); time.sleep(.3)
        self.assertEqual(self.command("show",job["id"])["state"],"queued")
        self.command("queue-window","Default","--set",input="null")
        checkpoint=self.wait_checkpoint(job,process)
        self.assertEqual(checkpoint["state"],"downloading")
        self.assertEqual(checkpoint["durable_bytes"],"1048576")
        self.command("queue-window","Default","--set",input=json.dumps(closed))
        deadline=time.monotonic()+12
        while time.monotonic()<deadline:
            current=self.command("show",job["id"])
            if current["state"]=="queued" and int(current["durable_bytes"])>0: break
            time.sleep(.05)
        else: self.fail(f"Active ranged job did not wait for recurring hours: state={current['state']}, durable={current['durable_bytes']}, window_open={current['queue_window_open']}")
        self.assertFalse(current["queue_window_open"])
        gate.set()
        self.command("queue-window","Default","--set",input="null")
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if self.command("show",job["id"])["state"]=="completed": break
            time.sleep(.1)
        else: self.fail("Ranged job did not resume after recurring hours opened")
        self.command("queue-pause"); _,stderr=process.communicate(timeout=10)
        self.assertEqual(process.returncode,0,stderr)
        self.assertEqual(hashlib.sha256(Path(job["destination"]).read_bytes()).hexdigest(),DIGEST)

    def test_recurring_window_end_does_not_replay_single_use_transfer(self):
        job=self.add("/slow",replayable=False,name="window-single.bin")
        with Fixture.guard: before=Fixture.counters[("/slow","")]
        process=subprocess.Popen([str(EXE),"--state-dir",str(self.state),"queue-run","--watch"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        self.processes.append(process)
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            if int(self.command("show",job["id"])["received_bytes"])>0: break
            time.sleep(.05)
        else: self.fail("No live single-stream progress")
        closed={"days":1 << ((time.localtime().tm_wday+3)%7),"start_minute":0,"end_minute":0}
        self.command("queue-window","Default","--set",input=json.dumps(closed))
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if self.command("show",job["id"])["state"]=="completed": break
            time.sleep(.1)
        else: self.fail("Single-use job did not finish its original request")
        self.command("queue-pause"); _,stderr=process.communicate(timeout=10)
        self.assertEqual(process.returncode,0,stderr)
        with Fixture.guard: self.assertEqual(Fixture.counters[("/slow","")]-before,1)
        self.assertEqual(hashlib.sha256(Path(job["destination"]).read_bytes()).hexdigest(),DIGEST)

    @unittest.skipUnless(os.environ.get("IDMXL_FAULT_EXE"),"Use the test-only short scheduling quantum")
    def test_named_queues_rotate_after_a_durable_checkpoint(self):
        first=self.add("/slow",name="first-queue.bin")
        second=self.add(name="second-queue.bin")
        self.command("policy",first["id"],input=json.dumps({"queue_name":"A","connections":1}))
        self.command("policy",second["id"],input=json.dumps({"queue_name":"B","connections":1}))
        db=sqlite3.connect(self.state / "jobs.db")
        db.executescript("CREATE TABLE audit(id TEXT); CREATE TRIGGER starts AFTER UPDATE OF state ON jobs WHEN new.state='downloading' AND old.state!='downloading' BEGIN INSERT INTO audit VALUES(new.id); END;")
        db.close()
        with Fixture.guard:before=Fixture.counters[("/slow","bytes=0-1048575")]
        result=subprocess.run([os.environ["IDMXL_FAULT_EXE"],"--state-dir",str(self.state),"queue-run"],env=dict(os.environ,IDMXL_TEST_QUANTUM_MS="500"),capture_output=True,text=True,timeout=35)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(self.command("show",first["id"])["state"],"completed")
        self.assertEqual(self.command("show",second["id"])["state"],"completed")
        db=sqlite3.connect(self.state / "jobs.db")
        order=[r[0] for r in db.execute("SELECT id FROM audit")];db.close()
        self.assertEqual(order[:3],[first["id"],second["id"],first["id"]])
        with Fixture.guard:self.assertEqual(Fixture.counters[("/slow","bytes=0-1048575")]-before,1)

    @unittest.skipUnless(os.environ.get("IDMXL_FAULT_EXE"),"Run scripts/test.ps1 -Faults for boundary exits")
    def test_failed_retry_budget_survives_repeated_process_loss(self):
        self.server.retry_healthy = False
        job = self.add("/retry-crash")
        try:
            for _ in range(3):
                result = subprocess.run([os.environ["IDMXL_FAULT_EXE"], "--state-dir", str(self.state), "run", job["id"], "--connections", "1", "--retries", "2"],
                                        env=dict(os.environ, IDMXL_FAULT="after_retry_record"), capture_output=True, timeout=15)
                self.assertEqual(result.returncode, 86, result.stderr)
            with Fixture.guard:
                before = Fixture.counters[("/retry-crash", "bytes=0-1048575")]
            result = self.command("run", job["id"], "--connections", "1", "--retries", "2", success=False)
            self.assertIn("retry_exhausted", result.stderr)
            with Fixture.guard:
                self.assertEqual(Fixture.counters[("/retry-crash", "bytes=0-1048575")], before)
            current = self.command("show", job["id"])
            self.assertEqual(current["failed_requests"], "3")
            self.server.retry_healthy = True
            self.assert_completed(job, "ranged")
            current = self.command("show", job["id"])
            self.assertEqual(current["failed_requests"], "3")
            self.assertGreater(int(current["transfer_requests"]), 3)
        finally:
            self.server.retry_healthy = False

    def test_refresh_expired_link_preserves_only_matching_checkpoints(self):
        job = self.add("/slow?token=old")
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        self.command("pause", job["id"])
        process.communicate(timeout=10)
        paused = self.command("show", job["id"])
        self.assertEqual(paused["state"], "paused")
        self.assertGreater(int(paused["durable_bytes"]), 0)
        for url, code in [(self.base + "/slow?changed=1", "resource_changed"),
                          (self.base + "/file?token=new", "source_identity_unproven")]:
            result = self.command("refresh-source", job["id"], input=json.dumps({"url": url}), success=False)
            self.assertIn(code, result.stderr)
        refreshed = self.command("refresh-source", job["id"], input=json.dumps({"url": self.base + "/slow?token=new"}))
        self.assertEqual(refreshed["durable_bytes"], paused["durable_bytes"])
        self.assertEqual(refreshed["generation"], paused["generation"])
        self.assertNotIn("token", json.dumps(refreshed))
        with Fixture.guard:
            before = Fixture.counters[("/slow", "bytes=0-1048575")]
        self.assert_completed(job, "ranged")
        with Fixture.guard:
            self.assertEqual(Fixture.counters[("/slow", "bytes=0-1048575")], before)

    def test_refresh_without_expected_digest_cannot_reuse_bytes(self):
        job = self.add("/slow?token=old", digest=None)
        process = self.start(job, "--connections", "1")
        self.wait_checkpoint(job, process)
        self.command("pause", job["id"])
        process.communicate(timeout=10)
        result = self.command("refresh-source", job["id"], input=json.dumps({"url": self.base + "/slow?token=new"}), success=False)
        self.assertIn("source_identity_unproven", result.stderr)
        self.assertGreater(int(self.command("show", job["id"])["durable_bytes"]), 0)

    @unittest.skipUnless(os.environ.get("IDMXL_FAULT_EXE"),"Run scripts/test.ps1 -Faults for boundary exits")
    def test_abrupt_exit_at_each_durability_boundary(self):
        for point in ["after_body_write","after_extent_flush","before_extent_commit","after_extent_commit",
                      "after_publication_intent","after_origin_metadata","after_rename","before_completion_commit"]:
            with self.subTest(point=point):
                job=self.add(name=point+".bin")
                result=subprocess.run([os.environ["IDMXL_FAULT_EXE"],"--state-dir",str(self.state),"run",job["id"]],
                                      env=dict(os.environ,IDMXL_FAULT=point),capture_output=True,timeout=15)
                self.assertEqual(result.returncode,86,result.stderr)
                self.assert_completed(job)


if __name__ == "__main__":
    unittest.main(verbosity=2)
