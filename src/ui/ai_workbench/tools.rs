use super::*;
use serde_json::{Value, json};

impl App {
    pub(super) fn ai_tool_arguments_private(&self, arguments: &Value) -> bool {
        let mut secrets = self
            .ai
            .protected_literals
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        secrets.extend(self.ai.keys.values().map(String::as_str));
        if let Some(server) = &self.ai.agent_server {
            secrets.push(server.token());
        }
        if let Some(server) = &self.ai.server {
            secrets.push(server.token());
        }
        fn contains(value: &Value, secrets: &[&str]) -> bool {
            match value {
                Value::String(text) => secrets
                    .iter()
                    .any(|secret| !secret.is_empty() && text.contains(secret)),
                Value::Array(values) => values.iter().any(|value| contains(value, secrets)),
                Value::Object(values) => values.iter().any(|(key, value)| {
                    secrets
                        .iter()
                        .any(|secret| !secret.is_empty() && key.contains(secret))
                        || contains(value, secrets)
                }),
                _ => false,
            }
        }
        contains(arguments, &secrets)
    }

    fn ai_tool_contains_secret(&self, call: &automation::ToolCall) -> bool {
        self.ai_tool_arguments_private(&call.arguments)
    }

    pub(super) fn poll_ai_tools(&mut self, ctx: &egui::Context) {
        // Do not interleave a worker transaction with an in-progress GUI command.
        if self.drag.is_some()
            || self.editing.is_some()
            || self.confirm.is_some()
            || self.layer_name_dialog.is_some()
        {
            return;
        }
        if self
            .ai
            .pending_tool
            .as_ref()
            .is_some_and(|r| r.is_cancelled() || r.scope != self.ai_project_key())
            && let Some(request) = self.ai.pending_tool.take()
        {
            let _ = request.reply.send(Err("任务已取消或工程已切换".into()));
        }
        if self
            .ai
            .pending_tool
            .as_ref()
            .is_some_and(|request| self.ai_auto_tool(request))
            && let Some(request) = self.ai.pending_tool.take()
        {
            self.ai_dispatch_tool(ctx, request);
        }
        if self.ai.pending_tool.is_some() {
            return;
        }
        for _ in 0..32 {
            let Some(request) = self
                .ai
                .page_agents
                .take_ready_edit()
                .or_else(|| self.ai.host.try_recv())
            else {
                break;
            };
            if request.is_cancelled() || request.scope != self.ai_project_key() {
                let _ = request
                    .reply
                    .send(Err("任务已取消或工程上下文已改变".into()));
                continue;
            }
            if self.ai_tool_contains_secret(&request.call) {
                let _ = request.reply.send(Err(
                    "工具参数包含受保护凭据，已阻止进入工程与版本记录".into()
                ));
                continue;
            }
            // An early MCP write must not block the remaining native launches or
            // open an approval dialog before the whole plan has been launched.
            if request.call.name == automation::EDIT_PAGE_TOOL
                && self.ai.page_agents.unstarted_count() > 0
                && self.ai_builtin_request(&request)
                && request.call.arguments["page_id"]
                    .as_u64()
                    .is_some_and(|id| self.ai.page_agents.planned_page(id))
            {
                self.ai.page_agents.defer_edit(request);
                continue;
            }
            let read = request.call.name.starts_with("rpshop_read_")
                || matches!(
                    request.call.name.as_str(),
                    "rpshop_capabilities"
                        | "rpshop_list_versions"
                        | "rpshop_show_page"
                        | "rpshop_page_activity"
                );
            // Reads beyond explicitly attached context require disclosure approval.
            // A stricter mode chosen mid-turn never silently broadens access.
            if !self.ai_auto_tool(&request)
                && (!read
                    || (request.call.name.starts_with("rpshop_read_")
                        && (self.ai.context != 0 || self.ai.task_context != 0)))
            {
                self.ai.pending_tool = Some(request);
                break;
            }
            self.ai_dispatch_tool(ctx, request);
        }
    }

    fn ai_dispatch_tool(&mut self, ctx: &egui::Context, request: automation::ToolRequest) {
        let result = self.execute_ai_tool_guarded(ctx, &request.call, Some(&request));
        if let Err(error) = &result {
            self.ai.changes.push(format!("工具未应用：{error}"));
        }
        let _ = request.reply.send(result);
    }

