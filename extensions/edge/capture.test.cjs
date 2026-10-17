const {test}=require('node:test');
const assert=require('node:assert/strict');
async function fixture(options={}) {
  const {createCapture}=await import('./capture.js');
  const journal={captureConfig:{enabled:true,scope:'all'}};
  let download={id:9,startTime:'2026-10-16T00:00:00Z',url:'https://example.org/files/file.bin',finalUrl:'https://example.org/files/file.bin',filename:'C:\\Downloads\\file.bin',totalBytes:2097152,state:'in_progress',paused:false,danger:'safe',incognito:false};
  let phase='missing', lost=false;const calls=[],messages=[];
  const api={permissions:{contains:async()=>!options.permissionMissing},storage:{local:{get:async key=>({[key]:structuredClone(journal[key])}),set:async value=>Object.assign(journal,structuredClone(value))}},runtime:{sendNativeMessage:async(_,r)=>{
    calls.push(r.kind);messages.push(structuredClone(r));
    if (r.kind==='prepare') {if(options.prepareFails)throw new Error();phase='ready';if(options.disableDuringPrepare)journal.captureConfig.enabled=false;if(options.dangerDuringPrepare)download.danger='uncommon';if(options.urlChangesDuringPrepare)download.finalUrl+='?changed=1';}
    if (r.kind==='abort')phase='aborted';
    if (r.kind==='commit') {phase='committed';if(options.loseCommitReply&&!lost){lost=true;throw new Error();}}
    return {version:2,accepted:true,request_id:r.request_id,job_id:'job',phase,expires:100000};
  }},downloads:{search:async()=>[structuredClone(download)],pause:async()=>{calls.push('pause');download.paused=true;if(options.completesOnPause)download.state='complete';},resume:async()=>{calls.push('resume');download.paused=false;},cancel:async()=>{calls.push('cancel');if(options.cancelFails)throw new Error();download.state='interrupted';download.error='USER_CANCELED';}}};
  const capture=createCapture(api,()=>1000,()=> '0f417bf1-a960-462a-a1af-62d427ee655d');
  function observe(headers=[],responseHeaders=null,requestId='request') {capture.before({requestId,url:download.finalUrl,method:'GET',requestHeaders:headers});capture.headers({requestId,url:download.finalUrl,statusCode:200,responseHeaders:responseHeaders||[{name:'ETag',value:'"one"'},{name:'Content-Length',value:String(download.totalBytes)}]});}
  return {api,capture,observe,journal,calls,messages,get download(){return download;},set download(v){download=v;},get phase(){return phase;}};
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
test('private, credentialed and unsupported downloads stay untouched',async()=>{
  for(const change of [{incognito:true},{danger:'uncommon'},{finalUrl:'blob:https://example.org/id'},{finalUrl:'https://user:password@example.org/file'},{totalBytes:500},{byExtensionId:'other'},{paused:true}]){
    const f=await fixture();f.observe();Object.assign(f.download,change);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  }
  for(const header of ['Cookie','Authorization','Range']) {const f=await fixture();f.observe([{name:header,value:'private'}]);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);}
  const f=await fixture();await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
});

test('all-sites mode captures an unrelated host without folder rules',async()=>{
  const f=await fixture();f.download.url=f.download.finalUrl='https://cdn.example.net/releases/package.zip';f.observe();
  await f.capture.created(f.download);assert.equal(f.phase,'committed');
});

test('exactly 1 MB and smaller stay in the browser; one byte over is captured',async()=>{
  for(const size of [999999,1000000,1000001,1048575]) {
    const f=await fixture();f.download.totalBytes=size;f.observe();await f.capture.created(f.download);
    assert.equal(f.phase,size>1000000?'committed':'missing');
  }
});

test('redirected final URLs and reusable query links are handed off without persisting the URL',async()=>{
  for(const source of ['https://cdn.example.net/file?token=secret&download=1','http://example.net/public/file']) {
    const f=await fixture();f.download.finalUrl=source;f.observe();await f.capture.created(f.download);
    assert.equal(f.phase,'committed');assert.equal(f.messages.find(r=>r.kind==='prepare').url,source);
    assert.doesNotMatch(JSON.stringify(f.journal),/token=secret|cdn\.example|http:\/\/example\.net/);
  }
});

test('late size and filename updates are considered; pause-only updates are ignored',async()=>{
  const f=await fixture();f.observe();f.download.totalBytes=-1;f.download.filename='';
  await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  f.download.totalBytes=2097152;await f.capture.changed({id:9,totalBytes:{current:2097152}});assert.deepEqual(f.calls,[]);
  f.download.filename='C:\\Downloads\\late.zip';await f.capture.changed({id:9,filename:{current:f.download.filename}});
  assert.equal(f.phase,'committed');assert.equal(f.messages.find(r=>r.kind==='prepare').filename,'late.zip');
  const count=f.calls.length;await f.capture.changed({id:9,paused:{current:false}});assert.equal(f.calls.length,count);
});

