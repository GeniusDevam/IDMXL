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
        if path == "/throttle" and byte_range and byte_range != "bytes=0-0" and attempt == 1:
            self.send_response(429)
            self.send_header("Retry-After", "0")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if byte_range and path != "/ignore":
            start, end = map(int, byte_range.removeprefix("bytes=").split("-"))
            end = min(end, len(DATA) - 1)
            body = DATA[start:end + 1]
            self.send_response(206)
            claimed_end = end + 1 if path == "/malformed" and byte_range != "bytes=0-0" else end
            self.send_header("Content-Range", f"bytes {start}-{claimed_end}/{len(DATA)}")
            tag = 'W/"v1"' if path == "/weak" else ('"v2"' if path == "/changed" and start > 0 else '"v1"')
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
        self.end_headers()
        if path == "/truncate" and byte_range != "bytes=0-0" and attempt == 1:
            body = body[:len(body) // 2]
        try:
            for i in range(0, len(body), 32768):
                self.wfile.write(body[i:i + 32768])
                self.wfile.flush()
                if path == "/slow" and len(body) > 1:
                    time.sleep(0.025)
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
                                text=True, capture_output=True, timeout=35)
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


if __name__ == "__main__":
    unittest.main(verbosity=2)
