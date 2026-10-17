const {test}=require('node:test');
const assert=require('node:assert/strict');

test('all-site permission is requested from the enabling click before configuration is saved',async()=>{
  const {enableAutomaticCapture}=await import('./capture-settings.js');const calls=[];
  const api={permissions:{request:async permission=>{calls.push(['permission',permission]);return true;}},storage:{local:{set:async value=>calls.push(['save',value])}}};
  const enabled=enableAutomaticCapture(api);
  assert.equal(calls[0][0],'permission');assert.equal(await enabled,true);
  assert.deepEqual(calls[0][1],{permissions:['downloads','webRequest'],origins:['http://*/*','https://*/*']});
  assert.deepEqual(calls[1],['save',{captureConfig:{enabled:true,scope:'all'}}]);
});

test('denied or failed permissions never enable capture',async()=>{
  const {enableAutomaticCapture}=await import('./capture-settings.js');
  for(const reject of [false,true]) {
    let saved=false;const api={permissions:{request:async()=>{if(reject)throw new Error('denied');return false;}},storage:{local:{set:async()=>saved=true}}};
    if(reject)await assert.rejects(enableAutomaticCapture(api));else assert.equal(await enableAutomaticCapture(api),false);
    assert.equal(saved,false);
  }
});

test('turning off stops future capture without removing browser permissions or existing history',async()=>{
  const {disableAutomaticCapture}=await import('./capture-settings.js');const state={captureRecords:{one:{phase:'committed'}}};
  await disableAutomaticCapture({storage:{local:{set:async value=>Object.assign(state,value)}}});
  assert.deepEqual(state,{captureRecords:{one:{phase:'committed'}},captureConfig:{enabled:false,scope:'all'}});
});