    fn ai_tool_active(&self, request: Option<&automation::ToolRequest>) -> Result<(), String> {
        if request.is_some_and(automation::ToolRequest::is_cancelled) {
            Err("工具请求已取消或超时".into())
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn execute_ai_tool(
        &mut self,
        ctx: &egui::Context,
        call: &automation::ToolCall,
    ) -> Result<Value, String> {
        self.execute_ai_tool_guarded(ctx, call, None)
    }

    fn execute_ai_tool_guarded(
        &mut self,
        ctx: &egui::Context,
        call: &automation::ToolCall,
        request: Option<&automation::ToolRequest>,
    ) -> Result<Value, String> {
        self.ai_tool_active(request)?;
        let is_read = call.name.starts_with("rpshop_read_")
            || matches!(
                call.name.as_str(),
                "rpshop_capabilities"
                    | "rpshop_list_versions"
                    | "rpshop_preview"
                    | "rpshop_show_page"
                    | "rpshop_page_activity"
            );
        if !is_read
            && (self.ai.discuss_only || (self.ai.running.is_some() && self.ai.task_readonly))
        {
            return Err("当前为只读讨论模式，未修改设计。请切换为确认编辑后重新发送需求。".into());
        }
        if self.ai_tool_contains_secret(call) {
            return Err("工具参数包含受保护凭据，已阻止写入".into());
        }
        if call.name == "rpshop_batch_edit"
            && self.ai.page_agents.reserves_page_edits()
            && request.is_some_and(|request| request.same_task(&self.ai.task_cancelled))
        {
            return Err("本轮页面由原生子代理负责。先用 rpshop_design_pages 分工，子代理使用 rpshop_edit_assigned_page 写入".into());
        }
        if let Some(page_id) = call.arguments.get("page_id") {
            let page_id = page_id.as_u64().ok_or("page_id 必须为整数")?;
            if matches!(call.name.as_str(), "rpshop_preview" | "rpshop_export") {
                self.ai_show_page(page_id)?;
            }
        }
        match call.name.as_str() {
            automation::DESIGN_PAGES_TOOL => {
                self.ai_prepare_page_agents(ctx, &call.arguments, request)
            }
            automation::EDIT_PAGE_TOOL => {
                let page_id = call.arguments["page_id"]
                    .as_u64()
                    .ok_or("需要分配的页面 ID")?;
                if !request.is_some_and(|request| self.ai_builtin_request(request))
                    || !self.ai.page_agents.assigned(page_id)
                    || call
                        .arguments
                        .as_object()
                        .is_none_or(|object| object.len() != 2)
                {
                    return Err("此工具只能编辑当前会话分配的页面".into());
                }
                self.ai.page_agents.ensure_launched()?;
                let edit = automation::ToolCall {
                    name: "rpshop_batch_edit".into(),
                    arguments: json!({"operations":call.arguments["operations"]}),
                };
                self.ai_execute_document_tool(ctx, &edit, request, Some(page_id))
            }
            "rpshop_page_activity" => {
                let owner = self.ai_activity_owner(request);
                let result = self
                    .ai
                    .page_activity
                    .apply(&self.project, owner, &call.arguments)?;
                self.ai_sync_activity_view();
                ctx.request_repaint();
                Ok(result)
            }
            "rpshop_show_page" => {
                let id = call.arguments["page_id"]
                    .as_u64()
                    .ok_or("page_id 必须为整数")?;
                self.ai_show_page(id)?;
                ctx.request_repaint();
                Ok(json!({"shown_page_id":id,"document_changed":false}))
            }
            "rpshop_preview" => {
                if self.ai.kind == design::ArtifactKind::Html {
                    if self.ai.html.is_empty() {
                        return Err("还没有 HTML 产物".into());
                    }
                    self.ai.visible = true;
                    self.ai.artifact_open = true;
                    self.ai.source = false;
                } else {
                    self.start_preview(ctx);
                }
                Ok(json!({"preview":"opened","kind":self.ai.kind}))
            }
            "rpshop_save" => {
                if self.save_project(call.arguments["save_as"].as_bool().unwrap_or(false)) {
                    Ok(json!({"saved":true}))
                } else {
                    Err("保存未完成或已取消；查看编辑器状态".into())
                }
            }
            "rpshop_export" => {
                if !matches!(call.arguments["format"].as_str(), Some("html" | "svg")) {
                    return Err("format 只支持 html 或 svg".into());
                }
                if self.ai.kind == design::ArtifactKind::Html {
                    if call.arguments["format"] == "svg" {
                        return Err("HTML 产物不支持原生页面 SVG 导出".into());
                    }
                    self.ai_export_html()?;
                } else {
                    self.export_file_result(call.arguments["format"] == "svg")?;
                }
                Ok(json!({"exported":true}))
            }
            "rpshop_new_project" => {
                // Full replacement must keep the existing save/discard/cancel protection.
                let name = call
                    .arguments
                    .get("name")
                    .map(|v| {
                        v.as_str()
                            .filter(|s| !s.trim().is_empty() && s.len() <= 1024)
                            .ok_or("工程名称无效")
                    })
                    .transpose()?;
                self.ai.pending_new_name = name.map(str::to_owned);
                self.request_replace("new");
                Ok(
                    json!({"replacement_requested":true,"note":"等待现有未保存确认；工程切换后旧会话工具权限失效，请用户发起新一轮。"}),
                )
            }
            "rpshop_publish_html" => {
                let source = call.arguments["source"]
                    .as_str()
                    .ok_or("请提供 source 完整 HTML，而不是文件路径")?;
                design::validate_html(source).map_err(|e| e.to_string())?;
                self.ai_tool_active(request)?;
                self.ai.html = source.to_owned();
                self.ai.kind = design::ArtifactKind::Html;
                self.ai.visible = true;
                self.ai.artifact_open = true;
                self.ai.source = false;
                let title = call.arguments["title"].as_str().unwrap_or("AI 动态 HTML");
                self.ai_record_version(
                    title,
                    design::ArtifactContent::Html {
                        source: source.into(),
                    },
                    false,
                );
                self.ai
                    .changes
                    .push("已发布独立 HTML 产物 · 原生工程未改动".into());
                Ok(json!({"published":true,"kind":"html","bytes":source.len()}))
            }
            "rpshop_list_versions" => {
                let sessions = if let (Some(store), Some(scope), Some(id)) =
                    (&self.ai.store, &self.ai.active_scope, self.ai.session_id)
                {
                    store.session(scope,id).map(|s|s.revisions.iter().map(|r|json!({"version_id":r.id,"title":r.label(),"kind":r.content.kind()})).collect::<Vec<_>>()).unwrap_or_default()
                } else {
                    vec![]
                };
                Ok(json!({"versions":sessions}))
            }
            "rpshop_restore_version" => {
                let id = call.arguments["version_id"]
                    .as_u64()
                    .ok_or("version_id 应为版本整数 ID")?;
                self.ai_restore_guarded(id, request)?;
                Ok(json!({"restored":id}))
            }
            _ => self.ai_execute_document_tool(ctx, call, request, None),
        }
    }

    pub(super) fn ai_execute_document_tool(
        &mut self,
        ctx: &egui::Context,
        call: &automation::ToolCall,
        request: Option<&automation::ToolRequest>,
        worker_page: Option<u64>,
    ) -> Result<Value, String> {
        let mut candidate = self.project.clone();
        let native = self.native.is_some();
        let result = if let Some(page_id) = worker_page {
            automation::execute_page(&mut candidate, native, page_id, call)?
        } else {
            let selection = self.selected.iter().copied().collect::<Vec<_>>();
            automation::execute(&mut candidate, native, self.page, &selection, call)?
        };
        if candidate != self.project {
            if let Some(native) = &self.native {
                native.refresh_previews(&mut candidate, &self.project);
                native
                    .preview_scene(&candidate)
                    .map_err(|e| format!("原生兼容校验未通过，未修改工程：{e}"))?;
            }
            self.ai_tool_active(request)?;
            let before = std::mem::replace(&mut self.project, candidate);
            if let Some(id) = worker_page {
                self.ai.page_agents.record_edit(id);
            }
            self.page = self.page.min(self.project.pages.len() - 1);
            let owner = self.ai_activity_owner(request);
            self.ai_follow_batch(&before, call, &result, owner);
            self.selected.retain(|id| {
                self.project.pages[self.page]
                    .widgets
                    .iter()
                    .any(|w| w.id == *id)
            });
            self.textures.clear();
            let animated_before = before.clone();
            let ongoing_reveal = std::mem::take(&mut self.ai_reveal);
            self.remember(before);
            self.ai_reveal = ongoing_reveal;
            self.ai_reveal.record(
                &animated_before,
                &self.project,
                ctx.input(|input| input.time),
            );
            ctx.request_repaint();
            self.ai
                .changes
                .push("文档事务已应用 · 可撤销 / 重做".into());
            self.status = "AI 已修改当前工程 · Ctrl+Z 可撤销".into();
        }
        Ok(result)
    }

    pub(in crate::ui) fn ai_permissions(&mut self, ctx: &egui::Context) {
        if self.ai.pending_tool.is_none() && self.ai.pending_approval.is_none() {
            return;
        }
        self.ai.embedded.hide();
        let mut tool_answer = None;
        let mut engine_answer = None;
        let mut close = false;
        let response = dialog_style::modal(ctx, "ai-tool-authorization").show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 96.0).clamp(280.0, 540.0));
            close |= dialog_style::header(
                ui,
                dialog_style::Symbol::Shield,
                "确认本次工具操作",
                "AI 希望更新当前设计，修改可以撤销",
            );
            if let Some(request) = &self.ai.pending_tool {
                dialog_style::section(ui, |ui| {
                    let operation = if request.call.name == automation::DESIGN_PAGES_TOOL {
                        "启动并行页面设计"
                    } else {
                        &request.call.name
                    };
                    dialog_style::detail(ui, "工具操作", operation);
                    if let Some(page_id) = request.call.arguments["page_id"].as_u64()
                        && let Some(page) =
                            self.project.pages.iter().find(|page| page.id == page_id)
                    {
                        dialog_style::detail(ui, "目标页面", &page.name);
                    }
                    dialog_style::note(ui, "仅在允许后应用到当前工程；无效参数会被拒绝。");
                });
                let text = if request.call.name == "rpshop_publish_html" {
                    format!(
                        "发布独立 HTML，{} 字节；沙箱预览，不修改原生工程。",
                        request.call.arguments["source"]
                            .as_str()
                            .map(str::len)
                            .unwrap_or(0)
                    )
                } else {
                    serde_json::to_string_pretty(&request.call.arguments).unwrap_or_default()
                };
                ui.collapsing("查看操作参数", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.monospace(text.chars().take(12000).collect::<String>());
                        });
                });
                if self.ai_builtin_request(request)
                    && ui.button("完全授权本会话，后续不再询问…").clicked()
                {
                    self.ai.authorization.confirm_full = true;
                }
                dialog_style::actions(ui, |ui| {
                    if ui
                        .add(dialog_style::primary("允许本次 RPShop 工具操作", PURPLE))
                        .clicked()
                    {
                        tool_answer = Some(true);
                    }
                    if ui.add(dialog_style::secondary("拒绝")).clicked() {
                        tool_answer = Some(false);
                    }
                });
            }
            if let Some((id, tool)) = &self.ai.pending_approval {
                ui.separator();
                ui.label(format!("引擎权限请求：{tool}"));
                ui.horizontal(|ui| {
                    if ui.button("允许本次引擎操作").clicked() {
                        engine_answer = Some((id.clone(), true));
                    }
                    if ui.button("拒绝引擎操作").clicked() {
                        engine_answer = Some((id.clone(), false));
                    }
                });
            }
            ui.add_space(4.0);
            if ui.add(dialog_style::danger("停止整个任务")).clicked() {
                self.ai_stop_design();
            }
        });
        if close || response.should_close() {
            if self.ai.pending_tool.is_some() {
                tool_answer = Some(false);
            }
            if let Some((id, _)) = &self.ai.pending_approval {
                engine_answer = Some((id.clone(), false));
            }
        }
        if let Some(allowed) = tool_answer
            && let Some(request) = self.ai.pending_tool.take()
        {
            if !allowed {
                let _ = request.reply.send(Err("用户拒绝本次操作".into()));
            } else if request.is_cancelled() || request.scope != self.ai_project_key() {
                let _ = request.reply.send(Err("任务或工程上下文已失效".into()));
            } else {
                self.ai_dispatch_tool(ctx, request);
            }
        }
        if let Some((id, allowed)) = engine_answer {
            if let Some(running) = &self.ai.running {
                let _ = running.control.approve(id, allowed);
            }
            self.ai.pending_approval = None;
        }
    }

    pub(super) fn ai_export_html(&mut self) -> Result<(), String> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HTML", &["html"])
            .set_file_name("rpshop-design.html")
            .save_file()
        else {
            return Err("已取消导出".into());
        };
        design::export_html(&path, &self.ai.html).map_err(|e| e.to_string())?;
        self.ai.state = format!("已导出 HTML：{}", path.display());
        Ok(())
    }
}
