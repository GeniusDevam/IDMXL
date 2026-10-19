const {test}=require('node:test');
const assert=require('node:assert/strict');
test('popup uses matching bounded native replies and sends only typed controls',async()=>{
  const {requestDownloads}=await import('./downloads-panel.js');const requests=[];
  const api={runtime:{sendNativeMessage:async(host,r)=>{requests.push({host,r});return{version:5,accepted:true,request_id:r.request_id,jobs:[{id:'job',name:'a.zip'}]};}}};
  assert.equal((await requestDownloads(api)).length,1);
  await requestDownloads(api,'control',{job_id:'job',action:'pause'});
  assert.deepEqual(Object.keys(requests[1].r).sort(),['action','job_id','kind','request_id','version']);
  assert.equal(requests[1].r.action,'pause');
  for(const change of [{version:4},{accepted:false},{request_id:'wrong'},{jobs:Array(101).fill({})}]){
    api.runtime.sendNativeMessage=async(_,r)=>({version:5,accepted:true,request_id:r.request_id,jobs:[],...change});
    await assert.rejects(requestDownloads(api));
  }
});
test('popup exposes pause/resume/cancel and explicit retry without security approval',async()=>{
  const {downloadButtons}=await import('./downloads-panel.js');
  assert.deepEqual(downloadButtons({state:'downloading'}).map(b=>b.action),['pause','cancel']);
  assert.deepEqual(downloadButtons({state:'paused',can_resume:true}).map(b=>b.action),['start','cancel']);
  assert.deepEqual(downloadButtons({state:'failed',mode:'ranged'}).map(b=>b.action),['retry','cancel']);
  assert.deepEqual(downloadButtons({state:'attention'}).map(b=>b.action),['cancel']);
  assert.deepEqual(downloadButtons({state:'completed'}),[]);
  assert(!downloadButtons({state:'failed',mode:'browser',can_resume:false}).some(b=>b.action==='start'));
});
