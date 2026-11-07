// Run the real Windows embedded WebView in isolated screenshot mode; never call a model.
// node scripts/ai-webview-qa.mjs target/debug/ingenaro.exe target/ai-webview-qa
import { spawn } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import net from 'node:net';

const executable = path.resolve(process.argv[2]);
const output = path.resolve(process.argv[3]);
await mkdir(output, { recursive: true });
const reserve = net.createServer();
await new Promise(resolve => reserve.listen(0, '127.0.0.1', resolve));
const port = reserve.address().port;
await new Promise(resolve => reserve.close(resolve));
const child = spawn(executable, ['--screenshot', path.join(output, 'editor.png'), '--ai-example', '--qa-interactive'], {
  env: { ...process.env, RPSHOP_QA_CAPTURE_REQUEST: path.join(output, 'capture-request.txt'), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-address=127.0.0.1 --remote-debugging-port=${port}` },
  stdio: ['ignore', 'ignore', 'inherit'],
});
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket;
try {
  let pages;
  for (let n = 0; n < 100; n++) {
    if (child.exitCode !== null) throw new Error('Ingenaro exited before WebView was ready');
    try {
      pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      if (pages.some(page => page.type === 'page')) break;
    } catch {}
    await pause(100);
  }
  const page = pages?.find(page => page.type === 'page');
  if (!page) throw new Error('Embedded WebView CDP unavailable');
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let serial = 0;
  const pending = new Map();
  const contexts = new Map();
  socket.onmessage = event => {
    const frame = JSON.parse(event.data);
    if (frame.id) { const response = pending.get(frame.id); pending.delete(frame.id); frame.error ? response?.reject(new Error(frame.error.message)) : response?.resolve(frame.result); }
    if (frame.method === 'Runtime.executionContextCreated') contexts.set(`${frame.sessionId || ''}:${frame.params.context.id}`, {...frame.params.context, sessionId:frame.sessionId});
    if (frame.method === 'Target.attachedToTarget') {
      const sessionId = frame.params.sessionId;
      call('Runtime.enable', {}, sessionId).catch(() => {});
      call('Runtime.runIfWaitingForDebugger', {}, sessionId).catch(() => {});
    }
  };
  function call(method, params = {}, sessionId) {
    return new Promise((resolve, reject) => {
      const id = ++serial;
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 5000);
      pending.set(id, { resolve: result => { clearTimeout(timer); resolve(result); }, reject: error => { clearTimeout(timer); reject(error); } });
      socket.send(JSON.stringify({ id, method, params, ...(sessionId ? {sessionId} : {}) }));
    });
  }
  await call('Target.setAutoAttach', {autoAttach:true,waitForDebuggerOnStart:false,flatten:true});
  await call('Runtime.enable');
  await call('Page.enable');
  await pause(2500);
  const documents = [];
  for (const context of contexts.values()) {
    try {
      const result = await call('Runtime.evaluate', { contextId: context.id, expression: 'JSON.stringify({title:document.title,body:document.body?.innerText.slice(0,500),width:innerWidth,height:innerHeight})', returnByValue: true }, context.sessionId);
      if (result.result.value) documents.push(JSON.parse(result.result.value));
    } catch {}
  }
  if (!documents.some(document => /漫行|FIELDNOTES/.test(document.body || ''))) {
    const tree = await call('Page.getFrameTree');
    const dom = await call('Runtime.evaluate', { expression: 'document.documentElement.outerHTML.slice(0,1200)', returnByValue: true });
    throw new Error(`Built-in design not rendered in WebView: ${JSON.stringify({documents,tree,dom,pages})}`);
  }
  const screenshot = await call('Page.captureScreenshot', { format: 'png' });
  await writeFile(path.join(output, 'embedded-preview.png'), Buffer.from(screenshot.data, 'base64'));
  await writeFile(path.join(output, 'result.json'), JSON.stringify({ passed: true, documents }, null, 2));
  console.log(JSON.stringify({ passed: true, output, documents }));
} finally {
  socket?.close();
  if (child.exitCode === null) {
    const stop = spawn('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await new Promise(resolve => stop.once('exit', resolve));
  }
}
