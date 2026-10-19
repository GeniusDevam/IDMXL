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
    return {version:2,accepted:true,request_id:r.request_id,job_id:'job',phase,expires:100000,activation:options.activation};
  }},downloads:{search:async()=>[structuredClone(download)],pause:async()=>{calls.push('pause');download.paused=true;if(options.completesOnPause)download.state='complete';},resume:async()=>{calls.push('resume');download.paused=false;},cancel:async()=>{calls.push('cancel');if(options.cancelFails)throw new Error();download.state='interrupted';download.error='USER_CANCELED';}}};
  const capture=createCapture(api,()=>1000,()=> '0f417bf1-a960-462a-a1af-62d427ee655d',options.browser);
  function observe(headers=[],responseHeaders=null,requestId='request') {capture.before({requestId,url:download.finalUrl,method:'GET',requestHeaders:headers});capture.headers({requestId,url:download.finalUrl,statusCode:200,responseHeaders:responseHeaders||[{name:'ETag',value:'"one"'},{name:'Content-Length',value:String(download.totalBytes)}]});}
  return {api,capture,observe,journal,calls,messages,get download(){return download;},set download(v){download=v;},get phase(){return phase;}};
}

function assisted() {
  let owned=false;const tracked=[],updates=[];
  return {tracked,updates,owns:async()=>owned,track:async item=>{owned=true;tracked.push(structuredClone(item));return true;},update:async item=>updates.push(structuredClone(item))};
}
const settleBrowser=async(capture,delay=25)=>{await new Promise(resolve=>setTimeout(resolve,delay));await capture.idle();};

test('weakly identified downloads use browser assistance without cancellation or replay',async()=>{
  const browser=assisted(),f=await fixture({browser});f.observe([],[{name:'ETag',value:'W/"weak"'},{name:'Content-Length',value:String(f.download.totalBytes)}]);
  await f.capture.created(f.download);await settleBrowser(f.capture);assert.equal(browser.tracked.length,1);assert.deepEqual(f.calls,[]);assert.equal(f.download.state,'in_progress');
  f.observe();await f.capture.changed({id:9,totalBytes:{current:f.download.totalBytes}});assert.equal(browser.tracked.length,1);assert(browser.updates.length>=1);assert(browser.updates.every(item=>item.id===f.download.id));assert.deepEqual(f.calls,[]);
});
test('successful native takeover never creates a duplicate browser transfer',async()=>{
  const browser=assisted(),f=await fixture({browser});f.observe();await f.capture.created(f.download);await settleBrowser(f.capture);assert.equal(f.phase,'committed');assert.equal(browser.tracked.length,0);
});
test('late observed headers can finish native takeover before pending fallback claims the item',async()=>{
  const browser=assisted(),f=await fixture({browser});await f.capture.created(f.download);f.observe();await f.capture.idle();await settleBrowser(f.capture,800);assert.equal(f.phase,'committed');assert.equal(browser.tracked.length,0);
});
test('failed native preparation resumes Chrome before browser-assisted registration',async()=>{
  const browser=assisted(),f=await fixture({browser,prepareFails:true});f.observe();await f.capture.created(f.download);await settleBrowser(f.capture);assert.equal(browser.tracked.length,1);assert.equal(browser.tracked[0].paused,false);assert(!f.calls.includes('cancel'));assert(f.calls.includes('resume'));
});

