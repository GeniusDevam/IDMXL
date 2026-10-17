const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
function fixture(reply) {
  const hooks={},calls=[],storage=[];
  const chrome={permissions:{onAdded:{addListener:()=>{}}},runtime:{onInstalled:{addListener:fn=>hooks.installed=fn},sendNativeMessage:async(host,request)=>{calls.push({host,request});return reply(request);}},
    contextMenus:{onClicked:{addListener:fn=>hooks.clicked=fn},removeAll:fn=>fn(),create:info=>hooks.menu=info},
    storage:{local:{set:async value=>storage.push(value)}},action:{setBadgeText:async()=>{},setBadgeBackgroundColor:async()=>{}}};
  vm.runInNewContext(fs.readFileSync(__dirname+'/background.js','utf8').replace(/^import .*$/m,''),{chrome,URL,createCaptureManager:()=>({start:async()=>{}}),crypto:{randomUUID:()=> '0f417bf1-a960-462a-a1af-62d427ee655d'}});
  return {hooks,calls,storage};
}
test('only an explicit valid link context-menu event can invoke the host',async()=>{
  const f=fixture(r=>({version:1,accepted:true,request_id:r.request_id,job_id:'job-1'}));
  f.hooks.installed();assert.equal(f.hooks.menu.contexts[0],'link');
  for(const linkUrl of ['file:///C:/private','https://user:secret@example.org/file','not a URL'])await f.hooks.clicked({menuItemId:'idmxl-send-link',linkUrl});
  await f.hooks.clicked({menuItemId:'unrelated',linkUrl:'https://example.org/file'});
  assert.equal(f.calls.length,0);
  await f.hooks.clicked({menuItemId:'idmxl-send-link',linkUrl:'https://example.org/file.zip?token=secret'});
  assert.equal(f.calls.length,1);assert.equal(f.calls[0].host,'dev.idmxl.host');
  assert.equal(f.calls[0].request.filename,'file.zip');assert.equal(f.calls[0].request.url,'https://example.org/file.zip?token=secret');
  assert.match(f.storage[0].lastStatus,/Queued/);assert.doesNotMatch(JSON.stringify(f.storage),/token|secret/);
});
test('bad acknowledgement and native errors retain no source URL in local status',async()=>{
  for(const reply of [()=>({version:1,accepted:true,request_id:'wrong',job_id:'job'}),()=>{throw new Error('native failure')}]){
    const f=fixture(reply);await f.hooks.clicked({menuItemId:'idmxl-send-link',linkUrl:'https://example.org/file?secret=token'});
    assert.match(f.storage[0].lastStatus,/failed/);assert.doesNotMatch(JSON.stringify(f.storage),/secret|token/);
  }
});