test('disabled capture and legacy folder boundaries remain respected',async()=>{
  for(const config of [{enabled:false,scope:'all'},{enabled:true,rules:[{origin:'https://elsewhere.org',prefix:'/'}]}]) {
    const f=await fixture();f.journal.captureConfig=config;f.observe();await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  }
  const f=await fixture();f.journal.captureConfig={enabled:true,rules:[{origin:'https://example.org',prefix:'/files/'}]};
  f.observe();await f.capture.created(f.download);assert.equal(f.phase,'committed');
});

test('missing or revoked all-site permission leaves the browser download untouched',async()=>{
  const f=await fixture({permissionMissing:true});f.observe();await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  const g=await fixture();g.observe();const send=g.api.runtime.sendNativeMessage;
  g.api.runtime.sendNativeMessage=async(host,r)=>{const reply=await send(host,r);if(r.kind==='prepare')g.api.permissions.contains=async()=>false;return reply;};
  await g.capture.created(g.download);assert(!g.calls.includes('cancel'));assert.equal(g.download.paused,false);
});

test('turning capture off, changed danger or a changed source during preparation releases the browser',async()=>{
  for(const option of [{disableDuringPrepare:true},{dangerDuringPrepare:true},{urlChangesDuringPrepare:true}]) {
    const f=await fixture(option);f.observe();await f.capture.created(f.download);
    assert(!f.calls.includes('cancel'));assert(!f.calls.includes('commit'));assert.equal(f.download.paused,false);assert.equal(f.phase,'aborted');
  }
});

test('unverified, weak, compressed, duplicate or mismatching response identities are not captured',async()=>{
  for(const headers of [
    [{name:'Content-Length',value:'2097152'}],
    [{name:'ETag',value:'W/"weak"'},{name:'Content-Length',value:'2097152'}],
    [{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097152'},{name:'Content-Encoding',value:'gzip'}],
    [{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097152'},{name:'Content-Encoding',binaryValue:[1]}],
    [{name:'ETag',value:'"one"'},{name:'ETag',value:'"two"'},{name:'Content-Length',value:'2097152'}],
    [{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097153'}],
    [{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097152'},{name:'Set-Cookie',value:'private'}],
  ]) {
    const f=await fixture();f.observe([],headers);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  }
});

test('parallel observations of the same URL are ambiguous and cannot cancel a browser download',async()=>{
  const f=await fixture();f.observe([],null,'one');f.observe([],null,'two');await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
});

test('concurrent creation and change events commit only once',async()=>{
  const f=await fixture();f.observe();await Promise.all([f.capture.created(f.download),f.capture.changed({id:9,totalBytes:{current:2097152}})]);
  assert.equal(f.calls.filter(c=>c==='cancel').length,1);assert.equal(f.calls.filter(c=>c==='commit').length,1);
});

test('expired preparation after browser cancellation renews the same final URL before commit',async()=>{
  const f=await fixture();f.download.finalUrl+='?token=secret';f.observe();await f.capture.created(f.download);
  const record=Object.values(f.journal.captureRecords)[0];record.phase='cancel_requested';
  const send=f.api.runtime.sendNativeMessage;let expire=true;
  f.api.runtime.sendNativeMessage=async(host,r)=>{const reply=await send(host,r);if(r.kind==='status'&&expire){reply.phase='expired';expire=false;}return reply;};
  await f.capture.recover();assert.equal(f.phase,'committed');
  assert.equal(f.messages.filter(r=>r.kind==='prepare').length,2);assert.equal(f.messages.at(-2).url,f.download.finalUrl);
  assert.equal(f.calls.filter(c=>c==='cancel').length,1);
});

test('download listeners register before asynchronous recovery on a cold worker start',async()=>{
  const {installCapture}=await import('./capture.js');const hooks={};
  const event=name=>({addListener(fn,filter){hooks[name]={fn,filter};},removeListener(){delete hooks[name];}});
  let release;const held=new Promise(resolve=>release=resolve);
  const api={storage:{local:{get:async()=>{await held;return {};}}},runtime:{onStartup:event('startup')},downloads:{onCreated:event('created'),onChanged:event('changed')},webRequest:{onBeforeSendHeaders:event('before'),onHeadersReceived:event('headers'),onBeforeRedirect:event('redirect'),onErrorOccurred:event('failed')}};
  const installed=installCapture(api);assert(hooks.created);assert(hooks.changed);assert.deepEqual(hooks.before.filter.urls,['http://*/*','https://*/*']);
  release();assert.equal(await installed,true);
});

test('missing optional APIs or a rejected listener registration can be retried cleanly',async()=>{
  const {installCapture}=await import('./capture.js');assert.equal(await installCapture({}),false);
  let removed=0;const event={addListener(){},removeListener(){removed++;}};
  const api={downloads:{},webRequest:{onBeforeSendHeaders:event,onHeadersReceived:{addListener(){throw new Error('permission missing');}}}};
  assert.equal(await installCapture(api),false);assert.equal(removed,1);
});
