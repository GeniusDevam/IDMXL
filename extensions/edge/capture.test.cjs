const {test}=require('node:test');
const assert=require('node:assert/strict');
async function fixture(options={}) {
  const {createCapture}=await import('./capture.js');
  const journal={captureConfig:{enabled:true,rules:[{origin:'https://example.org',prefix:'/files/'}]}};
  let download={id:9,startTime:'2026-10-16T00:00:00Z',url:'https://example.org/files/file.bin',finalUrl:'https://example.org/files/file.bin',filename:'C:\\Downloads\\file.bin',totalBytes:2097152,state:'in_progress',paused:false,danger:'safe',incognito:false};
  let phase='missing', lost=false;const calls=[];
  const api={storage:{local:{get:async key=>({[key]:structuredClone(journal[key])}),set:async value=>Object.assign(journal,structuredClone(value))}},runtime:{sendNativeMessage:async(_,r)=>{
    calls.push(r.kind);
    if (r.kind==='prepare') {if(options.prepareFails)throw new Error();phase='ready';}
    if (r.kind==='abort')phase='aborted';
    if (r.kind==='commit') {phase='committed';if(options.loseCommitReply&&!lost){lost=true;throw new Error();}}
    return {version:2,accepted:true,request_id:r.request_id,job_id:'job',phase,expires:100000};
  }},downloads:{search:async()=>[structuredClone(download)],pause:async()=>{calls.push('pause');download.paused=true;if(options.completesOnPause)download.state='complete';},resume:async()=>{calls.push('resume');download.paused=false;},cancel:async()=>{calls.push('cancel');if(options.cancelFails)throw new Error();download.state='interrupted';download.error='USER_CANCELED';}}};
  const capture=createCapture(api,()=>1000,()=> '0f417bf1-a960-462a-a1af-62d427ee655d');
  function observe(headers=[]) {capture.before({requestId:'request',url:download.url,method:'GET',requestHeaders:headers});capture.headers({requestId:'request',url:download.url,statusCode:200,responseHeaders:[{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097152'}]});}
  return {api,capture,observe,journal,calls,get download(){return download;},set download(v){download=v;},get phase(){return phase;}};
}
test('prepare, confirmed pause, cancellation and durable ownership occur in order',async()=>{
  const f=await fixture();f.observe();await f.capture.created(f.download);
  assert.deepEqual(f.calls,['pause','prepare','cancel','status','commit']);
  assert.equal(f.phase,'committed');assert.equal(Object.values(f.journal.captureRecords)[0].phase,'committed');
  assert.doesNotMatch(JSON.stringify(f.journal),/https:\/\/example.org\/files\/file|Cookie/);
  await f.capture.created(f.download);assert.equal(f.calls.filter(c=>c==='commit').length,1);
});
test('failed preparation and failed cancellation resume the browser without committing',async()=>{
  for(const option of [{prepareFails:true},{cancelFails:true}]){
    const f=await fixture(option);f.observe();await f.capture.created(f.download);
    assert.equal(f.download.paused,false);assert.equal(f.download.state,'in_progress');assert(!f.calls.includes('commit'));
    assert.equal(Object.values(f.journal.captureRecords)[0].phase,'released');
  }
});
test('browser completion wins over takeover',async()=>{
  const f=await fixture({completesOnPause:true});f.observe();await f.capture.created(f.download);
  assert.equal(f.download.state,'complete');assert(!f.calls.includes('cancel'));assert(!f.calls.includes('prepare'));
});
test('lost commit acknowledgement recovers the same native job without another cancellation',async()=>{
  const f=await fixture({loseCommitReply:true});f.observe();await f.capture.created(f.download);
  assert.equal(f.phase,'committed');assert.equal(f.calls.filter(c=>c==='cancel').length,1);
  await f.capture.recover();assert.equal(f.calls.filter(c=>c==='cancel').length,1);
});
test('worker restart before cancellation releases its pause and aborts preparation',async()=>{
  const f=await fixture();f.download.paused=true;
  f.journal.captureRecords={'9:2026-10-16T00:00:00Z':{key:'9:2026-10-16T00:00:00Z',download_id:9,start_time:f.download.startTime,request_id:'request',phase:'prepared',paused_by_us:true}};
  await f.capture.recover();assert.equal(f.download.paused,false);assert(!f.calls.includes('cancel'));assert(f.calls.includes('abort'));
});
test('private, tokenized, credentialed, ambiguous and unsupported downloads stay untouched',async()=>{
  for(const change of [{incognito:true},{danger:'uncommon'},{url:'https://example.org/export',finalUrl:'https://example.org/files/file.bin'},{finalUrl:'https://example.org/files/file.bin?token=private'},{totalBytes:500},{byExtensionId:'other'},{paused:true}]){
    const f=await fixture();f.observe();Object.assign(f.download,change);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  }
  for(const header of ['Cookie','Authorization','Range']) {const f=await fixture();f.observe([{name:header,value:'private'}]);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);}
  const f=await fixture();await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
});
