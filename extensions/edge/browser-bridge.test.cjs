const {test}=require('node:test');
const assert=require('node:assert/strict');
const {randomUUID}=require('node:crypto');
async function fixture() {
  const state={captureConfig:{enabled:true,scope:'all'}};
  let now=10000, offline=false, failAfter=0, notices=0;
  const item={id:12,startTime:'2026-10-19T00:00:00Z',url:'blob:https://example.org/private-id',finalUrl:'blob:https://example.org/private-id',filename:'C:\\Private\\small.zip',totalBytes:-1,bytesReceived:123,state:'in_progress',danger:'safe',paused:false,canResume:false,incognito:false};
  const messages=[],actions=[],commands=[];
  const api={storage:{local:{get:async keys=>Object.fromEntries((Array.isArray(keys)?keys:[keys]).map(k=>[k,structuredClone(state[k])])),set:async value=>Object.assign(state,structuredClone(value))}},runtime:{sendNativeMessage:async(host,message)=>{
    messages.push(structuredClone(message));if(offline || failAfter && messages.length>=failAfter)throw new Error('private error URL token');
    for(const ack of message.acknowledgements){const i=commands.findIndex(c=>c.id===ack.id);if(i>=0)commands.splice(i,1);}
    return {version:4,accepted:true,request_id:message.request_id,client_id:message.client_id,commands:structuredClone(commands),desktop_requested:message.reports.length>0};
  }},downloads:{search:async()=>[structuredClone(item)],pause:async()=>{actions.push('pause');item.paused=true;item.canResume=true;},resume:async()=>{actions.push('resume');item.paused=false;},cancel:async()=>{actions.push('cancel');item.state='interrupted';item.error='USER_CANCELED';},show:async()=>actions.push('show'),acceptDanger:()=>assert.fail('must not accept browser warnings')}};
  const {createBrowserBridge}=await import('./browser-bridge.js');
  const make=()=>createBrowserBridge(api,{autoPoll:false,clock:()=>now,uuid:randomUUID,showNotice:async()=>notices++});
  const bridge=make();await bridge.ready;
  const command=action=>{const c={id:randomUUID(),transfer_id:Object.values(state.browserTransfers)[0].request_id,action,created:now};commands.push(c);return c;};
  return {state,item,api,bridge,messages,actions,commands,command,make,get notices(){return notices;},tick(ms){now+=ms;},offline(v){offline=v;},failAfter(v){failAfter=v;}};
}
test('browser-only unknown-length downloads register without replay or cancellation',async()=>{
  const f=await fixture();await f.bridge.track(f.item);const r=f.messages[0].reports[0];
  assert.equal(r.total,null);assert.equal(r.origin,'https://example.org');assert.equal(r.name,'small.zip');assert.equal(f.notices,1);assert.deepEqual(f.actions,[]);
  assert.equal(f.state.captureConfirmation.transport,'browser');assert.doesNotMatch(JSON.stringify(f.messages),/private-id|Private|Cookie|Authorization/);assert.doesNotMatch(JSON.stringify(f.state.browserTransfers),/small.zip|https:|Private/);
});
test('zero-byte, small and extension-created files are tracked; incognito stays private',async()=>{
  for(const bytes of [0,100,1000000,2097152]){const f=await fixture();Object.assign(f.item,{totalBytes:bytes,bytesReceived:bytes,state:'complete',exists:true,byExtensionId:'another'});await f.bridge.track(f.item);assert.equal(f.messages[0].reports[0].received,bytes);assert.equal(f.messages[0].reports[0].state,'completed');assert.deepEqual(f.actions,[]);}
  const f=await fixture();f.item.incognito=true;assert.equal(await f.bridge.track(f.item),false);assert.equal(f.messages.length,0);
});
test('progress and manual browser pauses are reflected honestly',async()=>{
  const f=await fixture();await f.bridge.track(f.item);f.tick(2000);f.item.bytesReceived=2123;f.item.totalBytes=9000;await f.bridge.flush();let r=f.messages.at(-1).reports[0];assert.equal(r.speed,1000);assert.equal(r.total,9000);
  f.item.paused=true;f.item.canResume=true;await f.bridge.update(f.item);r=f.messages.at(-1).reports[0];assert.equal(r.state,'paused');assert.equal(r.can_resume,true);assert.deepEqual(f.actions,[]);
});
test('only explicit desktop commands pause, resume, cancel or reveal browser items',async()=>{
  const f=await fixture();await f.bridge.track(f.item);for(const action of ['pause','resume','cancel']){f.command(action);await f.bridge.flush();}Object.assign(f.item,{state:'complete',exists:true});f.command('show');await f.bridge.flush();assert.deepEqual(f.actions,['pause','resume','cancel','show']);
});

