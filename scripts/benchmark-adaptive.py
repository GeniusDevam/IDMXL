"""Bounded localhost experiment, not a general acceleration claim.

Each request has its own 512 KiB/s fixture limit. This intentionally models a
server where parallel requests can help; it does not model a shared bottleneck.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import threading
import time

DATA=bytes(range(256))*65536
EXPECTED=hashlib.sha256(DATA).hexdigest()
RATE=512*1024
class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version="HTTP/1.1"
    def log_message(self,*_):pass
    def do_GET(self):
        start,end=map(int,self.headers["Range"].removeprefix("bytes=").split("-"))
        end=min(end,len(DATA)-1);count=end-start+1
        with self.server.guard:
            self.server.active+=count>1;self.server.peak=max(self.server.peak,self.server.active)
        counted=count>1
        try:
            self.send_response(206);self.send_header("Content-Range",f"bytes {start}-{end}/{len(DATA)}")
            self.send_header("ETag",'"generated-benchmark-v1"');self.send_header("Content-Length",str(count));self.send_header("Connection","close");self.end_headers()
            began=time.monotonic();sent=0
            while sent<count:
                size=min(16384,count-sent)
                if count>1:time.sleep(max(0,began+(sent+size)/RATE-time.monotonic()))
                # A finished response can remain in this server thread after the
                # client starts its next lease. Count outstanding body work,
                # releasing it immediately before the final write, not after a sleep.
                if counted and sent+size==count:
                    with self.server.guard:self.server.active-=1
                    counted=False
                self.wfile.write(DATA[start+sent:start+sent+size]);self.wfile.flush();sent+=size
        except OSError:pass
        finally:
            if counted:
                with self.server.guard:self.server.active-=1

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--engine",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    engine=args.engine.resolve();server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
    server.guard=threading.Lock();server.active=0;server.peak=0
    threading.Thread(target=server.serve_forever,daemon=True).start()
    results=[]
    try:
        with tempfile.TemporaryDirectory(prefix="idmxl-benchmark-") as temp:
            directory=Path(temp)
            for name,options in [("fixed_one",["--connections","1"]),("adaptive_max_four",["--connections","4","--adaptive"])]:
                with server.guard:server.peak=0
                output=directory / (name+".bin");state=directory / (name+"-state")
                add=subprocess.run([str(engine),"--state-dir",str(state),"add",f"http://127.0.0.1:{server.server_port}/file","--output",str(output),"--replayable","--sha256",EXPECTED],capture_output=True,text=True,encoding="utf-8",check=True)
                job=json.loads(add.stdout);began=time.monotonic()
                run=subprocess.run([str(engine),"--state-dir",str(state),"run",job["id"],*options],capture_output=True,text=True,encoding="utf-8",timeout=120,check=True)
                elapsed=time.monotonic()-began;result=json.loads(run.stdout)
                assert result["state"]=="completed" and result["sha256"]==EXPECTED
                assert hashlib.sha256(output.read_bytes()).hexdigest()==EXPECTED
                with server.guard:peak=server.peak
                assert 1<=peak<=4
                if name.startswith("adaptive"):assert peak>=2,"Controller never increased admission"
                results.append({"mode":name,"seconds":round(elapsed,3),"peak_active_requests":peak,"sha256":EXPECTED})
                print(json.dumps(results[-1]),flush=True)
                time.sleep(.2)
    finally:server.shutdown();server.server_close()
    evidence={"platform":platform.platform(),"processor":os.environ.get("PROCESSOR_IDENTIFIER",platform.processor()),"logical_cpus":os.cpu_count(),
              "bytes":str(len(DATA)),"per_request_fixture_bytes_per_second":str(RATE),"runs":results,
              "observed_ratio":round(results[0]["seconds"]/results[1]["seconds"],3),
              "limitations":"One warm development-machine localhost run per mode, with an artificial per-request bottleneck. Not a browser comparison or universal speedup, and not shared-bandwidth/disk/H2 tuning qualification."}
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(evidence,indent=2),encoding="utf-8")

if __name__=="__main__":main()