test('worker restart after native release recovers browser tracking before late headers can strand it',async()=>{
  const browser=assisted(),f=await fixture({browser});
  f.journal.captureRecords={'9:2026-10-16T00:00:00Z':{key:'9:2026-10-16T00:00:00Z',download_id:9,start_time:f.download.startTime,request_id:'request',phase:'released',paused_by_us:true}};
  f.observe();await f.capture.recover();await settleBrowser(f.capture);
  assert.equal(browser.tracked.length,1);assert.equal(browser.tracked[0].paused,false);assert.deepEqual(f.calls,[]);
});
test('an ambiguous post-cancellation handoff is not relabeled as a continuing browser transfer',async()=>{
  const browser=assisted(),f=await fixture({browser});await f.capture.created(f.download);const send=f.api.runtime.sendNativeMessage;f.api.runtime.sendNativeMessage=async(host,message)=>{if(message.kind==='commit')throw new Error('lost');return send(host,message);};f.observe();await f.capture.idle();await settleBrowser(f.capture,800);assert.equal(f.download.state,'interrupted');assert.equal(Object.values(f.journal.captureRecords)[0].phase,'attention');assert.equal(browser.tracked.length,0);
});
test('completed small files are tracked and stopped capture controllers discard pending fallback',async()=>{
  const browser=assisted(),f=await fixture({browser});f.download.totalBytes=0;f.download.state='complete';await f.capture.created(f.download);await settleBrowser(f.capture);assert.equal(browser.tracked.length,1);assert.equal(browser.tracked[0].totalBytes,0);
  const second=assisted(),g=await fixture({browser:second});g.download.totalBytes=100;await g.capture.created(g.download);g.capture.stop();await settleBrowser(g.capture);assert.equal(second.tracked.length,0);
});
test('prepare, confirmed pause, cancellation and durable ownership occur in order',async()=>{
  const f=await fixture();f.observe();await f.capture.created(f.download);
  assert.deepEqual(f.calls,['pause','prepare','cancel','status','commit']);
  assert.equal(f.phase,'committed');assert.equal(Object.values(f.journal.captureRecords)[0].phase,'committed');
  assert.doesNotMatch(JSON.stringify(f.journal),/https:\/\/example.org\/files\/file|Cookie/);
  await f.capture.created(f.download);assert.equal(f.calls.filter(c=>c==='commit').length,1);
});
test('confirmation distinguishes automatic startup from an older or unavailable app',async()=>{
  for(const activation of [undefined,{queue_started:true,desktop_requested:true},{queue_started:false,desktop_requested:true}]) {
    const f=await fixture({activation});f.api.windows={create:async()=>({id:1})};f.api.runtime.getURL=p=>p;
    f.observe();await f.capture.created(f.download);
    assert.equal(f.journal.captureConfirmation.automatic,activation?.queue_started===true);
    assert.equal(f.journal.captureConfirmation.desktop_requested,activation?.desktop_requested===true);
    assert.equal(f.journal.lastCaptureAttempt.code,activation?.queue_started?'captured':'captured_manual_start');
    assert.equal(f.phase,'committed');assert.equal(f.calls.filter(c=>c==='cancel').length,1);
  }
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
  for(const header of ['Authorization','Proxy-Authorization','Range']) {const f=await fixture();f.observe([{name:header,value:'private'}]);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);}
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
  ]) {
    const f=await fixture();f.observe([],headers);await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
  }
});

test('conflicting observations of the same URL cannot cancel a browser download',async()=>{
  const f=await fixture();f.observe([],null,'one');f.observe([],[{name:'ETag',value:'"two"'},{name:'Content-Length',value:'2097152'}],'two');await f.capture.created(f.download);assert.deepEqual(f.calls,[]);
});

test('duplicate navigation/download observations can capture one identical public representation',async()=>{
  const f=await fixture();f.observe([],null,'navigation');f.observe([],null,'download');await f.capture.created(f.download);
  assert.equal(f.phase,'committed');assert.equal(f.calls.filter(c=>c==='cancel').length,1);
});

test('a public observation cannot mask a credential-bearing response at the same address',async()=>{
  const f=await fixture();f.observe([],null,'public');f.observe([{name:'Authorization',value:'private-secret'}],null,'private');
  await f.capture.created(f.download);assert.deepEqual(f.calls,[]);assert.equal(f.journal.lastCaptureAttempt.code,'ambiguous_request');
  assert.doesNotMatch(JSON.stringify(f.journal),/private-secret/);
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
  const api={storage:{local:{get:async()=>{await held;return {};}}},runtime:{onStartup:event('startup')},downloads:{onCreated:event('created'),onChanged:event('changed'),onDeterminingFilename:event('filename')},webRequest:{onBeforeSendHeaders:event('before'),onHeadersReceived:event('headers'),onBeforeRedirect:event('redirect'),onErrorOccurred:event('failed')}};
  const installed=installCapture(api);assert(hooks.created);assert(hooks.changed);assert.deepEqual(hooks.before.filter.urls,['http://*/*','https://*/*']);
  release();assert.equal(await installed,true);
});

test('missing optional APIs or a rejected listener registration can be retried cleanly',async()=>{
  const {installCapture}=await import('./capture.js');assert.equal(await installCapture({}),false);
  let removed=0;const event={addListener(){},removeListener(){removed++;}};
  const api={downloads:{},webRequest:{onBeforeSendHeaders:event,onHeadersReceived:{addListener(){throw new Error('permission missing');}}}};
  assert.equal(await installCapture(api),false);assert.equal(removed,1);
});

test('a download created before its response observation is retried when headers arrive',async()=>{
  const f=await fixture();await f.capture.created(f.download);
  assert.equal(f.journal.lastCaptureAttempt.code,'request_not_observed');assert.deepEqual(f.calls,[]);
  f.observe();await f.capture.idle();assert.equal(f.phase,'committed');
  assert.equal(f.calls.filter(c=>c==='commit').length,1);
});

test('navigation-to-download abort preserves verified metadata; real network errors discard it',async()=>{
  for(const error of ['net::ERR_ABORTED','net::ERR_CONNECTION_RESET']) {
    const f=await fixture();f.observe();f.capture.failed({requestId:'request',error});await f.capture.created(f.download);
    assert.equal(f.phase,error==='net::ERR_ABORTED'?'committed':'missing');
  }
});

