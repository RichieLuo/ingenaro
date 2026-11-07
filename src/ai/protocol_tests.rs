//! Spawned protocol doubles: no provider network, login or downloaded code execution.
use super::*;
use crate::automation::ToolHost;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new(source: &str) -> Option<Self> {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return None;
        }
        let dir = std::env::temp_dir().join(format!("rpshop-ai-protocol-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("mock.js"), source).unwrap();
        Some(Self { dir })
    }
    fn request(&self, kind: EngineKind) -> (StartRequest, ToolHost) {
        let mut config = EngineConfig::new(kind);
        config.executable = Some(self.dir.join("mock.js"));
        config.timeout_seconds = 10;
        let (host, tools) = ToolHost::new();
        (
            StartRequest {
                config,
                key: Some("session-private-key".into()),
                prompt: "mock design".into(),
                context: String::new(),
                images: vec![],
                working_dir: self.dir.clone(),
                scope: "project-1".into(),
                resume: None,
                tools,
                mcp: Some(McpEndpoint {
                    endpoint: "http://127.0.0.1:12345/mcp".into(),
                    token: "bridge-private-token".into(),
                }),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            host,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const CODEX: &str = r#"
const readline = require('readline');
const send = v => console.log(JSON.stringify(v));
readline.createInterface({input:process.stdin}).on('line',line=>{
 const m=JSON.parse(line);
 if(m.method==='initialize') { if(!m.params.capabilities.experimentalApi) process.exit(2); send({id:m.id,result:{userAgent:'mock'}}); }
 if(m.method==='mcpServerStatus/list')send({id:m.id,result:{data:[{name:'inherited-server'}],nextCursor:null}});
 if(m.method==='thread/start'||m.method==='thread/resume') {
  if(m.params.config['mcp_servers.inherited-server.enabled']!==false)process.exit(6);
  if(m.params.sandbox!=='read-only') process.exit(3);
  if(m.method==='thread/resume'&&m.params.excludeTurns!==true)process.exit(15);
  if(m.method==='thread/start'&&!m.params.dynamicTools.every(t=>t.type==='function'&&t.inputSchema))process.exit(4);
  send({id:m.id,result:{thread:{id:m.params.threadId||'mock-session'}}});
 }
 if(m.method==='turn/start') {
  if(m.params.input[0].text.includes('multimodal')) {
   const blocks=m.params.input;
   const expected="\n\nProject context:\nunchanged mock context\n设计页面\n\nUser request:\nmultimodal reference request";
   if(blocks.length!==3||blocks[0].type!=='text'||!blocks[0].text.endsWith(expected)||!blocks[0].text.includes('## Organize the page directory'))process.exit(10);
   blocks.slice(1).forEach((block,i)=>{
    if(block.type!=='image'||Object.hasOwn(block,'path')||!block.url.startsWith('data:image/png;base64,'))process.exit(11);
    const data=block.url.slice('data:image/png;base64,'.length), png=Buffer.from(data,'base64');
    const edge=i===0?1:576;
    if(png.toString('base64')!==data||png.subarray(0,8).toString('hex')!=='89504e470d0a1a0a'||png.readUInt32BE(16)!==edge||png.readUInt32BE(20)!==edge)process.exit(12);
    if(i===1&&line.length<=1024*1024)process.exit(13);
   });
   const item={id:'input-image',type:'userMessage',content:blocks};
   send({id:m.id,result:{turn:{id:'image-turn',items:[item]}}});
   send({method:'turn/started',params:{turn:{id:'image-turn',items:[item]}}});
   send({method:'item/started',params:{item}});
   send({method:'item/completed',params:{item}});
   send({method:'item/agentMessage/delta',params:{delta:'image payload verified'}});
   send({method:'turn/completed',params:{turn:{status:'completed',items:[item]}}});return;
  }
  send({id:m.id,result:{turn:{id:'turn-1'}}});
  if(m.params.input[0].text.includes('public-summary')) {
   const effort=m.params.input[0].text.match(/effort:(\w+)/)?.[1];
   if(m.params.effort!==effort||m.params.summary!=='auto')process.exit(7);
   const base={threadId:m.params.threadId,turnId:'turn-1',itemId:'summary-1',summaryIndex:0};
   const reason=delta=>send({method:'item/reasoning/summaryTextDelta',params:{...base,delta}});
   send({method:'item/reasoning/summaryPartAdded',params:base});
   reason('Check session-');
   send({method:'item/agentMessage/delta',params:{itemId:'answer',delta:'Answer bridge-'}});
   send({method:'item/reasoning/textDelta',params:{...base,delta:'raw-private-reasoning'}});
   reason('private-key and bridge-');
   send({method:'item/agentMessage/delta',params:{itemId:'answer',delta:'private-token'}});
   reason('private-token');
   const completed={method:'item/completed',params:{threadId:m.params.threadId,turnId:'turn-1',item:{id:'summary-1',type:'reasoning',summary:['Check session-private-key and bridge-private-token done'],content:['raw-private-reasoning']}}};
   send(completed);send(completed);
   send({method:'item/completed',params:{threadId:m.params.threadId,turnId:'turn-1',item:{id:'summary-fallback',type:'reasoning',summary:['Fallback.']}}});
   send({method:'item/completed',params:{item:{id:'answer',type:'agentMessage',text:'Answer bridge-private-token.'}}});
   send({method:'item/completed',params:{item:{id:'answer',type:'agentMessage',text:'Answer bridge-private-token.'}}});
   send({method:'item/reasoning/summaryTextDelta',params:{...base,itemId:'dangling',delta:' session-'}});
   if(m.params.input[0].text.includes('summary-error'))send({method:'error',params:{message:'private-provider-error'}});
   else if(!m.params.input[0].text.includes('summary-wait'))send({method:'turn/completed',params:{turn:{status:'completed'}}});
   return;
  }
  if(m.params.input[0].text.includes('flood')) {
   for(let i=0;i<400;i++)send({method:'item/agentMessage/delta',params:{delta:'x'}});
   send({method:'turn/completed',params:{turn:{status:'completed'}}});return;
  }
  const args=m.params.input[0].text.includes('echo-secret')?{text:'session-private-key'}:{};
  send({id:40,method:'item/tool/call',params:{threadId:m.params.threadId,turnId:'turn-1',callId:'read-1',tool:'rpshop_read_pages',arguments:args}});
 }
 if(m.id===40&&m.result) {
  if(!m.result.contentItems||m.result.contentItems[0].type!=='inputText')process.exit(5);
  send({method:'item/agentMessage/delta',params:{delta:'hello session-'}});
  send({method:'item/agentMessage/delta',params:{delta:'private-key world'}});
  send({method:'turn/completed',params:{turn:{status:'completed'}}});
 }
});
"#;
const CLAUDE: &str = r#"
const readline=require('readline');
const send=v=>console.log(JSON.stringify(v));
const args=process.argv.slice(2);
if(!args.includes('--permission-prompt-tool')||args[args.indexOf('--tools')+1]!=='')process.exit(2);
readline.createInterface({input:process.stdin}).on('line',line=>{
 const m=JSON.parse(line);
 if(m.type==='control_request'&&m.request.subtype==='initialize')send({type:'control_response',response:{subtype:'success',request_id:m.request_id,response:{}}});
 if(m.type==='user') {
  send({type:'system',subtype:'init',session_id:'claude-mock-session'});
  if(Array.isArray(m.message.content)) {
   const blocks=m.message.content;
   const expected="\n\nProject context:\nunchanged mock context\n设计页面\n\nUser request:\nmultimodal reference request";
   const resumed=args.includes('--resume');
   if(blocks.length!==3||blocks[2].type!=='text'||!blocks[2].text.endsWith(expected)||!blocks[2].text.includes('## Organize the page directory')||m.message.role!=='user'||m.parent_tool_use_id!==null)process.exit(10);
   if(resumed&&(args[args.indexOf('--resume')+1]!=='claude-mock-session'||m.session_id!=='claude-mock-session'))process.exit(14);
   blocks.slice(0,2).forEach((block,i)=>{
    if(block.type!=='image'||block.source.type!=='base64'||block.source.media_type!=='image/png')process.exit(11);
    const data=block.source.data, png=Buffer.from(data,'base64'), edge=i===0?1:576;
    if(png.toString('base64')!==data||png.subarray(0,8).toString('hex')!=='89504e470d0a1a0a'||png.readUInt32BE(16)!==edge||png.readUInt32BE(20)!==edge)process.exit(12);
    if(i===1&&line.length<=1024*1024)process.exit(13);
   });
   send(m); // Native CLI user/input echo, not assistant output.
   send({type:'stream_event',event:{type:'content_block_delta',index:0,delta:{type:'text_delta',text:'image payload verified'}}});
   send({type:'result',subtype:'success',is_error:false});return;
  }
  if(m.message.content.includes('public-summary')) {
   const effort=m.message.content.match(/effort:(\w+)/)?.[1];
   if(args[args.indexOf('--effort')+1]!==effort||args[args.indexOf('--thinking-display')+1]!=='summarized')process.exit(7);
   const stream=event=>send({type:'stream_event',event});
   stream({type:'message_start',message:{id:'m1'}});
   stream({type:'content_block_start',index:0,content_block:{type:'thinking',thinking:''}});
   stream({type:'content_block_delta',index:0,delta:{type:'thinking_delta',thinking:'Check session-'}});
   stream({type:'content_block_delta',index:1,delta:{type:'text_delta',text:'Answer bridge-'}});
   stream({type:'content_block_delta',index:0,delta:{type:'signature_delta',signature:'raw-private-reasoning'}});
   stream({type:'content_block_delta',index:0,delta:{type:'thinking_delta',thinking:'private-key and bridge-'}});
   stream({type:'content_block_delta',index:1,delta:{type:'text_delta',text:'private-token'}});
   stream({type:'content_block_delta',index:0,delta:{type:'thinking_delta',thinking:'private-token'}});
   const completed={type:'assistant',message:{id:'m1',content:[{type:'thinking',thinking:'Check session-private-key and bridge-private-token done',signature:'raw-private-reasoning'},{type:'text',text:'Answer bridge-private-token.'},{type:'redacted_thinking',data:'raw-private-reasoning'}]}};
   send(completed);send(completed);
   send({type:'assistant',message:{id:'m2',content:[{type:'thinking',thinking:'Fallback.'}]}});
   stream({type:'message_start',message:{id:'m3'}});
   stream({type:'content_block_delta',index:0,delta:{type:'thinking_delta',thinking:' session-'}});
   if(m.message.content.includes('summary-error'))send({type:'result',subtype:'error_during_execution',is_error:true,error:'private-provider-error'});
   else if(!m.message.content.includes('summary-wait'))send({type:'result',subtype:'success',is_error:false});
   return;
  }
  send({type:'control_request',request_id:'permission-1',request:{subtype:'can_use_tool',tool_name:'mcp__rpshop__rpshop_read_pages',input:{}}});
 }
 if(m.type==='control_response'&&m.response.request_id==='permission-1') {
  send({type:'stream_event',event:{type:'content_block_delta',index:0,delta:{type:'text_delta',text:m.response.response.behavior}}});
  send({type:'result',subtype:'success',is_error:false,session_id:'claude-mock-session'});
 }
});
"#;

fn collect(running: &RunningSession, host: &ToolHost, approve: bool) -> Vec<AiEvent> {
    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(Instant::now() < deadline, "protocol mock did not complete");
        while let Some(request) = host.try_recv() {
            assert_eq!(request.scope, "project-1");
            assert!(!request.is_cancelled());
            request.reply.send(Ok(json!({"pages":[]}))).unwrap();
        }
        match running.events.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => {
                if let AiEvent::ApprovalRequired { id, .. } = &event {
                    running.control.approve(id.clone(), approve).unwrap();
                }
                let complete = matches!(event, AiEvent::Complete { .. });
                events.push(event);
                if complete {
                    return events;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(e) => panic!("event stream disconnected: {e}"),
        }
    }
}
fn noisy_image(edge: u32) -> ImageAttachment {
    let mut seed = 0xcaba_7865u32;
    let rgba: Vec<u8> = (0..edge as usize * edge as usize * 4)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    ImageAttachment::from_rgba("clipboard.png", edge, edge, &rgba).unwrap()
}
#[test]
fn cli_images_use_real_structured_payloads_on_initial_and_resumed_turns() {
    let images = vec![
        ImageAttachment::from_rgba("private-reference.png", 1, 1, &[255, 0, 0, 255]).unwrap(),
        noisy_image(576),
    ];
    assert!(images[1].byte_len() > super::process::MAX_FRAME);
    for (kind, source, session_id) in [
        (EngineKind::Claude, CLAUDE, "claude-mock-session"),
        (EngineKind::Codex, CODEX, "mock-session"),
    ] {
        let Some(f) = Fixture::new(source) else {
            return;
        };
        let (mut request, host) = f.request(kind);
        let config = request.config.clone();
        request.prompt = "multimodal reference request".into();
        request.context = "unchanged mock context\n设计页面".into();
        request.images = images.clone();
        let running = AiRuntime::start(request).unwrap();
        let initial = collect(&running, &host, true);
        let (mut request, host) = f.request(kind);
        request.config = config.clone();
        request.prompt = "multimodal reference request".into();
        request.context = "unchanged mock context\n设计页面".into();
        request.images = images.clone();
        request.resume = Some(SessionBinding {
            session_id: session_id.into(),
            engine_id: config.id,
            scope: "project-1".into(),
            tool_revision: super::TOOL_PROTOCOL_REVISION,
        });
        let running = AiRuntime::start(request).unwrap();
        let resumed = collect(&running, &host, true);
        for events in [initial, resumed] {
            assert_eq!(
                public_text(&events, false),
                "image payload verified",
                "{kind:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, AiEvent::Error { .. }))
            );
            assert!(matches!(
                events.last(),
                Some(AiEvent::Complete { cancelled: false })
            ));
            let serialized = serde_json::to_string(&events).unwrap();
            for image in &images {
                assert!(!serialized.contains(&image.base64()));
                assert!(!serialized.contains(image.name()));
            }
            assert!(!serialized.contains("data:image/"));
            assert!(!serialized.contains("session-private-key"));
        }
    }
}
#[test]
fn cli_output_eof_reports_error_without_waiting_for_input_writer() {
    let Some(f) = Fixture::new("process.exit(0);") else {
        return;
    };
    let (request, host) = f.request(EngineKind::Codex);
    let running = AiRuntime::start(request).unwrap();
    let started = Instant::now();
    let events = collect(&running, &host, true);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AiEvent::Error { .. }))
    );
    assert!(matches!(
        events.last(),
        Some(AiEvent::Complete { cancelled: false })
    ));
}
#[test]
fn runtime_rejects_image_count_and_aggregate_before_spawning_cli() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (mut request, _) = f.request(EngineKind::Codex);
    request.config.executable = Some(f.dir.join("must-not-spawn.exe"));
    request.images = vec![ImageAttachment::from_rgba("clipboard", 1, 1, &[0; 4]).unwrap(); 5];
    assert!(matches!(AiRuntime::start(request), Err(error) if error.contains("at most 4 images")));
    let (mut request, _) = f.request(EngineKind::Codex);
    request.config.executable = Some(f.dir.join("must-not-spawn.exe"));
    request.images = vec![noisy_image(1024); 3];
    assert!(matches!(AiRuntime::start(request), Err(error) if error.contains("12 MiB total")));
}
#[test]
fn spawned_codex_dynamic_tool_roundtrip_resume_and_secret_redaction() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (request, host) = f.request(EngineKind::Codex);
    let config = request.config.clone();
    let running = AiRuntime::start(request).unwrap();
    let events = collect(&running, &host, true);
    let text = events
        .iter()
        .filter_map(|e| {
            if let AiEvent::TextDelta { text } = e {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<String>();
    assert_eq!(text, "hello [REDACTED] world");
    assert!(events.iter().any(|e| matches!(
        e,
        AiEvent::ToolResult {
            is_error: false,
            ..
        }
    )));
    assert!(running.control.is_finished());
    let (mut request, host) = f.request(EngineKind::Codex);
    request.config = config.clone();
    request.resume = Some(SessionBinding {
        session_id: "mock-session".into(),
        tool_revision: super::TOOL_PROTOCOL_REVISION,
        engine_id: config.id,
        scope: "project-1".into(),
    });
    let running = AiRuntime::start(request).unwrap();
    assert!(
        collect(&running, &host, true)
            .iter()
            .any(|e| matches!(e, AiEvent::Complete { cancelled: false }))
    );
}
#[test]
fn spawned_claude_permissions_can_allow_or_deny_without_job_lock() {
    let Some(f) = Fixture::new(CLAUDE) else {
        return;
    };
    for allowed in [true, false] {
        let (request, host) = f.request(EngineKind::Claude);
        let running = AiRuntime::start(request).unwrap();
        let events = collect(&running, &host, allowed);
        assert!(events.iter().any(
            |e| matches!(e,AiEvent::TextDelta{text} if text==if allowed {"allow"}else{"deny"})
        ));
    }
}
fn public_text(events: &[AiEvent], reasoning: bool) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            AiEvent::TextDelta { text } if !reasoning => Some(text.as_str()),
            AiEvent::ReasoningDelta { text } if reasoning => Some(text.as_str()),
            _ => None,
        })
        .collect()
}
#[test]
fn public_summary_protocols_apply_effort_and_redact_fragmented_credentials_independently() {
    for (kind, source, efforts) in [
        (
            EngineKind::Claude,
            CLAUDE,
            vec![
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ],
        ),
        (
            EngineKind::Codex,
            CODEX,
            vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ],
        ),
    ] {
        let Some(f) = Fixture::new(source) else {
            return;
        };
        for effort in efforts {
            let (mut request, host) = f.request(kind);
            request.config.reasoning_effort = Some(effort);
            request.prompt = format!("public-summary effort:{}", effort.as_str());
            let running = AiRuntime::start(request).unwrap();
            let events = collect(&running, &host, true);
            assert_eq!(
                public_text(&events, true),
                "Check [REDACTED] and [REDACTED] doneFallback. [REDACTED]",
                "{kind:?} {effort:?}"
            );
            assert_eq!(
                public_text(&events, false),
                "Answer [REDACTED].",
                "{kind:?} {effort:?}"
            );
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, AiEvent::Error { .. })),
                "{events:?}"
            );
            let serialized = serde_json::to_string(&events).unwrap();
            for private in [
                "session-private-key",
                "bridge-private-token",
                "raw-private-reasoning",
            ] {
                assert!(!serialized.contains(private), "{kind:?} leaked {private}");
            }
            assert!(matches!(
                events.last(),
                Some(AiEvent::Complete { cancelled: false })
            ));
        }
    }
}
#[test]
fn public_summary_error_flushes_private_prefix_without_provider_error_bodies() {
    for (kind, source) in [(EngineKind::Claude, CLAUDE), (EngineKind::Codex, CODEX)] {
        let Some(f) = Fixture::new(source) else {
            return;
        };
        let (mut request, host) = f.request(kind);
        request.config.reasoning_effort = Some(ReasoningEffort::High);
        request.prompt = "public-summary summary-error effort:high".into();
        let running = AiRuntime::start(request).unwrap();
        let events = collect(&running, &host, true);
        assert_eq!(
            public_text(&events, true),
            "Check [REDACTED] and [REDACTED] doneFallback. [REDACTED]"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AiEvent::Error { .. }))
        );
        let serialized = serde_json::to_string(&events).unwrap();
        assert!(!serialized.contains("private-provider-error"));
        assert!(!serialized.contains("session-private-key"));
        assert!(!serialized.contains("bridge-private-token"));
        assert!(matches!(
            events.last(),
            Some(AiEvent::Complete { cancelled: false })
        ));
    }
}
#[test]
fn public_summary_cancellation_keeps_partial_feedback_and_completes_promptly() {
    for (kind, source) in [(EngineKind::Claude, CLAUDE), (EngineKind::Codex, CODEX)] {
        let Some(f) = Fixture::new(source) else {
            return;
        };
        let (mut request, host) = f.request(kind);
        request.config.reasoning_effort = Some(ReasoningEffort::High);
        request.prompt = "public-summary summary-wait effort:high".into();
        let running = AiRuntime::start(request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            assert!(Instant::now() < deadline);
            let event = match running.events.recv_timeout(Duration::from_millis(100)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => panic!("public summary stream disconnected: {error}"),
            };
            assert!(
                !matches!(&event, AiEvent::Error { .. } | AiEvent::Complete { .. }),
                "{event:?}"
            );
            events.push(event);
            if public_text(&events, true).ends_with("Fallback.") {
                break;
            }
        }
        let start = Instant::now();
        running.control.cancel();
        events.extend(collect(&running, &host, true));
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(
            public_text(&events, true).starts_with("Check [REDACTED] and [REDACTED] doneFallback.")
        );
        assert!(!public_text(&events, true).contains("session-private-key"));
        assert!(matches!(
            events.last(),
            Some(AiEvent::Complete { cancelled: true })
        ));
    }
}
#[test]
fn cancellation_invalidates_inflight_host_request_immediately() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (request, host) = f.request(EngineKind::Codex);
    let running = AiRuntime::start(request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let pending = loop {
        if let Some(request) = host.try_recv() {
            break request;
        }
        assert!(Instant::now() < deadline);
        std::thread::park_timeout(Duration::from_millis(10));
    };
    let start = Instant::now();
    running.control.cancel();
    assert!(pending.is_cancelled());
    let _ = pending.reply.send(Ok(json!({"late":true})));
    loop {
        if let Ok(AiEvent::Complete { cancelled }) =
            running.events.recv_timeout(Duration::from_millis(50))
        {
            assert!(cancelled);
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
#[test]
fn completion_survives_full_event_queue() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (mut request, host) = f.request(EngineKind::Codex);
    request.prompt = "flood".into();
    let running = AiRuntime::start(request).unwrap();
    std::thread::park_timeout(Duration::from_millis(250));
    let events = collect(&running, &host, true);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, AiEvent::TextDelta { .. }))
            .count(),
        400
    );
    assert!(matches!(
        events.last(),
        Some(AiEvent::Complete { cancelled: false })
    ));
}
#[test]
fn provider_echoed_key_in_tool_arguments_never_reaches_host() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (mut request, host) = f.request(EngineKind::Codex);
    request.prompt = "echo-secret".into();
    let running = AiRuntime::start(request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            host.try_recv().is_none(),
            "credential-bearing arguments reached document host"
        );
        assert!(Instant::now() < deadline);
        match running.events.recv_timeout(Duration::from_millis(50)) {
            Ok(AiEvent::Complete { cancelled }) => {
                assert!(!cancelled);
                break;
            }
            Ok(AiEvent::Error { message }) => panic!("{message}"),
            _ => {}
        }
    }
}
#[test]
fn resume_rejects_different_engine_or_project() {
    let Some(f) = Fixture::new(CODEX) else { return };
    let (mut request, _) = f.request(EngineKind::Codex);
    request.resume = Some(SessionBinding {
        session_id: "session".into(),
        tool_revision: super::TOOL_PROTOCOL_REVISION,
        engine_id: request.config.id.clone(),
        scope: "other-project".into(),
    });
    assert!(AiRuntime::start(request).is_err());
}
#[test]
fn api_sse_mock_stream_tool_loop_and_no_key() {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let images =
        vec![ImageAttachment::from_rgba("reference.png", 1, 1, &[0, 255, 0, 255]).unwrap()];
    let expected_url = images[0].data_url();
    let server = std::thread::spawn(move || {
        for turn in 0..2 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            assert!(first.starts_with("POST /v1/chat/completions "));
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                assert!(!line.to_ascii_lowercase().starts_with("authorization:"));
                if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(body["stream"].as_bool().unwrap());
            let input = &body["messages"][0]["content"];
            assert!(input.is_array());
            assert_eq!(input[0]["type"], "image_url");
            assert!(input[0]["image_url"]["url"].as_str() == Some(expected_url.as_str()));
            assert_eq!(input[1]["type"], "text");
            assert!(
                input[1]["text"]
                    .as_str()
                    .unwrap()
                    .ends_with("User request:\ntest")
            );
            let content = if turn == 0 {
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call1\",\"function\":{\"name\":\"rpshop_read_pages\",\"arguments\":\"{\"}}]}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\r\n\r\ndata: [DONE]\r\n\r\n"
            } else {
                assert_eq!(body["messages"][2]["tool_call_id"], "call1");
                "data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
            };
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",content.len(),content).unwrap();
        }
    });
    let (host, tools) = ToolHost::new();
    let mut config = EngineConfig::new(EngineKind::OpenAiCompatible);
    config.model = "mock".into();
    config.api_base = Some(format!("http://{address}/v1/chat/completions"));
    let running = AiRuntime::start(StartRequest {
        config,
        key: None,
        prompt: "test".into(),
        context: String::new(),
        images,
        working_dir: std::env::temp_dir(),
        scope: "project-1".into(),
        resume: None,
        tools,
        mcp: None,
        cancelled: Arc::new(AtomicBool::new(false)),
    })
    .unwrap();
    let events = collect(&running, &host, true);
    server.join().unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e,AiEvent::TextDelta{text}if text=="done"))
    );
    assert!(running.control.cancelled().load(Ordering::Acquire));
}

