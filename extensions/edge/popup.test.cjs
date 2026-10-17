const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');

async function popup(granted,workerAvailable=true) {
  const settings=await import('./capture-settings.js');
  const diagnostics=await import('./capture-diagnostics.js');
  const elements=new Map();const state={captureConfig:{enabled:false,scope:'all'}};let permitted=false;
  const element=id=>{
    if(!elements.has(id))elements.set(id,{textContent:'',disabled:false,hooks:{},addEventListener(event,fn){this.hooks[event]=fn;}});
    return elements.get(id);
  };
  const chrome={runtime:{sendMessage:async()=>({ready:permitted&&workerAvailable})},permissions:{contains:async()=>permitted,request:async()=>{permitted=granted;return granted;}},storage:{local:{get:async key=>({[key]:structuredClone(state[key])}),set:async value=>Object.assign(state,structuredClone(value))}}};
  const source=fs.readFileSync(__dirname+'/popup.js','utf8').replace(/import[\s\S]*?from "\.\/capture-(?:settings|diagnostics)\.js";/g,'');
  vm.runInNewContext(source,{...settings,...diagnostics,chrome,document:{getElementById:element},URL});
  await new Promise(resolve=>setImmediate(resolve));
  return {state,element};
}

test('popup enables all-site capture and exposes a working off switch',async()=>{
  const f=await popup(true);assert.equal(f.element('enable-capture').disabled,false);
  await f.element('enable-capture').hooks.click();
  assert.equal(f.state.captureConfig.scope,'all');assert.equal(f.state.captureConfig.enabled,true);
  assert.match(f.element('rules').textContent,/On: downloads over 1 MB/);assert.equal(f.element('enable-capture').disabled,true);
  await f.element('disable').hooks.click();assert.equal(f.state.captureConfig.enabled,false);
  assert.equal(f.element('enable-capture').disabled,false);assert.equal(f.element('disable').disabled,true);
});

test('popup displays denied permission without claiming capture is enabled',async()=>{
  const f=await popup(false);await f.element('enable-capture').hooks.click();
  assert.equal(f.state.captureConfig.enabled,false);assert.match(f.element('status').textContent,/permission was not granted/);
  assert.equal(f.element('enable-capture').disabled,false);
});

test('popup does not display On when permission is saved but the worker is unavailable',async()=>{
  const f=await popup(true,false);await f.element('enable-capture').hooks.click();
  assert.equal(f.state.captureConfig.enabled,true);assert.equal(f.element('enable-capture').disabled,false);
  assert.match(f.element('rules').textContent,/worker is not ready/);
  assert.match(f.element('capture-detail').textContent,/worker is not running/);
  assert.doesNotMatch(f.element('rules').textContent,/^On:/);
});
