//! A deliberate, in-memory grant bound to the built-in agent, document and chat generation.
use super::*;

pub(super) use ai::PermissionMode;
#[derive(Clone, PartialEq, Eq)]
struct Grant {
    document: String,
    engine: String,
    generation: String,
}
#[derive(Default)]
pub(super) struct Authorization {
    mode: PermissionMode,
    grant: Option<Grant>,
    pub confirm_full: bool,
    send_after_confirmation: bool,
}
impl Authorization {
    fn bound(ai: &Workbench) -> Grant {
        Grant {
            document: ai.document_id.clone(),
            engine: ai.selected_engine.clone(),
            generation: ai.generation.clone(),
        }
    }
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }
    pub fn reset(&mut self) {
        self.remember_mode(PermissionMode::Ask);
    }
    pub fn remember_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
        self.grant = None;
        self.close_confirmation();
    }
    pub fn confirm_before_send(&mut self) {
        self.confirm_full = true;
        self.send_after_confirmation = true;
    }
    fn close_confirmation(&mut self) {
        self.confirm_full = false;
        self.send_after_confirmation = false;
    }
    pub fn allow_for(&mut self, document: String, engine: String, generation: String) {
        self.mode = PermissionMode::Full;
        self.grant = Some(Grant {
            document,
            engine,
            generation,
        });
        self.close_confirmation();
    }
    pub fn allows(&self, ai: &Workbench) -> bool {
        self.mode == PermissionMode::Full && self.grant.as_ref() == Some(&Self::bound(ai))
    }
}

pub(super) fn known_tool(name: &str) -> Option<&str> {
    let name = name.strip_prefix("mcp__rpshop__").unwrap_or(name);
    ai::is_local_tool(name).then_some(name)
}

impl App {
    #[cfg(test)]
    pub(in crate::ui) fn ai_request_full_permission_test(&mut self) {
        self.ai.authorization.confirm_full = true;
    }

    pub(in crate::ui) fn ai_has_full_permission(&self) -> bool {
        self.ai.authorization.allows(&self.ai)
    }
    pub(super) fn ai_grant_full_permission(&mut self) {
        let send = self.ai.authorization.send_after_confirmation;
        self.ai.authorization.allow_for(
            self.ai.document_id.clone(),
            self.ai.selected_engine.clone(),
            self.ai.generation.clone(),
        );
        self.ai.discuss_only = false;
        self.ai.state =
            "完全授权 · 仅当前工程和会话；画布修改不再逐次询问，可随时撤销或停止".into();
        self.ai_save_composer_preferences();
        if send {
            self.ai_submit();
        }
    }
    pub(super) fn ai_change_permission(&mut self, mode: PermissionMode) {
        if mode == PermissionMode::Full {
            self.ai.authorization.confirm_full = true;
            return;
        }
        self.ai.authorization.remember_mode(mode);
        self.ai.discuss_only = mode == PermissionMode::ReadOnly;
        self.ai_save_composer_preferences();
    }
    pub(super) fn ai_builtin_request(&self, request: &automation::ToolRequest) -> bool {
        self.ai.running.is_some()
            && request.same_task(&self.ai.task_cancelled)
            && request.scope == self.ai_project_key()
            && !request.is_cancelled()
    }
    pub(super) fn ai_auto_tool(&self, request: &automation::ToolRequest) -> bool {
        self.ai_has_full_permission()
            && self.ai_builtin_request(request)
            && known_tool(&request.call.name).is_some()
    }
    pub(super) fn ai_engine_permission(
        &mut self,
        id: String,
        tool: String,
        arguments: serde_json::Value,
    ) {
        let approved = known_tool(&tool).is_some()
            || self.ai.page_agents.enabled
                && matches!(tool.as_str(), "Agent" | "Task")
                && arguments["subagent_type"] == "rpshop-page";
        if !approved {
            if let Some(running) = &self.ai.running {
                let _ = running.control.approve(id, false);
            }
            return;
        }
        // The CLI's registered Ingenaro MCP tools always route through the host gate below.
        // One host decision replaces two duplicate dialogs; no shell/filesystem permission changes.
        let allowed = !self.ai_tool_arguments_private(&arguments);
        if let Some(running) = &self.ai.running {
            let _ = running.control.approve(id, allowed);
        }
    }
    pub(in crate::ui) fn ai_modal_active(&self) -> bool {
        self.ai.authorization.confirm_full
            || self.ai.pending_tool.is_some()
            || self.ai.pending_approval.is_some()
            || self.ai.palette.is_open()
    }

    pub(in crate::ui) fn ai_full_permission_dialog(&mut self, ctx: &egui::Context) {
        if !self.ai.authorization.confirm_full {
            return;
        }
        self.ai.embedded.hide();
        let response=dialog_style::modal(ctx,"ai-full-session-grant").show(ctx,|ui| {
            ui.set_width((ctx.content_rect().width()-96.0).clamp(280.0,488.0));
            if dialog_style::header(ui,dialog_style::Symbol::Shield,"完全授权当前 AI 会话？","让 AI 连续设计，不再逐次打断") {self.ai.authorization.close_confirmation();}
            dialog_style::section(ui,|ui| {
                dialog_style::detail(ui,"允许连续操作当前设计","读取工程、增删元件、调整样式与交互、发布 HTML、恢复设计版本。");
                ui.add_space(6.0);
                dialog_style::detail(ui,"始终保留你的控制权","所有文档编辑仍可撤销；停止任务立即撤销后续工具访问。模型与原生文件校验保持开启。");
            });
            ui.add_space(10.0);
            dialog_style::note(ui,"授权仅限当前内置 Agent、工程与会话，切换后失效。外部 MCP 不继承，不开放任意终端或文件系统操作。");
            dialog_style::note(ui,"保存、导出和替换工程仍遵循文件选择与未保存保护。");
            dialog_style::actions(ui,|ui| {
                let label = if self.ai.authorization.send_after_confirmation {
                    "确认授权并发送"
                } else {
                    "确认完全授权本会话"
                };
                if ui.add(dialog_style::primary(label,PURPLE)).clicked(){self.ai_grant_full_permission();}
                if ui.add(dialog_style::secondary("取消")).clicked(){self.ai.authorization.close_confirmation();}
            });
        });
        if response.should_close() {
            self.ai.authorization.close_confirmation();
        }
    }
}
