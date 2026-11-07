//! Page assignments and activity for native children of the current conversation.
use super::*;
use ai::SubagentState;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

const LAUNCH_WAIT: Duration = Duration::from_secs(45);
const LAUNCH_INCOMPLETE: &str = "尚未启动全部分组，请先启动其余分组，再等待子代理结果";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    shared_brief: String,
    groups: Vec<Assignment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assignment {
    title: String,
    pages: Vec<PageTask>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageTask {
    page_id: Option<u64>,
    new_page: Option<String>,
    instruction: String,
}

impl Plan {
    fn read(arguments: &Value, project: &Project) -> Result<Self, String> {
        let plan = Self::deserialize(arguments)
            .map_err(|_| "需要 shared_brief 和 groups[{title,pages}]")?;
        if !(automation::MIN_PAGE_SUBAGENTS..=automation::MAX_PAGE_SUBAGENTS)
            .contains(&plan.groups.len())
        {
            return Err(format!(
                "请把本轮可独立设计的页面一次分成 {}–{} 组；只有一组时由主会话直接完成",
                automation::MIN_PAGE_SUBAGENTS,
                automation::MAX_PAGE_SUBAGENTS,
            ));
        }
        if plan.groups.iter().any(|group| {
            group.title.trim().is_empty()
                || group.title.len() > 256
                || group.title.chars().any(char::is_control)
                || group.pages.is_empty()
        }) || plan
            .groups
            .iter()
            .map(|group| group.pages.len())
            .sum::<usize>()
            > 128
        {
            return Err("每个子代理需要名称和至少一个页面，每组最多涉及 128 页".into());
        }
        let mut seen = BTreeSet::new();
        for task in plan.groups.iter().flat_map(|group| &group.pages) {
            match (task.page_id, task.new_page.as_deref()) {
                (Some(id), None) if seen.insert(id) && project.pages.iter().any(|p| p.id == id) => {
                }
                (None, Some(name))
                    if !name.trim().is_empty()
                        && name.len() <= 1024
                        && !name.chars().any(char::is_control) => {}
                _ => {
                    return Err(
                        "每项分工需要一个现有且不重复的页面 ID，或一个有效的新页面名称".into(),
                    );
                }
            }
            if task.instruction.trim().is_empty() || task.instruction.len() > 8192 {
                return Err("每个页面需要明确的设计要求，最多 8192 字节".into());
            }
        }
        if plan.shared_brief.len() > 8192 {
            return Err("共用设计要求最多 8192 字节".into());
        }
        Ok(plan)
    }
}

#[derive(Default)]
pub(super) struct PageAgents {
    pub enabled: bool,
    targets: Vec<PageAssignment>,
    rows: Vec<AgentRow>,
    waiting_edits: VecDeque<(Instant, automation::ToolRequest)>,
}

struct PageAssignment {
    pages: Vec<u64>,
    task_name: String,
    title: String,
}

struct AgentRow {
    id: String,
    pages: Vec<u64>,
    title: String,
    state: SubagentState,
    edits: usize,
}

fn pending(state: SubagentState) -> bool {
    matches!(state, SubagentState::Starting | SubagentState::Running)
}

impl PageAgents {
    pub fn begin(&mut self, enabled: bool) {
        *self = Self {
            enabled,
            ..Self::default()
        };
    }

    pub fn finish(&mut self) {
        self.enabled = false;
        self.targets.clear();
        for (_, request) in self.waiting_edits.drain(..) {
            let _ = request.reply.send(Err("本轮已结束，页面编辑未应用".into()));
        }
        for row in &mut self.rows {
            if pending(row.state) {
                row.state = SubagentState::Stopped;
            }
        }
    }

    pub fn progress(&self) -> (usize, usize) {
        (
            self.rows
                .iter()
                .filter(|row| row.state == SubagentState::Running)
                .count(),
            self.rows.len(),
        )
    }

    pub fn has_active(&self) -> bool {
        self.rows.iter().any(|row| pending(row.state))
    }

    pub fn planned_count(&self) -> usize {
        self.targets.len()
    }

    pub fn unstarted_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|target| {
                !self
                    .rows
                    .iter()
                    .any(|row| row.pages == target.pages && row.state != SubagentState::Starting)
            })
            .count()
    }

    pub fn unfinished_count(&self) -> usize {
        self.targets
            .iter()
            .filter(|target| {
                !self
                    .rows
                    .iter()
                    .any(|row| row.pages == target.pages && row.state == SubagentState::Completed)
            })
            .count()
    }

    pub fn defer_edit(&mut self, request: automation::ToolRequest) {
        self.waiting_edits
            .push_back((Instant::now() + LAUNCH_WAIT, request));
    }

    pub fn take_ready_edit(&mut self) -> Option<automation::ToolRequest> {
        self.waiting_edits.front()?;
        let all_launched = self.unstarted_count() == 0;
        while let Some((deadline, request)) = self.waiting_edits.front() {
            let error = if request.is_cancelled() {
                "工具请求已取消或超时"
            } else if all_launched {
                return self.waiting_edits.pop_front().map(|(_, request)| request);
            } else if Instant::now() >= *deadline {
                LAUNCH_INCOMPLETE
            } else {
                break;
            };
            if let Some((_, request)) = self.waiting_edits.pop_front() {
                let _ = request.reply.send(Err(error.into()));
            }
        }
        None
    }

    pub fn ensure_launched(&self) -> Result<(), String> {
        if self.unstarted_count() == 0 {
            Ok(())
        } else {
            Err(LAUNCH_INCOMPLETE.into())
        }
    }

    pub fn reserves_page_edits(&self) -> bool {
        self.enabled
            && (self.has_active()
                || self.targets.iter().any(|target| {
                    !self
                        .rows
                        .iter()
                        .any(|row| row.pages == target.pages && !pending(row.state))
                }))
    }

    pub fn assigned(&self, id: u64) -> bool {
        self.planned_page(id)
            && self
                .rows
                .iter()
                .any(|row| row.pages.contains(&id) && pending(row.state))
    }

    pub fn planned_page(&self, id: u64) -> bool {
        self.enabled && self.targets.iter().any(|target| target.pages.contains(&id))
    }

    pub fn record_edit(&mut self, id: u64) {
        if let Some(row) = self
            .rows
            .iter_mut()
            .rev()
            .find(|row| row.pages.contains(&id) && pending(row.state))
        {
            row.edits += 1;
        }
    }
}

