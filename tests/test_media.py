"""Original fixtures; no account, public media or production signing credentials."""
import collections, hashlib, http.server, importlib.util, json, os, sqlite3, struct, subprocess, tempfile, threading, time, unittest, wave
from pathlib import Path
from urllib.parse import urlsplit
ROOT=Path(__file__).resolve().parents[1]
EXE=ROOT/'target/debug/idmxl.exe'
HELPERS=EXE.parent/'media'
ENCODER=Path(os.environ.get('IDMXL_FIXTURE_ENCODER',ROOT/'../../work/media-build/fixture-encoder/ffmpeg.exe'))

def run(program,args,**kwargs):
    result=subprocess.run([str(program),*map(str,args)],capture_output=True,timeout=120,**kwargs)
    if result.returncode:raise AssertionError(result.stderr.decode(errors='replace')[:2000] if isinstance(result.stderr,bytes) else result.stderr[:2000])
    return result.stdout

class Server(http.server.BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self,*_):pass
    def handle(self):
        try:super().handle()
        except (ConnectionResetError,ConnectionAbortedError):pass
    def do_GET(self):
        path=urlsplit(self.path).path;self.server.calls[path]+=1
        value=self.server.files.get(path)
        if value is None:self.send_error(404);return
        if getattr(self.server,'cooldowns',{}).get(path,0)>0 and self.headers.get('Range')!='bytes=0-0':
            self.server.cooldowns[path]-=1
            self.send_response(503);self.send_header('Retry-After','1');self.send_header('Content-Length','0');self.end_headers();return
        data=value.read_bytes() if isinstance(value,Path) else value
        start,end=0,len(data)-1;range_header=self.headers.get('Range')
        if range_header:
            start,end=map(int,range_header.removeprefix('bytes=').split('-'));end=min(end,len(data)-1)
        self.send_response(206 if range_header else 200)
        self.send_header('Content-Length',str(end-start+1));self.send_header('ETag','"'+hashlib.sha256(data).hexdigest()+'"')
        if range_header:self.send_header('Content-Range',f'bytes {start}-{end}/{len(data)}')
        self.end_headers()
        try:
            for offset in range(start,end+1,65536):
                self.wfile.write(data[offset:min(offset+65536,end+1)])
                if self.server.delay:time.sleep(self.server.delay)
        except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError):pass

