//! Same-process follow-ups; a failed/unacknowledged send never clears the draft.
use super::*;

pub(super) struct Pending {
    submission: queue::Submission,
    receipt: mpsc::Receiver<Result<(), String>>,
    queue_id: Option<u64>,
}
impl App {
    pub(super) fn ai_steer(&mut self) {
        if self.ai.steering.is_some() {
            self.ai.state = "上一条引导正在等待确认".into();
            return;
        }
        let submission = match self.ai_capture_submission() {
            Ok(submission) => submission,
            Err(error) => {
                self.ai.state = error;
                return;
            }
        };
        self.ai_steer_submission(submission, None);
    }

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
        self.ai_steer_submission(submission, Some(id));
    }

    fn ai_steer_submission(&mut self, submission: queue::Submission, queue_id: Option<u64>) {
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
                return Err("附带参考文件时请使用排队发送".into());
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
                return Err("引导不能改变当前任务的权限或引用范围，请使用排队发送".into());
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
                queue_id,
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
            Err(_) => Err("引导未获确认，草稿已保留".into()),
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
            self.ai.state = error;
            return;
        }
        // Edits made while waiting for acknowledgement are never erased.
        if let Some(id) = pending.queue_id {
            self.ai.queue.remove(id);
        } else if self.ai.prompt == submission.prompt
            && self
                .ai
                .images
                .iter()
                .map(|i| i.id())
                .eq(submission.images.iter().map(|i| i.id()))
        {
            self.ai.prompt.clear();
            self.ai.images.clear();
        }
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

    #[test]
    fn acknowledgement_preserves_new_edits_and_error_keeps_original_draft() {
        for (accepted, changed) in [(true, false), (true, true), (false, false)] {
            let mut app = fixture(Project::default());
            let config = ai::EngineConfig::new(ai::EngineKind::Codex);
            app.ai.selected_engine = config.id.clone();
            app.ai.settings.engines.push(config);
            app.ai.prompt = "补充要求".into();
            let submission = app.ai_capture_submission().unwrap();
            let (tx, receipt) = mpsc::channel();
            app.ai.steering = Some(Pending {
                submission,
                receipt,
                queue_id: None,
            });
            if changed {
                app.ai.prompt = "等待时的新编辑".into();
            }
            tx.send(if accepted {
                Ok(())
            } else {
                Err("引擎拒绝引导".into())
            })
            .unwrap();
            app.ai_poll_steering();
            assert_eq!(app.ai.messages.len(), usize::from(accepted));
            assert_eq!(
                app.ai.prompt,
                if changed {
                    "等待时的新编辑"
                } else if accepted {
                    ""
                } else {
                    "补充要求"
                }
            );
            assert!(app.ai.steering.is_none());
        }
    }

    #[test]
    fn in_flight_steering_cannot_be_queued_again_or_cross_a_generation() {
        let mut app = fixture(Project::default());
        let config = ai::EngineConfig::new(ai::EngineKind::Codex);
        app.ai.selected_engine = config.id.clone();
        app.ai.settings.engines.push(config);
        app.ai.prompt = "补充要求".into();
        let submission = app.ai_capture_submission().unwrap();
        let (tx, receipt) = mpsc::channel();
        app.ai.steering = Some(Pending {
            submission,
            receipt,
            queue_id: None,
        });
        app.ai_submit();
        assert_eq!(app.ai.queue.len(), 0);
        assert_eq!(app.ai.prompt, "补充要求");
        app.ai.generation = "another-generation".into();
        tx.send(Ok(())).unwrap();
        app.ai_poll_steering();
        assert!(app.ai.messages.is_empty());
        assert_eq!(app.ai.prompt, "补充要求");
    }
    #[test]
    fn queued_steering_removes_only_after_ack_and_never_clears_draft() {
        for accepted in [false, true] {
            let mut app = fixture(Project::default());
            let config = ai::EngineConfig::new(ai::EngineKind::Codex);
            app.ai.selected_engine = config.id.clone();
            app.ai.settings.engines.push(config);
            app.ai.prompt = "排队的消息".into();
            let submission = app.ai_capture_submission().unwrap();
            let id = app.ai.queue.push(submission.clone()).unwrap();
            app.ai.prompt = "另一条草稿".into();
            let (tx, receipt) = mpsc::channel();
            app.ai.steering = Some(Pending {
                submission,
                receipt,
                queue_id: Some(id),
            });
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
        }
    }
}
