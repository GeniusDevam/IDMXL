"""Optional real storage qualification. Requires >6 GiB free in the checkout."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest
import time
from test_transfers import EXE, ROOT

@unittest.skipUnless(os.environ.get("IDMXL_RESOURCE_TESTS"),"Set IDMXL_RESOURCE_TESTS=1 for >4 GiB / long-path fixture")
class ResourceTests(unittest.TestCase):
    def test_real_file_above_four_gib_and_long_unicode_destination(self):
        total=(1<<32)+257
        scratch=ROOT / ".test-output";scratch.mkdir(exist_ok=True)
        self.assertGreater(shutil.disk_usage(scratch).free,6<<30,"At least 6 GiB free is required")
        block_size=65536
        def block_at(index):return hashlib.sha256(index.to_bytes(8,"little")).digest()*(block_size//32)
        digest=hashlib.sha256()
        for index in range(total//block_size):digest.update(block_at(index))
        digest.update(block_at(total//block_size)[:total%block_size]);expected=digest.hexdigest()
        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version="HTTP/1.1"
            def log_message(self,*_):pass
            def do_GET(self):
                start=0;end=total-1;byte_range=self.headers.get("Range")
                if byte_range:start,end=map(int,byte_range.removeprefix("bytes=").split("-"));end=min(end,total-1)
                self.send_response(206 if byte_range else 200)
                self.send_header("ETag",'"large-position-pattern-v1"');self.send_header("Content-Length",str(end-start+1))
                if byte_range:self.send_header("Content-Range",f"bytes {start}-{end}/{total}")
                self.end_headers()
                remaining=end-start+1;position=start
                try:
                    while remaining:
                        index,within=divmod(position,block_size)
                        count=min(remaining,block_size-within)
                        self.wfile.write(block_at(index)[within:within+count]);remaining-=count;position+=count
                except OSError:pass
        server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        try:
            with tempfile.TemporaryDirectory(dir=scratch,prefix="large-") as temp:
                folder=Path(temp)
                for i in range(6):folder=folder / ("long-folder-"+str(i)+"-"+"x"*32)
                folder.mkdir(parents=True)
                destination=folder / "generated-文件-4GiB-plus.bin"
                state=Path(temp) / "state"
                add=subprocess.run([str(EXE),"--state-dir",str(state),"add",f"http://127.0.0.1:{server.server_port}/large","--output",str(destination),"--replayable","--sha256",expected],capture_output=True,text=True,encoding="utf-8",timeout=20)
                self.assertEqual(add.returncode,0,add.stderr)
                job=json.loads(add.stdout)
                started=time.monotonic()
                process=subprocess.Popen([str(EXE),"--state-dir",str(state),"run",job["id"],"--connections","4"],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding="utf-8")
                try:
                    while True:
                        try:
                            stdout,stderr=process.communicate(timeout=15);break
                        except subprocess.TimeoutExpired:
                            status=subprocess.run([str(EXE),"--state-dir",str(state),"show",job["id"]],capture_output=True,text=True,encoding="utf-8",timeout=15)
                            view=json.loads(status.stdout)
                            print(f"\nLarge-file progress: {view['state']}, {view['durable_bytes']} durable bytes",flush=True)
                            if time.monotonic()-started>600:raise
                finally:
                    if process.poll() is None:process.kill();process.communicate(timeout=15)
                self.assertEqual(process.returncode,0,stderr)
                result=json.loads(stdout)
                self.assertEqual(result["total_bytes"],str(total));self.assertEqual(result["durable_bytes"],str(total))
                self.assertEqual(result["sha256"],expected);self.assertEqual(destination.stat().st_size,total)
                print(f"\nResource fixture: {total} bytes, SHA256 {expected}, path {len(str(destination))} characters",flush=True)
                evidence=ROOT / ".local/resource-evidence.json";evidence.parent.mkdir(exist_ok=True)
                evidence.write_text(json.dumps({"total_bytes":str(total),"sha256":expected,"path_characters":len(str(destination)),"elapsed_seconds":round(time.monotonic()-started,2),"state":result["state"],"fixture":"Distinct SHA-256-derived pattern per 64 KiB block, including offset bits above 4 GiB"},indent=2),encoding="utf-8")
        finally:server.shutdown();server.server_close()