@unittest.skipUnless((HELPERS/'ffmpeg.exe').is_file() and ENCODER.is_file(),'Run scripts/test.ps1 -Media to build the controlled helpers and fixture-only encoder.')
class MediaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp=tempfile.TemporaryDirectory();cls.root=Path(cls.temp.name)
        spec=importlib.util.spec_from_file_location('original_media',ROOT/'scripts/media-experiment.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        cls.video=cls.root/'original-8k.mov';cls.video_digest=module.video(cls.video)
        cls.audio=cls.root/'original.wav';pcm=b''.join(struct.pack('<hh',(i%400-200)*100,(i%300-150)*100) for i in range(96000))
        with wave.open(str(cls.audio),'wb') as w:w.setnchannels(2);w.setsampwidth(2);w.setframerate(48000);w.writeframes(pcm)
        cls.aac=cls.root/'original.aac';run(ENCODER,['-nostdin','-v','error','-n','-i',cls.audio,'-c:a','aac','-b:a','128k','-f','adts',cls.aac])
        cls.fragmented=cls.root/'fragmented.mp4';run(HELPERS/'ffmpeg.exe',['-nostdin','-v','error','-n','-i',cls.aac,'-c','copy','-bsf:a','aac_adtstoasc','-movflags','frag_keyframe+empty_moov+default_base_moof','-frag_duration','1000000','-f','mp4',cls.fragmented])
        data=cls.fragmented.read_bytes();offset=0;initial=bytearray();fragments=[];current=bytearray()
        while offset<len(data):
            size,kind=struct.unpack('>I4s',data[offset:offset+8]);assert size>=8
            atom=data[offset:offset+size];offset+=size
            if kind==b'moof':
                if current:fragments.append(bytes(current));current.clear()
                current.extend(atom)
            elif current:current.extend(atom)
            else:initial.extend(atom)
        if current:fragments.append(bytes(current))
        cls.files={'/video.mov':cls.video,'/audio.wav':cls.audio,'/audio.aac':cls.aac,'/init.mp4':bytes(initial)}
        for index,fragment in enumerate(fragments,1):cls.files[f'/seg-{index}.m4s']=fragment
        cls.segment_count=len(fragments)
        cls.server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Server);cls.server.files=cls.files;cls.server.calls=collections.Counter();cls.server.delay=0
        cls.thread=threading.Thread(target=cls.server.serve_forever,daemon=True);cls.thread.start();cls.base=f'http://127.0.0.1:{cls.server.server_port}'
    @classmethod
    def tearDownClass(cls):cls.server.shutdown();cls.server.server_close();cls.thread.join();cls.temp.cleanup()
    def setUp(self):self.case=tempfile.TemporaryDirectory();self.directory=Path(self.case.name);self.state=self.directory/'state';self.server.delay=0;self.server.cooldowns={}
    def tearDown(self):self.server.delay=0;self.case.cleanup()
    def command(self,*args,input=None,success=True,exe=EXE,env=None):
        result=subprocess.run([str(exe),'--state-dir',str(self.state),*map(str,args)],input=json.dumps(input) if input is not None else None,text=True,capture_output=True,timeout=120,env=env)
        if success:
            detail=''
            if result.returncode and (self.state/'jobs.db').exists():
                c=sqlite3.connect(self.state/'jobs.db');detail=repr(c.execute('SELECT state,error,mode,total,durable FROM jobs').fetchall());c.close()
            if result.returncode:
                for path in self.directory.glob('.idmxl-media-*/*.media'):
                    if path.name.startswith(('track-','output-')):
                        metadata=json.loads(run(HELPERS/'ffprobe.exe',['-v','error','-count_packets','-show_streams','-show_format','-of','json',path]))
                        detail+='\n'+path.name+json.dumps({'streams':[{k:v for k,v in stream.items() if k in ['codec_type','codec_name','sample_rate','channels','nb_read_packets','side_data_list','duration','duration_ts','time_base']} for stream in metadata.get('streams',[])],'duration':metadata.get('format',{}).get('duration')})
            self.assertEqual(result.returncode,0,result.stderr+detail);return json.loads(result.stdout)
        self.assertNotEqual(result.returncode,0);return result
    def add(self,path,extension='mov',audio_only=False,audio_url=None):
        inspection=self.command('media-inspect',input={'url':self.base+path})
        return self.command('media-add',input={'url':self.base+path,'selection':inspection['choices'][0]['id'],'audio_selection':None,'audio_url':audio_url,'destination':str(self.directory/f'output.{extension}'),'container':extension,'audio_only':audio_only})
    def packets(self,path,kind):
        info=json.loads(run(HELPERS/'ffprobe.exe',['-v','error','-select_streams',kind,'-show_packets','-show_data_hash','sha256','-of','json',path]))
        return [p['data_hash'] for p in info['packets']]
    def aac_payloads(self):
        data=self.aac.read_bytes();offset=0;hashes=[]
        while offset<len(data):
            size=((data[offset+3]&3)<<11)|(data[offset+4]<<3)|(data[offset+5]>>5)
            header=7 if data[offset+1]&1 else 9
            hashes.append('SHA256:'+hashlib.sha256(data[offset+header:offset+size]).hexdigest());offset+=size
        return hashes
    def test_direct_original_8k_and_selected_audio_are_preserved(self):
        job=self.add('/video.mov',audio_url=self.base+'/audio.wav');done=self.command('run',job['id']);self.assertEqual(done['state'],'completed')
        output=Path(done['destination']);self.assertEqual(self.packets(output,'v'),['SHA256:'+self.video_digest]);extracted=self.directory/'extracted.wav';run(HELPERS/'ffmpeg.exe',['-nostdin','-v','error','-n','-i',output,'-map','0:a:0','-c','copy','-f','wav',extracted]);
        with wave.open(str(extracted),'rb') as actual, wave.open(str(self.audio),'rb') as expected:self.assertEqual(actual.readframes(actual.getnframes()),expected.readframes(expected.getnframes()))
        self.assertEqual(len(self.command('list')),1);self.assertNotIn('source-0.media',json.dumps(self.command('list')))
    def test_hls_clear_vod_byte_ranges_and_audio_only_keep_all_packets(self):
        data=self.aac.read_bytes();frames=[];offset=0
        while offset<len(data):
            size=((data[offset+3]&3)<<11)|(data[offset+4]<<3)|(data[offset+5]>>5);self.assertGreaterEqual(size,7);frames.append(size);offset+=size
        boundary=sum(frames[:len(frames)//2]);duration=len(frames)*1024/48000
        self.files['/audio.m3u8']=f'#EXTM3U\n#EXT-X-TARGETDURATION:3\n#EXTINF:{duration/2},\n#EXT-X-BYTERANGE:{boundary}@0\naudio.aac\n#EXTINF:{duration/2},\n#EXT-X-BYTERANGE:{len(data)-boundary}\naudio.aac\n#EXT-X-ENDLIST\n'.encode()
        before=self.server.calls['/audio.aac'];job=self.add('/audio.m3u8','m4a',True);done=self.command('run',job['id']);self.assertEqual(done['state'],'completed');self.assertEqual(self.packets(Path(done['destination']),'a'),self.aac_payloads())
        self.assertLessEqual(self.server.calls['/audio.aac']-before,3)
        self.files['/encrypted.m3u8']=b'#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI="key"\n#EXTINF:1,\naudio.aac\n#EXT-X-ENDLIST\n'
        self.command('media-inspect',input={'url':self.base+'/encrypted.m3u8'},success=False)
    def test_static_dash_selected_segment_list_preserves_aac_packets(self):
        segments=''.join(f'<SegmentURL media="seg-{i}.m4s"/>' for i in range(1,self.segment_count+1))
        self.files['/audio.mpd']=f'<MPD type="static"><Period><AdaptationSet contentType="audio"><Representation id="original-aac" bandwidth="128000"><SegmentList><Initialization sourceURL="init.mp4"/>{segments}</SegmentList></Representation></AdaptationSet></Period></MPD>'.encode()
        job=self.add('/audio.mpd','m4a',True);done=self.command('run',job['id']);self.assertEqual(done['state'],'completed');self.assertEqual(self.packets(Path(done['destination']),'a'),self.aac_payloads())
    def test_media_pause_and_process_restart_preserve_history_and_output(self):
        job=self.add('/video.mov');self.server.delay=0.005
        worker=subprocess.Popen([str(EXE),'--state-dir',str(self.state),'run',job['id']],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            deadline=time.monotonic()+20
            while time.monotonic()<deadline:
                view=self.command('show',job['id'])
                if int(view['received_bytes'])>65536:break
                time.sleep(0.05)
            else:self.fail('Media progress never appeared')
            self.command('pause',job['id']);worker.communicate(timeout=20);self.assertEqual(self.command('show',job['id'])['state'],'paused')
            self.server.delay=0;done=self.command('run',job['id']);self.assertEqual(done['state'],'completed');self.assertEqual(self.packets(Path(done['destination']),'v'),['SHA256:'+self.video_digest])
        finally:
            if worker.poll() is None:worker.kill();worker.communicate()
    def test_segment_cooldown_waits_and_resumes_parent_without_duplicate_history(self):
        segments=''.join(f'<SegmentURL media="seg-{i}.m4s"/>' for i in range(1,self.segment_count+1))
        self.files['/recovery.mpd']=f'<MPD type="static"><Period><AdaptationSet contentType="audio"><Representation id="aac" bandwidth="128000"><SegmentList><Initialization sourceURL="init.mp4"/>{segments}</SegmentList></Representation></AdaptationSet></Period></MPD>'.encode()
        job=self.add('/recovery.mpd','m4a',True);self.server.cooldowns={'/seg-1.m4s':1}
        worker=subprocess.Popen([str(EXE),'--state-dir',str(self.state),'run',job['id']],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                view=self.command('show',job['id'])
                if view['state']=='waiting':break
                self.assertIsNone(worker.poll());time.sleep(.05)
            else:self.fail('Media parent never waited for its throttled component')
            self.assertFalse(Path(view['destination']).exists());self.assertEqual(len(self.command('list')),1)
            out,err=worker.communicate(timeout=25);self.assertEqual(worker.returncode,0,err)
            done=json.loads(out);self.assertEqual(done['state'],'completed');self.assertEqual(self.packets(Path(done['destination']),'a'),self.aac_payloads())
        finally:
            if worker.poll() is None:worker.kill();worker.communicate()
    def test_unsigned_installer_is_not_accepted_as_an_update(self):
        path=self.directory/'fake.exe';path.write_bytes(b'not a signed installer')
        result=subprocess.run(['powershell.exe','-NoProfile','-NonInteractive','-File',str(ROOT/'scripts/verify-authenticode.ps1'),'-Installer',str(path),'-Thumbprint','a'*40],capture_output=True,timeout=30)
        self.assertNotEqual(result.returncode,0)
    @unittest.skipUnless(os.environ.get('IDMXL_FAULT_EXE'),'Run -Faults for media publication recovery')
    def test_media_publication_recovers_with_the_owned_output_after_process_loss(self):
        job=self.add('/audio.aac','m4a',True)
        env=dict(os.environ,IDMXL_FAULT='before_media_assembly',IDMXL_TEST_MEDIA_DIR=str(HELPERS))
        first=self.command('run',job['id'],exe=Path(os.environ['IDMXL_FAULT_EXE']),env=env,success=False);self.assertEqual(first.returncode,86,first.stderr)
        env['IDMXL_FAULT']='after_rename'
        second=self.command('run',job['id'],exe=Path(os.environ['IDMXL_FAULT_EXE']),env=env,success=False);self.assertEqual(second.returncode,86,second.stderr)
        done=self.command('run',job['id']);self.assertEqual(done['state'],'completed');self.assertEqual(self.packets(Path(done['destination']),'a'),self.aac_payloads())
        if os.name=='nt':self.assertIn('ZoneId=3',Path(done['destination']+':Zone.Identifier').read_text())
    @unittest.skipUnless(os.environ.get('IDMXL_FAULT_EXE'),'Run -Faults for retained-track corruption checks')
    def test_changed_completed_track_is_rejected_before_assembly(self):
        job=self.add('/audio.aac','m4a',True)
        env=dict(os.environ,IDMXL_FAULT='before_media_assembly',IDMXL_TEST_MEDIA_DIR=str(HELPERS))
        result=self.command('run',job['id'],exe=Path(os.environ['IDMXL_FAULT_EXE']),env=env,success=False);self.assertEqual(result.returncode,86)
        source=next(self.directory.glob('.idmxl-media-*/source-*.media'));source.write_bytes(source.read_bytes()+b'changed')
        result=self.command('run',job['id'],success=False);self.assertIn('media_verification_failed',result.stderr);self.assertFalse(Path(job['destination']).exists());self.assertTrue(source.is_file())

if os.environ.get('IDMXL_REQUIRE_MEDIA_TESTS') and (not ENCODER.is_file() or not (HELPERS/'ffmpeg.exe').is_file()):
    raise RuntimeError('Required media fixture tools are missing.')
