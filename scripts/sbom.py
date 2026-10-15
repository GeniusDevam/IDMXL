"""Resolved dependency inventory and collected notices; public release still needs an audit."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess
import uuid

ROOT=Path(__file__).resolve().parents[1]

def metadata(cargo,manifest):
    result=subprocess.run([cargo,"metadata","--locked","--offline","--format-version","1","--filter-platform","x86_64-pc-windows-msvc","--manifest-path",str(manifest)],capture_output=True,text=True,encoding="utf-8",check=True)
    return json.loads(result.stdout)

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--cargo",default="cargo");parser.add_argument("--engine",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    out=args.output;out.mkdir(parents=True,exist_ok=True)
    notices=out / "notices";notices.mkdir(exist_ok=True)
    components={};graphs=[];missing=[]
    def notice_files(ref,folder,explicit=None):
        candidates=[]
        if explicit:candidates.append(folder / explicit)
        if folder.is_dir():
            candidates.extend(p for p in folder.iterdir() if p.is_file() and re.match(r"(?i)^(licen[sc]e|copying|notice)(\.|-|$)",p.name))
        candidates=list(dict.fromkeys(p for p in candidates if p.is_file()))
        if not candidates:missing.append(ref)
        for file in candidates:
            name=hashlib.sha256(ref.encode()).hexdigest()[:16]+"-"+file.name
            (notices / name).write_bytes(file.read_bytes())
    for manifest in [ROOT / "Cargo.toml",ROOT / "apps/desktop/src-tauri/Cargo.toml"]:
        data=metadata(args.cargo,manifest);ids={}
        for package in data["packages"]:
            if package["source"] is None:continue
            ref=f"pkg:cargo/{package['name']}@{package['version']}";ids[package["id"]]=ref
            component={"type":"library","bom-ref":ref,"name":package["name"],"version":package["version"],"purl":ref,
                       "properties":[{"name":"idmxl:inventory-scope","value":"resolved Windows graph; includes build and test dependencies"}]}
            if package.get("license"):component["licenses"]=[{"expression":package["license"]}]
            components[ref]=component
            notice_files(ref,Path(package["manifest_path"]).parent,package.get("license_file"))
        for node in data["resolve"]["nodes"]:
            if node["id"] in ids:graphs.append({"ref":ids[node["id"]],"dependsOn":sorted({ids[n] for n in node["dependencies"] if n in ids})})
    frontend=ROOT / "apps/desktop"
    lock=json.loads((frontend / "package-lock.json").read_text())
    for path,package in lock["packages"].items():
        if not path:continue
        name=package.get("name") or path.split("node_modules/")[-1]
        ref=f"pkg:npm/{name.replace('@','%40')}@{package['version']}"
        component={"type":"library","bom-ref":ref,"name":name,"version":package["version"],"purl":ref,
                   "scope":"optional" if package.get("dev") else "required"}
        license=package.get("license")
        if license:component["licenses"]=[{"expression":license}]
        if package.get("integrity"):component["properties"]=[{"name":"npm:integrity","value":package["integrity"]}]
        components[ref]=component;notice_files(ref,frontend / path)
    # These native libraries are embedded by the cargo *-sys crates and need their own review.
    doctor=json.loads(subprocess.run([str(args.engine),"doctor"],capture_output=True,text=True,check=True).stdout)
    native=[("libcurl",doctor["libcurl"],"curl"),("nghttp2",doctor["nghttp2"],"MIT"),("SQLite",doctor["sqlite"],"blessing"),("zlib",doctor["zlib"],"Zlib")]
    for name,version,license in native:
        ref=f"native:{name}@{version}"
        components[ref]={"type":"library","bom-ref":ref,"name":name,"version":version,"licenses":[{"expression":license}],
                         "properties":[{"name":"idmxl:verification","value":"Reported by the linked engine binary; embedded native notices still require review"}]}
    for name,version in [("NSIS","3.13"),("nsis-tauri-utils","0.5.3")]:
        ref=f"installer:{name}@{version}"
        components[ref]={"type":"application","bom-ref":ref,"name":name,"version":version,
                         "properties":[{"name":"idmxl:inventory-scope","value":"Observed Windows bundler tools; verify these pins when upgrading Tauri CLI"}]}
    for name,scope in [("Microsoft Visual C runtime","Statically linked by static_vcruntime; compiler-runtime terms need review"),("Microsoft Windows UCRT","Windows platform prerequisite"),("Microsoft Edge WebView2 Runtime","External runtime prerequisite; installed version varies by machine")]:
        ref="platform:"+name.replace(" ","-")
        components[ref]={"type":"framework","bom-ref":ref,"name":name,"properties":[{"name":"idmxl:inventory-scope","value":scope}]}
    merged={}
    for graph in graphs:merged.setdefault(graph["ref"],set()).update(graph["dependsOn"])
    bom={"bomFormat":"CycloneDX","specVersion":"1.6","serialNumber":"urn:uuid:"+str(uuid.uuid4()),"version":1,
         "metadata":{"timestamp":datetime.datetime.now(datetime.timezone.utc).isoformat(),"component":{"type":"application","name":"IDMXL","version":"0.1.0"},
                     "properties":[{"name":"idmxl:release-status","value":"Development inventory; not a completed redistribution audit. Native versions require build review; OS/WebView2/NSIS terms are separate."}]},
         "components":sorted(components.values(),key=lambda n:n["bom-ref"]),"dependencies":[{"ref":ref,"dependsOn":sorted(deps)} for ref,deps in sorted(merged.items())]}
    (out / "sbom.cdx.json").write_text(json.dumps(bom,indent=2),encoding="utf-8")
    (out / "NOTICE-REVIEW.md").write_text("# Notice review required\n\nThis inventory includes build/test dependencies as well as runtime dependencies. Collected texts do not select a product licence or replace a distribution audit. Review embedded native sources, static-link obligations, compiler runtime, WebView2 and installer materials before distribution.\n\nPackages without a root notice file:\n\n"+"\n".join("- "+n for n in sorted(set(missing))),encoding="utf-8")
    print(json.dumps({"components":len(components),"notice_files":len(list(notices.iterdir())),"missing_root_notice":len(set(missing)),"output":str(out)}))

if __name__=="__main__":main()
