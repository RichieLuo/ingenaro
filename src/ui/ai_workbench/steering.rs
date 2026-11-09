//! Same-process follow-ups from the queue. Remove an entry only after acknowledgement.
use super::*;

pub(super) struct Pending {
    submission: queue::Submission,
    receipt: mpsc::Receiver<Result<(), String>>,
}
impl App {
    pub(super) fn ai_steer_queued(&mut self, id: u64) {
        let Some(submission) = self
            .ai
            .queue
            .entries()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        if self.ai.steering.is_some() {
            self.ai.state = "上一条引导正在等待确认".into();
            return;
        }
        let result = (|| {
            if submission.scope != self.ai_project_key()
                || submission.generation != self.ai.generation
                || submission.engine.id != self.ai.selected_engine
            {
                return Err("这条消息不属于当前工程或会话，不能引导当前对话".into());
            }
            if !submission.reference_files.is_empty() {
                return Err("附带参考文件的消息将在队列中等待下一轮".into());
            }
            let binding = self
                .ai
                .binding
                .as_ref()
                .ok_or("引擎尚未建立会话，请稍后重试")?;
            if binding.scope != submission.scope || binding.engine_id != submission.engine.id {
                return Err("引导目标与当前工程或引擎不一致".into());
            }
            if submission.read_only != self.ai.task_readonly
                || submission.context_mode != self.ai.task_context
            {
                return Err("引导需沿用当前任务的权限和引用范围，这条消息会留在队列".into());
            }
            let running = self.ai.running.as_ref().ok_or("当前没有运行中的任务")?;
            let text = format!(
                "用户对当前任务的补充要求（继续当前任务，不重启）：{}\n本条补充发送时的引用快照：{}\n写入前请核对实时元件，不覆盖用户的新编辑；原任务权限边界不变。",
                submission.prompt, submission.context
            );
            let receipt = running.control.steer(text, &submission.images)?;
            Ok(Pending {
                submission,
                receipt,
            })
        })();
        match result {
            Ok(pending) => {
                self.ai.steering = Some(pending);
                self.ai.state = "引导已发送，等待引擎确认接收…".into();
            }
            Err(error) => self.ai.state = error,
        }
    }
    pub(super) fn ai_poll_steering(&mut self) {
        let Some(pending) = self.ai.steering.as_ref() else {
            return;
        };
        let result = match pending.receipt.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("引导未获确认".into()),
        };
        let pending = self.ai.steering.take().unwrap();
        let submission = pending.submission;
        if submission.scope != self.ai_project_key()
            || submission.generation != self.ai.generation
            || submission.engine.id != self.ai.selected_engine
        {
            return;
        }
        if let Err(error) = result {
            self.ai.state = format!("{error} · 消息仍在队列中");
            return;
        }
        self.ai.queue.remove(submission.id);
        let mut display = submission.prompt;
        for image in &submission.images {
            display.push_str(&format!("\n[图片：{}]", image.name()));
        }
        self.ai
            .messages
            .push(response::Message::new(design::ChatRole::User, &display));
        self.ai.state = "引擎已接收引导 · 继续当前任务".into();
        if let (Some(store), Some(scope), Some(id)) = (
            &mut self.ai.store,
            &self.ai.active_scope,
            self.ai.session_id,
        ) && let Err(error) = store.append_message(scope, id, design::ChatRole::User, &display)
        {
            self.ai.state = format!("引导已接收，但历史保存失败：{error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixture;

    fn awaiting_queued_steering() -> (App, u64, mpsc::Sender<Result<(), String>>) {
        let mut app = fixture(Project::default());
        let config = ai::EngineConfig::new(ai::EngineKind::Codex);
        app.ai.selected_engine = config.id.clone();
        app.ai.settings.engines.push(config);
        app.ai.prompt = "排队的消息".into();
        let submission = app.ai_capture_submission().unwrap();
        let id = app.ai.queue.push(submission).unwrap();
        let submission = app.ai.queue.entries().next().unwrap().clone();
        let (tx, receipt) = mpsc::channel();
        app.ai.steering = Some(Pending {
            submission,
            receipt,
        });
        (app, id, tx)
    }

    #[test]
    fn queued_steering_removes_only_after_ack_and_never_clears_draft() {
        for accepted in [false, true] {
            let (mut app, _, tx) = awaiting_queued_steering();
            app.ai.prompt = "另一条草稿".into();
            app.ai_poll_queue();
            assert_eq!(app.ai.queue.len(), 1);
            tx.send(if accepted {
                Ok(())
            } else {
                Err("拒绝".into())
            })
            .unwrap();
            app.ai_poll_steering();
            assert_eq!(app.ai.queue.len(), usize::from(!accepted));
            assert_eq!(app.ai.prompt, "另一条草稿");
            assert_eq!(app.ai.messages.len(), usize::from(accepted));
            assert!(app.ai.steering.is_none());
        }
    }

    #[test]
    fn a_new_submission_can_queue_while_steering_is_awaiting_ack() {
        let (mut app, id, tx) = awaiting_queued_steering();
        app.ai.prompt = "等待确认期间的新消息".into();
        app.ai_submit();
        assert!(app.ai.prompt.is_empty());
        assert!(app.ai.running.is_none());
        assert_eq!(app.ai.queue.len(), 2);

        tx.send(Ok(())).unwrap();
        app.ai_poll_steering();
        let queued = app.ai.queue.entries().next().unwrap();
        assert_ne!(queued.id, id);
        assert_eq!(queued.prompt, "等待确认期间的新消息");
        assert_eq!(app.ai.queue.len(), 1);
        assert_eq!(app.ai.messages[0].text, "排队的消息");
    }

    #[test]
    fn acknowledgement_cannot_cross_a_generation() {
        let (mut app, _, tx) = awaiting_queued_steering();
        app.ai.generation = "another-generation".into();
        tx.send(Ok(())).unwrap();
        app.ai_poll_steering();
        assert!(app.ai.messages.is_empty());
        assert_eq!(app.ai.queue.len(), 1);
    }
}
