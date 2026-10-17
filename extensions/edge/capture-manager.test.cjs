const {test}=require('node:test');
const assert=require('node:assert/strict');
function event() {
  const listeners=new Set();
  return {listeners,addListener(fn){listeners.add(fn);},removeListener(fn){listeners.delete(fn);},hasListener(fn){return listeners.has(fn);}};
}
async function fixture({enabled=true,granted=true,missingApis=false}={}) {
  const state={captureConfig:{enabled,scope:'all'}};
  const api={runtime:{id:'self',getURL:p=>'chrome-extension://self/'+p,onStartup:event(),onMessage:event()},permissions:{contains:async()=>granted,onAdded:event(),onRemoved:event()},storage:{local:{get:async key=>({[key]:structuredClone(state[key])}),set:async value=>Object.assign(state,structuredClone(value))}}};
  const downloads={onCreated:event(),onChanged:event(),onDeterminingFilename:event(),search:async()=>[]};
  const webRequest={onBeforeSendHeaders:event(),onHeadersReceived:event(),onBeforeRedirect:event(),onErrorOccurred:event()};
  if(!missingApis)Object.assign(api,{downloads,webRequest});
  const {createCaptureManager}=await import('./capture-manager.js');const manager=createCaptureManager(api);
  return {state,api,manager,downloads,webRequest,setGranted(value){granted=value;},attachApis(){Object.assign(api,{downloads,webRequest});}};
}

test('worker readiness requires real listener registration and enabled permissions',async()=>{
  const f=await fixture();assert.equal(f.downloads.onCreated.listeners.size,1);
  assert.deepEqual(await f.manager.start(),{ready:true,enabled:true,reason:'ready',listeners:8});
  assert.equal(f.state.captureRuntime.ready,true);
  f.setGranted(false);await f.manager.refresh(true);
  assert.equal(f.state.captureRuntime.ready,false);assert.equal(f.downloads.onCreated.listeners.size,0);
});

test('granting optional APIs after startup installs fresh listeners without duplicates',async()=>{
  const f=await fixture({missingApis:true,granted:false});await f.manager.start();
  assert.equal(f.state.captureRuntime.ready,false);f.setGranted(true);f.attachApis();
  assert.equal((await f.manager.refresh(true)).ready,true);
  await f.manager.refresh(true);assert.equal(f.downloads.onCreated.listeners.size,1);
  assert.equal(f.webRequest.onHeadersReceived.listeners.size,1);
});

test('disabled settings cannot claim readiness and explicit refresh stops listeners',async()=>{
  const f=await fixture({enabled:false});await f.manager.start();assert.equal(f.state.captureRuntime.reason,'off');
  assert.equal(f.downloads.onCreated.listeners.size,0);f.state.captureConfig.enabled=true;
  await f.manager.refresh(true);assert.equal(f.state.captureRuntime.ready,true);
  f.state.captureConfig.enabled=false;await f.manager.refresh(true);assert.equal(f.downloads.onCreated.listeners.size,0);
});

test('only the installed extension popup pages can request worker refresh',async()=>{
  const f=await fixture();await f.manager.start();const [message]=f.api.runtime.onMessage.listeners;
  for(const sender of [{id:'other',url:'chrome-extension://self/popup.html'},{id:'self',url:'https://example.org'},{id:'self',url:'chrome-extension://self/unrelated.html'}]) {
    assert.equal(message({kind:'capture-refresh'},sender,()=>assert.fail('unexpected reply')),false);
  }
  const reply=await new Promise(resolve=>{
    assert.equal(message({kind:'capture-state'},{id:'self',url:'chrome-extension://self/popup.html'},resolve),true);
  });assert.equal(reply.ready,true);
});