test('URL fragments never enter native requests and leading-zero lengths compare numerically',async()=>{
  const f=await fixture();f.download.finalUrl+='#file-name.zip';
  f.observe([],[{name:'ETag',value:'"one"'},{name:'Content-Length',value:'02097152'}]);
  await f.capture.created(f.download);assert.equal(f.phase,'committed');
  assert.equal(f.messages.find(r=>r.kind==='prepare').url,'https://example.org/files/file.bin');
  assert.equal(f.messages.find(r=>r.kind==='prepare').total,'2097152');
});

test('skipped downloads expose a bounded reason without credentials or source addresses',async()=>{
  const f=await fixture();f.download.finalUrl+='?token=private-secret';
  f.observe([],[{name:'Content-Length',value:'2097152'}]);await f.capture.created(f.download);
  assert.equal(f.journal.lastCaptureAttempt.code,'strong_identity_missing');
  assert.doesNotMatch(JSON.stringify(f.journal),/private-secret|example\.org|https:/);
});

test('successful capture opens a confirmation once; popup failure cannot undo ownership',async()=>{
  for(const fail of [false,true]) {
    const f=await fixture();let windows=0;f.api.runtime.getURL=p=>'chrome-extension://test/'+p;
    f.api.windows={create:async info=>{windows++;assert.equal(info.type,'popup');assert.match(info.url,/capture-status\.html$/);if(fail)throw new Error('window failure');return {id:12};}};
    f.observe();await f.capture.created(f.download);await f.capture.recover();
    assert.equal(f.phase,'committed');assert.equal(windows,1);assert.equal(f.journal.captureConfirmation.job_id,'job');
    assert.equal(f.journal.captureNoticeError,fail?'confirmation_unavailable':null);
    assert.equal(f.calls.filter(c=>c==='cancel').length,1);
  }
});

test('capture confirmations reuse their own window and recover when that window was closed',async()=>{
  const {showCaptureConfirmation}=await import('./capture.js');let created=0,focused=0;const saved={captureNoticeWindow:8};
  const api={runtime:{getURL:p=>'chrome-extension://test/'+p},storage:{session:{get:async()=>saved,set:async value=>Object.assign(saved,value)}},windows:{update:async id=>{assert.equal(id,8);focused++;},create:async()=>{created++;return {id:9};}}};
  await showCaptureConfirmation(api);assert.equal(focused,1);assert.equal(created,0);
  api.windows.update=async()=>{throw new Error('closed');};await showCaptureConfirmation(api);
  assert.equal(created,1);assert.equal(saved.captureNoticeWindow,9);
});

test('public downloads can qualify despite cookies, but no cookie values leave the observer',async()=>{
  const f=await fixture();f.observe([{name:'Cookie',value:'private-cookie-value'}],[{name:'ETag',value:'"one"'},{name:'Content-Length',value:'2097152'},{name:'Set-Cookie',value:'private-response-cookie'}]);
  await f.capture.created(f.download);assert.equal(f.phase,'committed');
  assert.doesNotMatch(JSON.stringify(f.messages)+JSON.stringify(f.journal),/private-cookie-value|private-response-cookie|Cookie/);
});

test('a cookie-dependent file rejected by credential-free native verification resumes in Chrome',async()=>{
  const f=await fixture({prepareFails:true});f.observe([{name:'Cookie',value:'private-session'}]);
  await f.capture.created(f.download);assert.equal(f.download.state,'in_progress');assert.equal(f.download.paused,false);
  assert(!f.calls.includes('cancel'));assert(!f.calls.includes('commit'));
  assert.equal(f.journal.lastCaptureAttempt.code,'native_takeover_failed');
  assert.doesNotMatch(JSON.stringify(f.messages)+JSON.stringify(f.journal),/private-session/);
});

test('the early filename event considers fast downloads and releases its default-name callback once',async()=>{
  for(const fail of [false,true]) {
    const f=await fixture({prepareFails:fail});f.observe();f.download.filename='';await f.capture.created(f.download);
    let suggested=0;assert.equal(f.capture.determiningFilename({...f.download,filename:'C:\\Downloads\\early.zip'},()=>suggested++),false);
    assert.equal(suggested,1);
    await f.capture.idle();await new Promise(resolve=>setImmediate(resolve));
    assert.equal(suggested,1);assert.equal(f.phase,fail?'aborted':'committed');
    assert.equal(f.messages.find(r=>r.kind==='prepare').filename,'early.zip');
  }
  const f=await fixture();f.download.totalBytes=100;let suggested=0;
  assert.equal(f.capture.determiningFilename(f.download,()=>suggested++),false);assert.equal(suggested,1);assert.deepEqual(f.calls,[]);
});

test('a stopped capture controller cannot accept delayed events after a worker refresh',async()=>{
  const f=await fixture();f.observe();await f.capture.idle();f.capture.stop();
  await f.capture.created(f.download);await f.capture.changed({id:9,totalBytes:{current:2097152}});
  f.observe();await f.capture.idle();assert.deepEqual(f.calls,[]);
  let suggested=0;f.capture.determiningFilename(f.download,()=>suggested++);assert.equal(suggested,1);
});
