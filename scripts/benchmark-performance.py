"""Paired native-engine benchmark. Generated localhost data; optional fixed public file.

Payloads are hash checked then individually removed. Profiles, metrics and raw
results stay under --output. Never uses the application's normal profile.
"""
import argparse
import hashlib
import http.server
import json
import random
import statistics
import subprocess
import threading
import time
from pathlib import Path

PUBLIC_URL="https://download.blender.org/release/Blender4.5/blender-4.5.9-windows-x64.zip"
PUBLIC_SIZE=399051129
PUBLIC_HASH="41da973b9bf95bb312cbeff4d1982feb13259b43c821686b9bafea4dfe5477cf"
FLAGS=getattr(subprocess,"CREATE_NO_WINDOW",0)

def sha(path):
    digest=hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda:stream.read(1024*1024),b""):digest.update(block)
    return digest.hexdigest()

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version="HTTP/1.1"
    def log_message(self,*_):pass
    def do_GET(self):
        server=self.server
        value=self.headers.get("Range")
        start,end=map(int,value.removeprefix("bytes=").split("-")) if value else (0,len(server.data)-1)
        end=min(end,len(server.data)-1)
        if self.headers.get("If-Match",server.etag)!=server.etag:
            self.send_response(412);self.send_header("Content-Length","0");self.end_headers();return
        if end<start:
            self.send_response(416);self.send_header("Content-Length","0");self.end_headers();return
        if end>start and server.latency:time.sleep(server.latency)
        self.send_response(206 if value else 200)
        if value:self.send_header("Content-Range",f"bytes {start}-{end}/{len(server.data)}")
        self.send_header("ETag",server.etag)
        self.send_header("Content-Length",str(end-start+1))
        self.end_headers()
        began=time.perf_counter();sent=0
        try:
            for offset in range(start,end+1,65536):
                body=server.data[offset:min(offset+65536,end+1)]
                if end>start:
                    if server.aggregate:
                        with server.guard:
                            server.next_send=max(server.next_send,time.perf_counter())+len(body)/server.aggregate
                            deadline=server.next_send
                        time.sleep(max(0,deadline-time.perf_counter()))
                    if server.per_request:time.sleep(max(0,began+(sent+len(body))/server.per_request-time.perf_counter()))
                self.wfile.write(body);self.wfile.flush();sent+=len(body)
        except (OSError,ConnectionError):pass

