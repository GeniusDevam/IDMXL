const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
async function receipt(value) {
  const elements=new Map();let closed=0,changed;
  const element=id=>{if(!elements.has(id))elements.set(id,{textContent:'',hooks:{},addEventListener(event,fn){this.hooks[event]=fn;}});return elements.get(id);};
  const chrome={storage:{local:{get:async()=>({captureConfirmation:value})},onChanged:{addListener:fn=>changed=fn}}};
  vm.runInNewContext(fs.readFileSync(__dirname+'/capture-status.js','utf8'),{chrome,document:{getElementById:element},window:{close:()=>closed++}});
  await new Promise(resolve=>setImmediate(resolve));
  return {element,changed,get closed(){return closed;}};
}
test('confirmation displays a committed receipt as text and can be closed',async()=>{
  const f=await receipt({filename:'<img src=x onerror=alert(1)>.zip',bytes:2097152,job_id:'job'});
  assert.equal(f.element('filename').textContent,'<img src=x onerror=alert(1)>.zip');
  assert.equal(f.element('size').textContent,'2.10 MB');assert.match(f.element('result').textContent,/queued in IDMXL/);
  f.element('close').hooks.click();assert.equal(f.closed,1);
});
test('confirmation does not claim a missing receipt was successfully captured',async()=>{
  const f=await receipt(undefined);assert.match(f.element('result').textContent,/No capture receipt/);
  assert.doesNotMatch(f.element('result').textContent,/was queued/);
});
test('confirmation reports startup only when the native acknowledgement confirms it',async()=>{
  const f=await receipt({job_id:'job',automatic:true,desktop_requested:true});
  assert.match(f.element('startup').textContent,/IDMXL is opening/);
  assert.match(f.element('startup').textContent,/queue started automatically/);
  const old=await receipt({job_id:'job'});
  assert.match(old.element('startup').textContent,/startup was not confirmed/);
  assert.doesNotMatch(old.element('startup').textContent,/IDMXL is opening/);
});
