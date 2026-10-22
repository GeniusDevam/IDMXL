"""Real HTTP cooldowns near completion, with isolated profiles and owned payloads."""
import email.utils
import hashlib
import http.server
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading
import time
import unittest

ROOT=Path(__file__).resolve().parents[1]
EXE=Path(os.environ.get("IDMXL_EXE",ROOT/"target/debug/idmxl.exe"))
BLOCK=1048576
DATA=(bytes(range(256))*(32*BLOCK//256))+bytes(range(256))+b"!"
DIGEST=hashlib.sha256(DATA).hexdigest()
TAIL=32*BLOCK


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version="HTTP/1.1"
    def log_message(self,*_):pass
    def do_GET(self):
        start,end=map(int,self.headers.get("Range","bytes=0-0").removeprefix("bytes=").split("-"))
        end=min(end,len(DATA)-1)
        probe=start==end==0
        status=206;header=None
        with self.server.guard:
            if start==TAIL or (self.server.mode=="queue-long" and not probe and end>=TAIL):
                self.server.tail_attempts+=1
                attempt=self.server.tail_attempts
                if self.server.mode=="three" and attempt<=3:
                    status=503 if attempt%2 else 429
                    header=None if attempt==1 else "not a date" if attempt==2 else email.utils.formatdate(time.time()+1,usegmt=True)
                elif self.server.mode in ("long","queue-long"):status=503;header="600"
                elif self.server.mode=="always":status=429
                elif self.server.mode=="once" and attempt==1:status=503;header="1"
            self.server.requests.append({"start":start,"end":end,"at":time.time(),"status":status,"probe":probe})
        self.send_response(status)
        if status==206:
            self.send_header("Content-Range",f"bytes {start}-{end}/{len(DATA)}")
            self.send_header("ETag",getattr(self.server,"tag",'"recovery-fixture-v1"'))
        if header is not None:self.send_header("Retry-After",header)
        self.send_header("Content-Length",str(end-start+1 if status==206 else 0))
        self.send_header("Connection","close");self.close_connection=True
        try:
            self.end_headers()
            if status==206:
                for offset in range(start,end+1,65536):self.wfile.write(DATA[offset:min(offset+65536,end+1)])
        except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError,OSError):pass


class RecoveryTests(unittest.TestCase):
    def setUp(self):
        scratch=ROOT/".test-output";scratch.mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix="recovery-http-",dir=scratch)
        self.directory=Path(self.temp.name).resolve();assert self.directory.is_relative_to(scratch.resolve())
        self.state=self.directory/"state";self.processes=[];self.servers=[]
        self.server=self.make_server("once")
        self.base=f"http://127.0.0.1:{self.server.server_port}"
    def make_server(self,mode):
        server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
        server.mode=mode;server.guard=threading.Lock();server.requests=[];server.tail_attempts=0
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        self.servers.append((server,thread));return server
    def tearDown(self):
        for p in self.processes:
            if p.poll() is None:p.kill()
            p.communicate(timeout=15)
        for s,t in self.servers:s.shutdown();s.server_close();t.join(timeout=5)
        self.temp.cleanup()
    def command(self,*args,input=None,success=True):
        p=subprocess.run([str(EXE),"--state-dir",str(self.state),*map(str,args)],input=input,capture_output=True,text=True,encoding="utf-8",timeout=20)
        if success:self.assertEqual(p.returncode,0,p.stderr);return json.loads(p.stdout)
        self.assertNotEqual(p.returncode,0,p.stdout);return p
    def add(self,name="file.bin",base=None):
        return self.command("add","--url-stdin","--output",self.directory/name,"--replayable","--sha256",DIGEST,input=(base or self.base)+"/file?token=never-log-this")
    def start(self,job,*extra):
        p=subprocess.Popen([str(EXE),"--state-dir",str(self.state),"run",job["id"],"--connections","1","--request-mib","4",*map(str,extra)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding="utf-8")
        self.processes.append(p);return p
    def waiting(self,job,p,expected=TAIL):
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            view=self.command("show",job["id"])
            if view["state"]=="waiting":
                if expected is not None:self.assertEqual(view["durable_bytes"],str(expected))
                self.assertFalse(Path(view["destination"]).exists())
                return view
            self.assertIsNone(p.poll(),p.communicate() if p.poll() is not None else "")
            time.sleep(.05)
        self.fail("waiting state not reached")
    def complete(self,job,p,timeout=80):
        out,err=p.communicate(timeout=timeout);self.assertEqual(p.returncode,0,err)
        view=json.loads(out);self.assertEqual(view["state"],"completed");self.assertEqual(view["sha256"],DIGEST)
        self.assertEqual(hashlib.sha256(Path(view["destination"]).read_bytes()).hexdigest(),DIGEST)
        return view
    def test_repeated_429_and_503_at_end_recover_beyond_two_retries(self):
        self.server.mode="three";j=self.add();metrics=self.directory/"metrics.json"
        p=self.start(j,"--metrics",metrics);first=self.waiting(j,p)
        completed=self.complete(j,p)
        self.assertEqual(completed["generation"],first["generation"])
        self.assertEqual(completed["failed_requests"],"3")
        report=json.loads(metrics.read_text());self.assertEqual(report["status"],"completed")
        if os.name=="nt":
            self.assertEqual(report["validation_reuses"],3)
            self.assertEqual(report["resume_validated_bytes"],0)
            self.assertEqual(report["validation_reused_bytes"],3*TAIL)
        self.assertEqual(len(report["recovery_schedules"]),3)
        self.assertIn("429",report["http_statuses"]);self.assertIn("503",report["http_statuses"])
        self.assertTrue(any(x["invalid_retry_after"] for x in report["response_events"]))
        for secret in ("never-log-this",self.base,"file.bin"):self.assertNotIn(secret,metrics.read_text())
        with self.server.guard:requests=list(self.server.requests)
        received=[(x["start"],x["end"]) for x in requests if not x["probe"] and x["status"]==206 and x["start"]<TAIL]
        self.assertEqual(len(received),len(set(received)),"saved prefix was fetched again")
        failures=[x for x in requests if x["status"] in (429,503)]
        self.assertGreaterEqual(failures[1]["at"]-failures[0]["at"],5)
    def test_long_cooldown_survives_process_loss_without_early_requests(self):
        self.server.mode="long";j=self.add();metrics=self.directory/"long-wait.json";p=self.start(j,"--metrics",metrics);first=self.waiting(j,p)
        self.assertGreater(int(first["retry_not_before"]),time.time()*1000+590000)
        deadline=time.monotonic()+5
        while True:
            try:
                snapshot=json.loads(metrics.read_text())
                if snapshot.get("status")=="waiting":break
            except (ValueError,FileNotFoundError):pass
            self.assertLess(time.monotonic(),deadline);time.sleep(.02)
        self.assertEqual(snapshot["recovery_schedules"][-1]["http_status"],503)
        self.assertEqual(snapshot["recovery_schedules"][-1]["retry_after_ms"],600000)
        p.kill();p.communicate(timeout=10)
        self.assertEqual(json.loads(metrics.read_text())["status"],"waiting")
        count=len(self.server.requests)
        self.assertEqual(self.command("show",j["id"])["state"],"waiting")
        self.command("queue-run");self.assertEqual(len(self.server.requests),count)
        # Simulate the passage of the ten-minute cooldown without waiting ten
        # wall-clock minutes. Separate tests exercise real timer delays.
        with sqlite3.connect(self.state/"jobs.db") as db:
            db.execute("UPDATE origin_cooldowns SET until_ms=0")
            db.execute("UPDATE recovery_jobs SET next_ms=0")
            db.execute("UPDATE origin_pacing SET next_ms=0")
        db.close()
        self.server.mode="healthy";resumed_metrics=self.directory/"resumed.json"
        resumed=self.complete(j,self.start(j,"--metrics",resumed_metrics))
        self.assertEqual(json.loads(resumed_metrics.read_text())["resume_validated_bytes"],TAIL)
        self.assertEqual(resumed["generation"],first["generation"])
        new=[x for x in self.server.requests[count:] if not x["probe"]]
        self.assertTrue(new);self.assertTrue(all(x["start"]==TAIL for x in new))
    def test_pause_and_cancel_during_cooldown_are_immediate_and_persistent(self):
        for action in ("pause","cancel"):
            with self.subTest(action=action):
                self.server.mode="long";j=self.add(name=action+".bin")
                # A previous subcase's origin deadline should remain respected.
                with sqlite3.connect(self.state/"jobs.db") as db:
                    db.execute("UPDATE origin_cooldowns SET until_ms=0")
                    db.execute("UPDATE origin_pacing SET next_ms=0")
                db.close()
                p=self.start(j)
                self.waiting(j,p);self.command(action,j["id"])
                out,err=p.communicate(timeout=5);self.assertEqual(p.returncode,0,err)
                view=json.loads(out);self.assertEqual(view["state"],"paused" if action=="pause" else "canceled")
                self.assertEqual(view["durable_bytes"],str(TAIL));self.command("queue-run")
                self.assertEqual(self.command("show",j["id"])["state"],view["state"])
    def test_no_progress_budget_expires_without_losing_saved_bytes(self):
        self.server.mode="always";j=self.add();p=self.start(j,"--recovery-timeout-seconds","1")
        self.waiting(j,p);out,err=p.communicate(timeout=5)
        self.assertNotEqual(p.returncode,0,out);self.assertIn("recovery_timed_out",err)
        view=self.command("show",j["id"]);self.assertEqual(view["state"],"failed");self.assertEqual(view["durable_bytes"],str(TAIL))
        self.assertEqual(self.server.tail_attempts,1)
    def test_another_origin_can_finish_while_first_job_waits(self):
        self.server.mode="long";j=self.add();p=self.start(j);self.waiting(j,p)
        other=self.make_server("healthy");base=f"http://127.0.0.1:{other.server_port}"
        second=self.add(name="other.bin",base=base)
        self.command("policy",second["id"],input=json.dumps({"connections":1}))
        self.command("queue-run")
        self.assertEqual(self.command("show",second["id"])["sha256"],DIGEST)
        self.assertEqual(self.command("show",j["id"])["state"],"waiting")
        self.command("pause",j["id"]);p.communicate(timeout=5)

    @unittest.skipUnless(os.name=="nt","Windows mandatory file sharing")
    def test_waiting_owns_file_and_second_worker_cannot_change_job(self):
        self.server.mode="long";j=self.add();p=self.start(j);first=self.waiting(j,p)
        with sqlite3.connect(self.state/"jobs.db") as db:
            staging=Path(db.execute("SELECT staging FROM jobs WHERE id=?",(j["id"],)).fetchone()[0])
        db.close()
        with self.assertRaises(PermissionError):
            with staging.open("r+b") as f:f.write(b"changed")
        with self.assertRaises(PermissionError):staging.rename(staging.with_suffix(".moved"))
        error=self.command("run",j["id"],success=False)
        self.assertIn("engine_busy",error.stderr)
        self.assertEqual(self.command("show",j["id"])["state"],"waiting")
        self.command("pause",j["id"]);p.communicate(timeout=5)
        # Releasing ownership must restore write access, and a later process
        # must detect corruption instead of trusting the previous validation.
        with staging.open("r+b") as f:f.write(b"changed")
        with sqlite3.connect(self.state/"jobs.db") as db:
            db.execute("UPDATE origin_cooldowns SET until_ms=0")
            db.execute("UPDATE recovery_jobs SET next_ms=0")
            db.execute("UPDATE origin_pacing SET next_ms=0")
        db.close()
        self.server.mode="healthy";metrics=self.directory/"after-change.json"
        resumed=self.complete(j,self.start(j,"--metrics",metrics))
        self.assertEqual(resumed["generation"],first["generation"])
        self.assertEqual(json.loads(metrics.read_text())["resume_validated_bytes"],TAIL)

    def test_changed_remote_identity_discards_cached_validation(self):
        self.server.mode="long";j=self.add();metrics=self.directory/"changed.json"
        p=self.start(j,"--metrics",metrics);first=self.waiting(j,p)
        self.server.tag='"recovery-fixture-v2"';self.server.mode="healthy"
        with sqlite3.connect(self.state/"jobs.db") as db:
            db.execute("UPDATE origin_cooldowns SET until_ms=0")
            db.execute("UPDATE recovery_jobs SET next_ms=0")
            db.execute("UPDATE origin_pacing SET next_ms=0")
        db.close()
        result=self.complete(j,p)
        self.assertEqual(result["generation"],first["generation"]+1)
        self.assertEqual(json.loads(metrics.read_text())["validation_reuses"],0)
        self.assertEqual(sum(x["start"]==0 and not x["probe"] for x in self.server.requests),2)

    @unittest.skipUnless(os.name=="nt","Windows mandatory file sharing")
    def test_scheduler_retains_ownership_across_waiting_then_releases_on_cancel(self):
        self.server.mode="queue-long";j=self.add()
        self.command("policy",j["id"],input=json.dumps({"connections":1}))
        p=subprocess.Popen([str(EXE),"--state-dir",str(self.state),"queue-run","--watch"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        self.processes.append(p);self.waiting(j,p,expected=None)
        with sqlite3.connect(self.state/"jobs.db") as db:
            staging=Path(db.execute("SELECT staging FROM jobs WHERE id=?",(j["id"],)).fetchone()[0])
        db.close()
        with self.assertRaises(PermissionError):
            with staging.open("r+b"):pass
        self.command("cancel",j["id"])
        deadline=time.monotonic()+5
        while True:
            try:
                with staging.open("r+b"):pass
                break
            except PermissionError:
                self.assertLess(time.monotonic(),deadline);time.sleep(.05)
        self.command("queue-pause");p.communicate(timeout=5)


if __name__=="__main__":unittest.main()
