"""G0 stream-copy experiment using generated original 7680x4320 RGB and PCM.

This qualifies a direct MOV/PCM experiment, not streaming codec, HDR, HLS or site support.
No encoder, public media, upscaling, or downloaded fixture is involved.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import wave

def atom(kind,data):return struct.pack(">I4s",len(data)+8,kind.encode())+data
def full(kind,data):return atom(kind,b"\0\0\0\0"+data)
def u32(*values):return struct.pack(">"+"I"*len(values),*values)
MATRIX=u32(65536,0,0,0,65536,0,0,0,0x40000000)

def video(path):
    width,height=7680,4320
    row=bytes(v for x in range(width) for v in (x%256,(x//30)%256,128))
    size=width*height*3
    ftyp=atom("ftyp",b"qt  "+u32(0)+b"qt  ")
    offset=len(ftyp)+8
    mvhd=full("mvhd",u32(0,0,30,1,65536)+struct.pack(">H",256)+bytes(10)+MATRIX+bytes(24)+u32(2))
    tkhd=atom("tkhd",b"\0\0\0\x03"+u32(0,0,1,0,1)+bytes(8)+bytes(8)+MATRIX+u32(width<<16,height<<16))
    mdhd=full("mdhd",u32(0,0,30,1)+bytes(4))
    hdlr=full("hdlr",u32(0)+b"vide"+bytes(12)+b"IDMXL generated RGB\0")
    compressor=b"IDMXL RGB fixture";compressor=bytes([len(compressor)])+compressor;compressor=compressor.ljust(32,b"\0")
    sample=bytes(6)+struct.pack(">H",1)+bytes(16)+struct.pack(">HH",width,height)+u32(72<<16,72<<16,0)+struct.pack(">H",1)+compressor+struct.pack(">Hh",24,-1)
    stsd=full("stsd",u32(1)+atom("raw ",sample))
    stbl=atom("stbl",stsd+full("stts",u32(1,1,1))+full("stsc",u32(1,1,1,1))+full("stsz",u32(size,1))+full("stco",u32(1,offset)))
    url=atom("url ",b"\0\0\0\x01");dinf=atom("dinf",full("dref",u32(1)+url))
    minf=atom("minf",atom("vmhd",b"\0\0\0\x01"+bytes(8))+dinf+stbl)
    moov=atom("moov",mvhd+atom("trak",tkhd+atom("mdia",mdhd+hdlr+minf)))
    digest=hashlib.sha256()
    with path.open("xb") as output:
        output.write(ftyp);output.write(struct.pack(">I4s",size+8,b"mdat"))
        for _ in range(height):output.write(row);digest.update(row)
        output.write(moov)
    return digest.hexdigest()

def run(program,args):
    result=subprocess.run([str(program),*map(str,args)],capture_output=True,timeout=120)
    if result.returncode:raise RuntimeError(result.stderr.decode("utf-8",errors="replace")[:4000])
    return result.stdout

def main():
    parser=argparse.ArgumentParser();parser.add_argument("--helper-dir",type=Path,required=True);parser.add_argument("--output",type=Path,required=True);args=parser.parse_args()
    root=args.output;root.mkdir(parents=True,exist_ok=True)
    ffmpeg=args.helper_dir / "ffmpeg.exe";ffprobe=args.helper_dir / "ffprobe.exe"
    source=root / "generated-8k.mov";audio=root / "generated-audio.wav";output=root / "combined.mov";extracted=root / "extracted-audio.wav"
    video_digest=video(source)
    audio_bytes=b"".join(struct.pack("<hh",(i%400-200)*100,(i%300-150)*100) for i in range(4800))
    with wave.open(str(audio),"wb") as stream:stream.setnchannels(2);stream.setsampwidth(2);stream.setframerate(48000);stream.writeframes(audio_bytes)
    run(ffmpeg,["-nostdin","-v","error","-n","-protocol_whitelist","file","-i",source,"-protocol_whitelist","file","-i",audio,"-map","0:v:0","-map","1:a:0","-c","copy","-f","mov",output])
    metadata=json.loads(run(ffprobe,["-v","error","-protocol_whitelist","file","-show_streams","-show_packets","-show_data_hash","sha256","-of","json",output]))
    v=next(s for s in metadata["streams"] if s["codec_type"]=="video")
    a=next(s for s in metadata["streams"] if s["codec_type"]=="audio")
    assert (v["width"],v["height"],v["codec_name"])==(7680,4320,"rawvideo"),v
    assert a["codec_name"]=="pcm_s16le" and a["channels"]==2 and a["sample_rate"]=="48000",a
    packets=[p for p in metadata["packets"] if p["stream_index"]==v["index"]]
    assert len(packets)==1 and packets[0]["data_hash"]=="SHA256:"+video_digest,packets
    run(ffmpeg,["-nostdin","-v","error","-n","-protocol_whitelist","file","-i",output,"-map","0:a:0","-c","copy","-f","wav",extracted])
    with wave.open(str(extracted),"rb") as stream:assert stream.readframes(stream.getnframes())==audio_bytes
    evidence={"video":{"width":7680,"height":4320,"codec":"rawvideo","packet_sha256":video_digest,"frames":1},"audio":{"codec":"pcm_s16le","sample_rate":48000,"channels":2,"sha256":hashlib.sha256(audio_bytes).hexdigest()},"helper_version":run(ffmpeg,["-version"]).decode("utf-8"),"limitations":"Generated MOV/RGB + WAV/PCM stream-copy experiment only. Not AV1/VP9/HEVC, HDR, HLS/DASH or site qualification."}
    (root / "evidence.json").write_text(json.dumps(evidence,indent=2),encoding="utf-8")
    print(json.dumps({"result":"passed","evidence":str(root / "evidence.json")}))

if __name__=="__main__":main()