impl App {
    pub(super) fn ai_prepare_page_agents(
        &mut self,
        ctx: &egui::Context,
        arguments: &Value,
        request: Option<&automation::ToolRequest>,
    ) -> Result<Value, String> {
        if !self.ai.page_agents.enabled || !request.is_some_and(|r| self.ai_builtin_request(r)) {
            return Err("只有当前原生设计会话可以分配页面子代理".into());
        }
        if self.ai.page_agents.planned_count() > 0 {
            return Err("本轮已有并行分工。请启动现有分工中的全部子代理并汇总结果；遗漏或依赖这些结果的页面由主会话补齐".into());
        }
        let plan = Plan::read(arguments, &self.project)?;
        let operations: Vec<_> = plan
            .groups
            .iter()
            .flat_map(|group| &group.pages)
            .filter_map(|task| {
                task.new_page
                    .as_ref()
                    .map(|name| json!({"op":"create_page","name":name.trim()}))
            })
            .collect();
        let created = if operations.is_empty() {
            Value::Null
        } else {
            let call = automation::ToolCall {
                name: "rpshop_batch_edit".into(),
                arguments: json!({"operations":operations}),
            };
            self.ai_execute_document_tool(ctx, &call, request, None)?
        };
        let mut new_ids = created["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|result| result["page_id"].as_u64());
        let palette = self.ai_palette_context();
        let assignments: Vec<_> = plan.groups.into_iter().map(|group| {
            let mut pages = Vec::new();
            let mut brief = String::new();
            for task in group.pages {
                let id = task.page_id.unwrap_or_else(|| new_ids.next().expect("create_page returns an ID"));
                let page = self.project.pages.iter().find(|page| page.id == id).unwrap();
                pages.push(id);
                brief.push_str(&format!("\n页面 {}（ID {id}）\n{}", page.name, task.instruction));
            }
            let title = format!("page_id={} {}", pages[0], group.title);
            let prompt = format!("{title}\n共用要求：{}\n{palette}{brief}\n使用 rpshop_edit_assigned_page 分批绘制，只编辑本任务列出的页面 ID。使用返回的元件 ID 创建后续交互。完成后把结果汇报给主会话。", plan.shared_brief);
            let task_id = uuid::Uuid::new_v4().simple().to_string();
            let task_name = format!("rpshop_{}", &task_id[..16]);
            self.ai.page_agents.targets.push(PageAssignment {
                pages: pages.clone(),
                task_name: task_name.clone(),
                title: group.title,
            });
            json!({"page_ids":pages, "task_name":task_name, "description":title, "prompt":prompt})
        }).collect();
        self.ai
            .page_activity
            .clear_owner(page_activity::Owner::Builtin);
        Ok(json!({"assignments":assignments, "subagents_started":0}))
    }

    pub(super) fn ai_subagent_event(
        &mut self,
        id: String,
        page_id: Option<u64>,
        task_name: Option<String>,
        title: String,
        state: SubagentState,
    ) {
        if !self.ai.page_agents.enabled || id.is_empty() || id.len() > 96 {
            return;
        }
        let target = self.ai.page_agents.targets.iter().find(|target| {
            if let Some(task_name) = task_name.as_deref() {
                target.task_name == task_name
            } else {
                target.pages.first().copied() == page_id
            }
        });
        let rows = &mut self.ai.page_agents.rows;
        let row = if let Some(index) = rows.iter().position(|row| row.id == id) {
            &mut rows[index]
        } else {
            if rows.len() >= 128 {
                return;
            }
            rows.push(AgentRow {
                id,
                pages: Vec::new(),
                title: String::new(),
                state,
                edits: 0,
            });
            rows.last_mut().unwrap()
        };
        // Terminal notifications can arrive before a duplicated start frame.
        if !pending(row.state) && pending(state) {
            return;
        }
        if row.pages.is_empty()
            && let Some(target) = target
        {
            row.pages.clone_from(&target.pages);
            row.title = target
                .title
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(72)
                .collect();
        }
        if !title.is_empty() {
            row.title = title;
        }
        row.state = state;
        if row.pages.is_empty() || state == SubagentState::Starting {
            return;
        }
        let work_id = format!("subagent:{}", row.id);
        if matches!(state, SubagentState::Failed | SubagentState::Stopped) {
            self.ai.page_activity.interrupt_work(
                page_activity::Owner::Builtin,
                &work_id,
                &row.pages,
            );
        } else {
            let status = if state == SubagentState::Running {
                "working"
            } else {
                "completed"
            };
            let updates: Vec<_> = row
                .pages
                .iter()
                .copied()
                .filter(|id| self.project.pages.iter().any(|page| page.id == *id))
                .map(|page_id| json!({"page_id":page_id,"status":status,"label":"子代理设计中"}))
                .collect();
            if !updates.is_empty() {
                let _ = self.ai.page_activity.apply(
                    &self.project,
                    page_activity::Owner::Builtin,
                    &json!({"work_id":work_id,"updates":updates}),
                );
            }
        }
        self.ai_sync_activity_view();
    }

    pub(super) fn ai_subagent_indicator(&self, ui: &mut egui::Ui) {
        let rows = &self.ai.page_agents.rows;
        if rows.is_empty() && !(self.ai.running.is_some() && self.ai.page_agents.enabled) {
            return;
        }
        let (active, total) = self.ai.page_agents.progress();
        let total = total.max(self.ai.page_agents.planned_count());
        let label = if total == 0 {
            "子代理 0".into()
        } else {
            format!("子代理 {active}/{total}")
        };
        let width = (ui.available_width() - 22.0).clamp(144.0, 268.0);
        let response = ui
            .add(
                egui::Button::new(RichText::new(label).size(11.0).color(PURPLE))
                    .small()
                    .fill(Color32::from_rgb(244, 240, 255))
                    .stroke(Stroke::NONE)
                    .corner_radius(5),
            )
            .on_hover_text("运行中的子代理 / 本轮已分配的子代理");
        let awaiting = self
            .ai
            .pending_tool
            .as_ref()
            .and_then(|request| request.call.arguments["page_id"].as_u64());
        egui::Popup::menu(&response)
            .align(egui::RectAlign::TOP_START)
            .gap(6.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, Color32::from_gray(217)))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 3],
                        blur: 12,
                        spread: 0,
                        color: Color32::from_black_alpha(14),
                    }),
            )
            .show(|ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(
                    RichText::new(format!("页面子代理 · {active} 个运行中"))
                        .size(11.0)
                        .color(MUTED),
                );
                ui.separator();
                if rows.is_empty() {
                    ui.label(
                        RichText::new("本轮还没有启动页面子代理")
                            .size(12.0)
                            .color(MUTED),
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true])
                    .max_height(236.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        for row in rows {
                            let (state, color) = match row.state {
                                SubagentState::Starting => ("正在启动", PURPLE),
                                SubagentState::Running
                                    if awaiting.is_some_and(|id| row.pages.contains(&id)) =>
                                {
                                    ("等待授权", Color32::from_rgb(198, 116, 51))
                                }
                                SubagentState::Running => ("设计中", PURPLE),
                                SubagentState::Completed => {
                                    ("已完成", Color32::from_rgb(21, 128, 61))
                                }
                                SubagentState::Failed => ("失败", Color32::from_rgb(185, 49, 49)),
                                SubagentState::Stopped => ("已停止", MUTED),
                            };
                            ui.horizontal_top(|ui| {
                                let (dot, _) =
                                    ui.allocate_exact_size(vec2(6.0, 15.0), Sense::hover());
                                ui.painter().circle_filled(dot.center(), 3.0, color);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    let title = if row.title.is_empty() {
                                        "页面子代理"
                                    } else {
                                        &row.title
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(title).size(12.0).color(INK),
                                        )
                                        .truncate(),
                                    );
                                    ui.label(
                                        RichText::new(format!("{state} · 已应用 {} 批", row.edits))
                                            .size(11.0)
                                            .color(MUTED),
                                    );
                                });
                            });
                        }
                    });
            });
    }
}