#[test]
fn same_process_steering_is_acknowledged_with_images_for_both_local_engines() {
    let source = r#"
const readline = require('readline');
const send = x => console.log(JSON.stringify(x));
let initial = false;
readline.createInterface({input:process.stdin}).on('line', line => {
 const m = JSON.parse(line);
 if(m.method==='initialize') send({id:m.id,result:{}});
 if(m.method==='mcpServerStatus/list') send({id:m.id,result:{data:[]}});
 if(m.method==='thread/start') send({id:m.id,result:{thread:{id:'steer-thread'}}});
 if(m.method==='turn/start') send({id:m.id,result:{turn:{id:'active-turn'}}});
 if(m.method==='turn/steer') {
  if(m.params.threadId!=='steer-thread'||m.params.expectedTurnId!=='active-turn'||m.params.input[0].text!=='补充要求'||!m.params.input[1].url.startsWith('data:image/png;base64,')) process.exit(33);
  send({id:m.id,result:{turnId:'active-turn'}});
  send({method:'turn/completed',params:{threadId:'steer-thread',turn:{id:'active-turn',status:'completed'}}});
 }
 if(m.type==='control_request'&&m.request.subtype==='initialize') {
  if(!process.argv.includes('--replay-user-messages')) process.exit(34);
  send({type:'control_response',response:{request_id:m.request_id,subtype:'success'}});
 }
 if(m.type==='user') {
  if(!initial) { initial=true; send({type:'system',subtype:'init',session_id:'steer-thread'}); return; }
  if(m.session_id!=='steer-thread'||m.message.content[0].source.media_type!=='image/png'||m.message.content[1].text!=='补充要求') process.exit(35);
  send(m);
  send({type:'result',subtype:'success',is_error:false});
 }
});
"#;
    for kind in [EngineKind::Claude, EngineKind::Codex] {
        let Some(fixture) = Fixture::new(source) else {
            eprintln!("SKIP: Node unavailable");
            return;
        };
        let (request, host) = fixture.request(kind);
        let running = AiRuntime::start(request).unwrap();
        let image = ImageAttachment::from_rgba("steering-image", 1, 1, &[25, 50, 75, 255]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let receipt = loop {
            assert!(Instant::now() < deadline, "steer target never became ready");
            if let Ok(receipt) = running
                .control
                .steer("补充要求".into(), std::slice::from_ref(&image))
            {
                break receipt;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(
            receipt.recv_timeout(Duration::from_secs(5)).unwrap(),
            Ok(())
        );
        let events = collect(&running, &host, false);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AiEvent::Complete { cancelled: false }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AiEvent::Error { .. }))
        );
    }
}
