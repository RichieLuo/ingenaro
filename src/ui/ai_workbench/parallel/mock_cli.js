// One local CLI double per main turn. Child events stay inside its protocol.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const file = name => path.join(__dirname, name);
const send = value => console.log(JSON.stringify(value));
const fail = error => {
  fs.writeFileSync(file('failure.txt'), error.stack || String(error));
  process.exit(1);
};
const onLine = fn => readline.createInterface({input: process.stdin}).on('line', line => {
  try { fn(JSON.parse(line)); } catch (error) { fail(error); }
});
setTimeout(() => fail(new Error('mock native agent deadline')), 45000).unref();

const root = 'one-main-session';
const followUp = '接着在同一个会话里说明刚才的结果';
const makePlan = groups => ({
  shared_brief: '共享导航和配色',
  groups: groups.map((slots, i) => ({
    title: '设计任务 ' + i,
    pages: slots.map(slot => ({new_page: '页面 ' + slot, instruction: '设计页面 ' + slot})),
  })),
});
const pauseUntil = name => new Promise(resolve => {
  const timer = setInterval(() => {
    if (!fs.existsSync(file(name))) return;
    clearInterval(timer);
    resolve();
  }, 5);
});
const markFirstProcess = () => fs.writeFileSync(file('main-launched'), '', {flag:'wx'});
const mcp = (url, headers) => {
  let id = 0;
  return async (name, args) => {
    const response = await fetch(url, {
      method:'POST', headers:{'Content-Type':'application/json',...headers},
      body:JSON.stringify({jsonrpc:'2.0',id:++id,method:'tools/call',params:{name,arguments:args}}),
    });
    assert.equal(response.status, 200);
    const {result} = await response.json();
    assert(result);
    return {ok: !result.isError, value:result.structuredContent ?? result.content[0].text};
  };
};
async function design(text, rootTool, childTool, events) {
  markFirstProcess();
  const groups = text.endsWith('设计九个独立页面')
    ? [[0, 1, 2], [3, 4, 5], [6, 7, 8]] : [[0, 1], [2], [3, 4]];
  const plan = makePlan(groups);
  const single = await rootTool('rpshop_design_pages', makePlan([groups[0]]));
  assert.equal(single.ok, false, 'a one-worker round must be rejected');
  const result = await rootTool('rpshop_design_pages', plan);
  assert(result.ok, JSON.stringify(result.value));
  assert.equal(result.value.subagents_started, 0);
  const assignments = result.value.assignments;
  assert.deepEqual(assignments.map(a => a.page_ids.length), groups.map(group => group.length));
  const childId = i => 'child-' + i;
  events.queue(assignments.map((a, i) => [childId(i), a]));
  if (text.endsWith('验证分工未全部启动')) {
    events.start(childId(0));
    events.finish(childId(0));
    events.reply('只启动了一组');
    return;
  }
  const work = async (assignment, i) => {
    const id = childId(i);
    events.start(id);
    for (const page_id of assignment.page_ids) {
      if (i === 0 && page_id === assignment.page_ids[1]) {
        const bad = await childTool('rpshop_edit_assigned_page', {
          page_id, operations:[
            {op:'create_widget',kind:'Text'},
            {op:'create_widget',page_id:1,kind:'Text'},
          ],
        });
        assert.equal(bad.ok, false);
      }
      const edit = await childTool('rpshop_edit_assigned_page', {
        page_id, operations:[{op:'create_widget',kind:'Text',properties:{text:'页面 ' + (page_id - 2)}}],
      });
      assert(edit.ok, JSON.stringify(edit.value));
      assert.equal(edit.value.results[0].page_id, page_id);
    }
    fs.writeFileSync(file('edited-' + i), '');
    await pauseUntil('release-' + i);
    events.finish(id, i);
  };
  const first = work(assignments[0], 0);
  // Deliberately send the first write while the other native agents are unlaunched.
  await pauseUntil('launch-rest');
  await Promise.all([first, ...assignments.slice(1).map((assignment, i) => work(assignment, i + 1))]);
  const second = await rootTool('rpshop_design_pages', plan);
  assert.equal(second.ok, false, 'a later round in the same turn must be rejected');
  events.reply('本轮完成');
}

