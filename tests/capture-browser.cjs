// Isolated Chromium + real native host. Never opens or edits the user's browser profile.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const {chromium}=require(process.env.IDMXL_PLAYWRIGHT || 'playwright');
const repo=path.resolve(__dirname,'..'),root=fs.mkdtempSync(path.join(repo,'.test-output','capture-browser-'));
const host='dev.idmxl.fixture_'+crypto.randomBytes(4).toString('hex');
const extension=path.join(root,'extension'),state=path.join(root,'state'),nativeDir=path.join(root,'native-files'),browserDir=path.join(root,'chrome-files');
for(const d of [extension,state,nativeDir,browserDir])fs.mkdirSync(d,{recursive:true});
for(const file of fs.readdirSync(path.join(repo,'extensions','edge')))if(!file.endsWith('.test.cjs')){
 const bytes=fs.readFileSync(path.join(repo,'extensions','edge',file));fs.writeFileSync(path.join(extension,file),bytes.toString().replaceAll('dev.idmxl.host',host));
}
const manifest=JSON.parse(fs.readFileSync(path.join(extension,'manifest.json')));
manifest.permissions.push('downloads','webRequest');manifest.host_permissions=['http://*/*','https://*/*'];manifest.optional_permissions=[];manifest.optional_host_permissions=[];
fs.writeFileSync(path.join(extension,'manifest.json'),JSON.stringify(manifest));
const payload=crypto.randomBytes(4*1024*1024+17),digest=crypto.createHash('sha256').update(payload).digest('hex');
const requests=[];
const server=http.createServer((req,res)=>{
 if(req.url==='/'){res.writeHead(200,{'Content-Type':'text/html','Set-Cookie':'fixture_session=required; HttpOnly; SameSite=Lax'});res.end('<a href="/strong.zip" id="strong">Strong</a><a href="/session.zip" id="session">Session</a><a href="/unknown.zip" id="unknown">Unknown</a><a href="/slow.zip" id="slow">Slow</a><a href="/cancel.zip" id="cancel">Cancel</a>');return;}
 const name=req.url.split('/').pop();requests.push({name,cookie:!!req.headers.cookie,range:req.headers.range||null});
 if(name==='session.zip'&&!req.headers.cookie?.includes('fixture_session=required')){res.writeHead(403,{'Content-Type':'text/html'});res.end('session required');return;}
 const strong=['strong.zip','slow.zip'].includes(name),range=strong&&/^bytes=(\d+)-(\d+)$/.exec(req.headers.range||'');
 const headers={'Content-Type':'application/zip','Content-Disposition':`attachment; filename="${name}"`};
 if(strong)headers.ETag='"fixture-v1"';
 let data=payload;
 if(range){const start=Number(range[1]),end=Number(range[2]);headers['Content-Range']=`bytes ${start}-${end}/${payload.length}`;data=payload.subarray(start,end+1);}
 if(name!=='unknown.zip')headers['Content-Length']=data.length;
 res.writeHead(range?206:200,headers);
 let offset=0;const timer=setInterval(()=>{if(offset>=data.length){clearInterval(timer);res.end();return;}const end=Math.min(data.length,offset+65536);res.write(data.subarray(offset,end));offset=end;},name==='slow.zip'?100:2);
 res.on('close',()=>clearInterval(timer));
});
function shell(script){return cp.execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{encoding:'utf8',windowsHide:true});}
const keys=['Software\\Google\\Chrome\\NativeMessagingHosts','Software\\Chromium\\NativeMessagingHosts','Software\\Google\\ChromeForTesting\\NativeMessagingHosts'];
let context,worker,extensionId;
const hostConfig=path.join(root,'host-config.json'),choice=path.join(root,'location-choice.json'),hostManifest=path.join(root,'host-manifest.json');
const engine=path.join(repo,'.test-output','idmxl-capture-fixture.exe');
async function wait(fn,timeout=30000){const end=Date.now()+timeout;while(Date.now()<end){if(await fn())return;await new Promise(r=>setTimeout(r,100));}throw Error('Timed out waiting for capture');}
async function command(args){return JSON.parse(cp.execFileSync(engine,['--state-dir',state,...args],{encoding:'utf8',windowsHide:true}));}
(async()=>{
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`http://127.0.0.1:${server.address().port}`;
 const profile=path.join(root,'browser');fs.mkdirSync(path.join(profile,'Default'),{recursive:true});
 fs.writeFileSync(path.join(profile,'Default','Preferences'),JSON.stringify({download:{prompt_for_download:true,default_directory:browserDir}}));
 fs.writeFileSync(choice,JSON.stringify({directory:nativeDir}));
 context=await chromium.launchPersistentContext(profile,{channel:'chromium',executablePath:process.env.IDMXL_CHROMIUM||undefined,headless:true,acceptDownloads:true,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`],env:{...process.env,IDMXL_HOST_CONFIG:hostConfig,IDMXL_TEST_LOCATION_CHOICE:choice,IDMXL_TEST_CAPTURE_ORIGIN:origin}});
 [worker]=context.serviceWorkers();worker ||= await context.waitForEvent('serviceworker');extensionId=worker.url().split('/')[2];
 fs.writeFileSync(hostConfig,JSON.stringify({allowed_origin:`chrome-extension://${extensionId}/`,state_dir:state,download_dir:nativeDir}));
 fs.writeFileSync(hostManifest,JSON.stringify({name:host,description:'IDMXL isolated capture fixture',path:engine,type:'stdio',allowed_origins:[`chrome-extension://${extensionId}/`]}));
 for(const key of keys)shell(`$k='HKCU:\\${key}\\${host}';if(Test-Path -LiteralPath $k){throw 'Fixture key already exists'};New-Item -Path $k -Force | Out-Null;Set-Item -LiteralPath $k -Value '${hostManifest.replaceAll("'","''")}'`);
 const popup=await context.newPage();await popup.goto(`chrome-extension://${extensionId}/popup.html`);
 await popup.getByRole('button',{name:'Enable automatic capture',exact:true}).click();
 console.log('runtime',await popup.locator('#rules').textContent());
 const page=await context.newPage();await page.goto(origin+'/');
 for(const name of ['strong','session','unknown']){
   await page.locator('#'+name).click();
   await wait(async()=>{const rows=await command(['list']);return rows.some(j=>j.name===name+'.zip'&&j.state==='completed');});
   const result=fs.readFileSync(path.join(nativeDir,name+'.zip'));assert.equal(crypto.createHash('sha256').update(result).digest('hex'),digest);
   const browserItems=await worker.evaluate(()=>chrome.downloads.search({}));assert.equal(browserItems.length,0,'Captured download remains in Chrome');
   assert(!context.pages().some(p=>p.url().includes('capture-status.html')),'Capture receipt popup appeared');
   console.log('PASS',name,'native folder, SHA-256, no Chrome item or receipt');
 }
 await page.locator('#slow').click();
 let slow;
 await wait(async()=>{slow=(await command(['list'])).find(j=>j.name==='slow.zip');return slow&&slow.state==='downloading'&&Number(slow.received_bytes)>0;});
 const control=action=>worker.evaluate(async({id,action,host})=>chrome.runtime.sendNativeMessage(host,{version:5,kind:'control',request_id:crypto.randomUUID(),job_id:id,action}),{id:slow.id,action,host});
 await control('pause');await wait(async()=>(await command(['show',slow.id])).state==='paused');
 assert.equal((await worker.evaluate(()=>chrome.downloads.search({}))).length,0);
 await control('start');await wait(async()=>(await command(['show',slow.id])).state==='downloading');
 await control('cancel');await wait(async()=>(await command(['show',slow.id])).state==='canceled');
 console.log('PASS independent native extension Pause/Resume/Cancel, zero Chrome items');
 fs.writeFileSync(choice,JSON.stringify({cancel:true}));await page.locator('#cancel').click();
 await wait(async()=>{const data=await worker.evaluate(()=>chrome.storage.local.get('captureRecords'));return Object.values(data.captureRecords||{}).some(r=>r.phase==='user_canceled');});
 assert(!(await command(['list'])).some(j=>j.name==='cancel.zip'));assert(!fs.existsSync(path.join(nativeDir,'cancel.zip')));
 console.log('PASS canceled location does not download');
 console.log(JSON.stringify({success:true,root,digest,requests},null,2));
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{
 if(worker)try{console.log('capture-state',JSON.stringify(await worker.evaluate(()=>chrome.storage.local.get(['lastStatus','lastCaptureAttempt','captureRuntime','captureRecords']))));}catch{}
 server.closeAllConnections();
 if(context)await Promise.race([context.close(),new Promise(resolve=>setTimeout(resolve,10000))]);
 try{cp.execFileSync(engine,['--state-dir',state,'queue-pause'],{windowsHide:true,timeout:10000});}catch{}
 for(const key of keys)try{shell(`$k='HKCU:\\${key}\\${host}';if(Test-Path -LiteralPath $k){Remove-Item -LiteralPath $k}`);}catch{}
 server.closeAllConnections();server.close();
});