test('stale controls report failure and cannot resume a browser security warning',async()=>{
  for(const action of ['pause','cancel','show']){const f=await fixture();await f.bridge.track(f.item);f.item.state='complete';f.item.exists=false;const cmd=f.command(action);await f.bridge.flush();assert.deepEqual(f.actions,[]);assert(f.messages.some(m=>m.acknowledgements.some(a=>a.id===cmd.id && !a.success)));}
  const f=await fixture();await f.bridge.track(f.item);f.item.danger='content';f.item.canResume=true;const cmd=f.command('resume');await f.bridge.flush();assert.deepEqual(f.actions,[]);assert(f.messages.some(m=>m.acknowledgements.some(a=>a.id===cmd.id && !a.success)));
});
test('lost acknowledgement does not replay a pause after the user resumes in Chrome',async()=>{
  const f=await fixture();await f.bridge.track(f.item);f.command('pause');f.failAfter(f.messages.length+2);await f.bridge.flush();assert.deepEqual(f.actions,['pause']);f.item.paused=false;f.failAfter(0);await f.bridge.flush();assert.deepEqual(f.actions,['pause']);assert.equal(f.item.paused,false);
});
test('worker restart acknowledges an ambiguous command instead of repeating it',async()=>{
  const f=await fixture();await f.bridge.track(f.item);const cmd=f.command('cancel');f.state.browserCommandLedger[cmd.id]={pending:true,success:false,at:10000};f.bridge.dispose();const next=f.make();await next.ready;await next.flush();assert.deepEqual(f.actions,[]);assert(f.messages.some(m=>m.acknowledgements.some(a=>a.id===cmd.id && !a.success)));next.dispose();
});
test('reused browser IDs cannot redirect stale commands to another download',async()=>{
  const f=await fixture();await f.bridge.track(f.item);f.command('cancel');f.item.startTime='2026-10-19T01:00:00Z';await f.bridge.flush();assert.deepEqual(f.actions,[]);assert(f.messages.at(-1).acknowledgements.some(a=>!a.success));
});
test('native unavailability preserves Chrome and recovers tracking later',async()=>{
  const f=await fixture();f.offline(true);await f.bridge.track(f.item);assert.deepEqual(f.actions,[]);assert.match(f.state.browserBridgeError,/unavailable/);assert.equal(f.notices,0);f.offline(false);await f.bridge.flush();assert.equal(f.notices,1);assert.equal(f.state.browserBridgeError,null);
});
test('browser warnings need user attention and are never accepted automatically',async()=>{
  const f=await fixture();f.item.danger='content';await f.bridge.track(f.item);assert.equal(f.messages[0].reports[0].state,'attention');assert.deepEqual(f.actions,[]);
});
test('turning off compatibility or disposing never cancels browser downloads',async()=>{
  const f=await fixture();await f.bridge.track(f.item);const before=f.messages.length;f.state.captureConfig.enabled=false;await f.bridge.flush();assert.equal(f.messages.length,before);assert.deepEqual(f.actions,[]);f.bridge.dispose();f.state.captureConfig.enabled=true;await f.bridge.track(f.item);assert.equal(f.messages.length,before);
});