if (process.argv.includes('--print')) {
  const args = process.argv;
  assert.equal(args[args.indexOf('--tools')+1], 'Agent');
  assert(!args.includes('--no-session-persistence'));
  const agents=JSON.parse(args[args.indexOf('--agents')+1]);
  assert(agents['rpshop-coordinator'].tools.includes('Agent(rpshop-page)'));
  assert.equal(agents['rpshop-page'].background,true);
  assert(agents['rpshop-page'].tools.includes('mcp__rpshop__rpshop_edit_assigned_page'));
  assert(!agents['rpshop-page'].tools.includes('mcp__rpshop__rpshop_save'));
  const bridge=JSON.parse(args[args.indexOf('--mcp-config')+1]).mcpServers.rpshop;
  const tool=mcp(bridge.url, bridge.headers);
  const emit = (type, fields) => send({type,session_id:root,...fields});
  const reply = text => {
    emit('assistant',{message:{content:[{type:'text',text}]}});
    emit('result',{subtype:'success',is_error:false});
  };
  onLine(msg => {
    if (msg.type==='control_request' && msg.request.subtype==='initialize') {
      emit('system',{subtype:'init'});
      emit('control_response',{response:{request_id:msg.request_id,subtype:'success'}});
    } else if (msg.type==='user') {
      const text=msg.message.content;
      if (text.endsWith(followUp)) {
        assert.equal(args[args.indexOf('--resume')+1], root);
        assert.equal(msg.session_id, root);
        reply('仍在原会话');
        return;
      }
      design(text,tool,tool,{
        queue(agents) {
          const content = agents.map(([id, a]) => ({type:'tool_use',name:'Agent',id,input:{subagent_type:'rpshop-page',description:a.description,prompt:a.prompt,run_in_background:true}}));
          emit('assistant',{message:{content}});
        },
        start(id) {
          emit('system',{subtype:'task_started',task_id:'task-'+id,tool_use_id:id,task_type:'local_agent'});
          emit('user',{message:{content:[{type:'tool_result',tool_use_id:id,content:'Agent launched'}]}});
        },
        finish(id) {
          emit('system',{subtype:'task_notification',task_id:'task-'+id,status:'completed'});
        },
        reply,
      }).catch(fail);
    }
  });
} else {
  assert(process.argv.includes('features.multi_agent=true'));
  let childTool;
  let rpcId=100;
  const pending=new Map();
  const rootTool=(name,args)=>new Promise(resolve=>{
    const id=++rpcId; pending.set(id,resolve);
    send({id,method:'item/tool/call',params:{threadId:root,callId:String(id),tool:name,arguments:args}});
  });
  const emit=(method,params)=>send({method,params:{threadId:root,...params}});
  const reply=text=>{
    emit('item/agentMessage/delta',{delta:text});
    emit('turn/completed',{turn:{id:'root-turn',status:'completed'}});
  };
  onLine(msg=>{
    const ok = result => send({id:msg.id,result});
    switch (msg.method) {
      case 'initialize': ok({}); break;
      case 'initialized':
      case 'turn/interrupt': break;
      case 'mcpServerStatus/list': ok({data:[{name:'inherited'}]}); break;
      case 'thread/resume':
        assert.equal(msg.params.threadId,root);
        // The same app-server thread is resumed for follow-up messages.
      case 'thread/start': {
        assert.equal(msg.params.config['mcp_servers.inherited.enabled'],false);
        const bridge=msg.params.config['mcp_servers.rpshop_live'];
        assert.equal(bridge.default_tools_approval_mode,'approve');
        assert.equal(new URL(bridge.url).hostname,'127.0.0.1');
        childTool=mcp(bridge.url,{Authorization:'Bearer '+process.env[bridge.bearer_token_env_var]});
        ok({thread:{id:root}});
        break;
      }
      case 'turn/start': {
        const text=msg.params.input[0].text;
        ok({turn:{id:'root-turn'}});
        if(text.endsWith(followUp)) {reply('仍在原会话');break;}
        design(text,rootTool,childTool,{
          queue(agents) {
            for (const [id, a] of agents) {
              assert.match(a.task_name,/^rpshop_[a-f0-9]{16}$/);
              if (id === 'child-0') {
                emit('item/started',{item:{type:'subAgentActivity',kind:'started',
                  agentThreadId:id,agentPath:'/root/'+a.task_name}});
              } else {
                emit('item/started',{item:{type:'collabAgentToolCall',tool:'spawnAgent',
                  senderThreadId:root,receiverThreadIds:[id],prompt:a.prompt}});
              }
            }
          },
          start(id) {
            emit('turn/started',{threadId:id,turn:{id:'turn-'+id,status:'inProgress'}});
          },
          finish(id) {
            emit('item/agentMessage/delta',{threadId:id,delta:'子代理内部消息'});
            emit('turn/completed',{threadId:id,turn:{status:'completed'}});
          },
          reply,
        }).catch(fail);
        break;
      }
      default: {
        const r=msg.result;
        pending.get(msg.id)({ok:r.success,value:JSON.parse(r.contentItems[0].text)});
        pending.delete(msg.id);
      }
    }
  });
}
