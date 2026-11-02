const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
test('magnet clicks require capture enabled and a real unmodified user click',async()=>{
 let click,change,prevented=0,sent=0;
 const chrome={storage:{local:{get:async()=>({captureConfig:{enabled:true,scope:'all'}})},onChanged:{addListener:fn=>change=fn}},runtime:{sendMessage:async()=>{sent++;}}};
 vm.runInNewContext(fs.readFileSync(__dirname+'/torrent-links.js','utf8'),{chrome,document:{addEventListener:(_,fn)=>click=fn}});
 await new Promise(resolve=>setImmediate(resolve));
 const event={isTrusted:true,button:0,target:{closest:()=>({href:'magnet:?xt=urn:btih:abc'})},preventDefault:()=>prevented++,stopImmediatePropagation(){}};
 click({...event,isTrusted:false});click({...event,ctrlKey:true});assert.equal(sent,0);
 click(event);assert.equal(sent,1);assert.equal(prevented,1);
 change({captureConfig:{newValue:{enabled:false,scope:'all'}}},'local');click(event);assert.equal(sent,1);
});