def run_engine(engine,label,name,root,url,size,expected,extra,metrics):
    state=root/(name+"-"+label+"-state")
    target=root/(name+"-"+label+".zip")
    assert not state.exists() and not target.exists()
    add=subprocess.run([str(engine),"--state-dir",str(state),"add",url,"--output",str(target),"--replayable","--sha256",expected],capture_output=True,text=True,creationflags=FLAGS,timeout=30)
    if add.returncode:raise RuntimeError(add.stderr)
    job=json.loads(add.stdout)["id"]
    args=[str(engine),"--state-dir",str(state),"run",job,"--connections","4","--retries","2",*extra]
    report=root/(name+"-"+label+"-metrics.json")
    if metrics:args.extend(["--metrics",str(report)])
    start=time.perf_counter()
    result=subprocess.run(args,capture_output=True,text=True,creationflags=FLAGS,timeout=300)
    elapsed=time.perf_counter()-start
    row={"case":name,"engine":label,"seconds":elapsed,"bytes":size,"mbps":size*8/elapsed/1e6,"returncode":result.returncode,"options":extra,"verified":False}
    (root/(name+"-"+label+"-stdout.json")).write_text(result.stdout,encoding="utf-8")
    (root/(name+"-"+label+"-stderr.txt")).write_text(result.stderr,encoding="utf-8")
    if result.returncode:row["error"]=result.stderr
    else:
        view=json.loads(result.stdout)
        row.update(state=view["state"],requests=int(view["transfer_requests"]),failed_requests=int(view["failed_requests"]))
        row["verified"]=target.is_file() and target.stat().st_size==size and sha(target)==expected and view["sha256"]==expected
    if target.is_file() and row["verified"]:target.unlink()
    if report.is_file():row["metrics"]=json.loads(report.read_text())
    print(json.dumps({k:v for k,v in row.items() if k!="metrics"}),flush=True)
    return row

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--baseline",type=Path,required=True)
    parser.add_argument("--candidate",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--repeats",type=int,default=3)
    parser.add_argument("--public",action="store_true",help="Downloads the fixed 399 MB publisher file per engine/run")
    parser.add_argument("--only-public",action="store_true")
    args=parser.parse_args()
    root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
    engines={"baseline":args.baseline.resolve(),"candidate":args.candidate.resolve()}
    assert all(path.is_file() for path in engines.values())
    # Each 64 KiB block has a different SHA-derived pattern, including its index.
    data=b"".join(hashlib.sha256(i.to_bytes(8,"little")).digest()*2048 for i in range(1024))
    server=http.server.ThreadingHTTPServer(("127.0.0.1",0),Handler)
    server.daemon_threads=True
    server.data=data;server.etag='"'+hashlib.sha256(data).hexdigest()+'"'
    server.guard=threading.Lock();server.next_send=0.;server.aggregate=0;server.per_request=0;server.latency=0
    threading.Thread(target=server.serve_forever,daemon=True).start()
    cases=[] if args.only_public else [("unlimited",0,0,0,[]),("request-latency-30ms",0,0,.03,[]),("shared-configured-80mbps",10000000,0,0,[]),("per-request-1mbps",0,1000000,0,[]),("adaptive-per-request",0,1000000,0,["--adaptive"])]
    results=[];rng=random.Random(20261021)
    try:
        for case,aggregate,per_request,latency,extra in cases:
            server.aggregate=aggregate;server.per_request=per_request;server.latency=latency
            for repeat in range(args.repeats):
                order=list(engines);rng.shuffle(order)
                for label in order:
                    server.next_send=0.
                    results.append(run_engine(engines[label],label,f"{case}-{repeat+1}",root,f"http://127.0.0.1:{server.server_port}/file",len(data),hashlib.sha256(data).hexdigest(),extra,label=="candidate"))
                    (root/"results.json").write_text(json.dumps(results,indent=2)+"\n",encoding="utf-8")
        if args.public or args.only_public:
            for repeat in range(args.repeats):
                order=list(engines);rng.shuffle(order)
                for label in order:
                    results.append(run_engine(engines[label],label,f"public-{repeat+1}",root,PUBLIC_URL,PUBLIC_SIZE,PUBLIC_HASH,[],label=="candidate"))
                    (root/"results.json").write_text(json.dumps(results,indent=2)+"\n",encoding="utf-8")
    finally:server.shutdown();server.server_close()
    groups={}
    for row in results:
        case=row["case"].rsplit("-",1)[0]
        groups.setdefault(case,{}).setdefault(row["engine"],[]).append(row)
    summary={}
    for case,modes in groups.items():
        group={label:{"mean_seconds":statistics.mean(r["seconds"] for r in rows),"median_seconds":statistics.median(r["seconds"] for r in rows),"runs":len(rows),"verified":all(r["verified"] for r in rows),"mean_requests":statistics.mean(r.get("requests",0) for r in rows)} for label,rows in modes.items()}
        group["candidate_time_reduction_percent"]=100*(1-group["candidate"]["mean_seconds"]/group["baseline"]["mean_seconds"])
        summary[case]=group
    evidence={"engines":{label:{"path":str(path),"sha256":sha(path)} for label,path in engines.items()},"summary":summary,"limitations":"Paired serial release-engine runs on one laptop. Local rates are configured fixture ceilings, not a calibrated broadband simulator. Candidate instrumentation enabled; baseline lacks this feature. No IDM product comparison is implied."}
    (root/"summary.json").write_text(json.dumps(evidence,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(summary,indent=2),flush=True)
    if not all(r["verified"] for r in results):raise SystemExit("A download failed verification; inspect raw results")

if __name__=="__main__":main()
