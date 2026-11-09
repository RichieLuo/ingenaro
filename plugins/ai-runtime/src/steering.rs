//! One in-flight follow-up, acknowledged by the existing CLI process.
use super::*;
use serde_json::json;
use std::sync::{
    Mutex,
    mpsc::{self, Receiver, SyncSender},
};

pub(super) const REQUEST_ID: u64 = 91;
#[derive(Default)]
pub(super) struct Steering {
    pub target: Option<(SyncSender<Value>, EngineKind, String, String)>,
    pending: Option<SyncSender<Result<(), String>>>,
    pub message_id: Option<String>,
}
impl Steering {
    pub fn close_target(&mut self) -> bool {
        self.target = None;
        self.pending.is_some()
    }

    pub fn finish(&mut self, result: Result<(), String>) {
        if let Some(sender) = self.pending.take() {
            let _ = sender.try_send(result);
        }
    }
}

pub(super) fn send(
    state: &Mutex<Steering>,
    text: String,
    images: &[ImageAttachment],
) -> Result<Receiver<Result<(), String>>, String> {
    if text.is_empty() || text.len() > 2 * 1024 * 1024 {
        return Err("引导内容为空或超过限制".into());
    }
    validate_image_attachments(images).map_err(|e| e.to_string())?;
    let mut state = state.lock().map_err(|_| "引导通道不可用")?;
    if state.pending.is_some() {
        return Err("上一条引导仍在等待引擎确认".into());
    }
    let (writer, kind, thread, turn) = state
        .target
        .as_ref()
        .ok_or("引擎尚未进入可引导状态，请稍后重试")?;
    let message_id = uuid::Uuid::new_v4().to_string();
    let frame = match kind {
        EngineKind::Codex => {
            let mut input = vec![json!({"type":"text","text":text,"text_elements":[]})];
            input.extend(
                images
                    .iter()
                    .map(|image| json!({"type":"image","url":image.data_url()})),
            );
            json!({"id":REQUEST_ID,"method":"turn/steer","params":{"threadId":thread,"expectedTurnId":turn,"input":input}})
        }
        EngineKind::Claude => {
            let mut content: Vec<Value> = images.iter().map(|image| json!({"type":"image","source":{"type":"base64","media_type":image.media_type(),"data":image.base64()}})).collect();
            content.push(json!({"type":"text","text":text}));
            json!({"type":"user","uuid":message_id,"session_id":thread,"parent_tool_use_id":null,"message":{"role":"user","content":content}})
        }
        EngineKind::Antigravity => super::antigravity::prompt_frame(
            REQUEST_ID,
            thread,
            &format!("用户补充需求：\n{text}"),
            images,
        ),
        _ => return Err("此引擎不支持运行中引导".into()),
    };
    writer
        .try_send(frame)
        .map_err(|_| "引导通道忙碌或已关闭，请重试")?;
    let (tx, rx) = mpsc::sync_channel(1);
    state.message_id = Some(message_id);
    state.pending = Some(tx);
    Ok(rx)
}
