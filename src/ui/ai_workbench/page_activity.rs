//! Real page-work lifecycle. Completion is explicit, never inferred from idle time.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Owner {
    Builtin,
    External,
}

#[derive(Clone, Debug)]
struct Work {
    label: String,
    implicit: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Completed,
    Incomplete,
}

#[derive(Clone, Debug)]
pub(super) struct ActivityPage {
    pub id: u64,
    pub edits: usize,
    work: BTreeMap<(Owner, String), Work>,
    outcome: Option<(Owner, Outcome)>,
}
impl ActivityPage {
    pub fn is_working(&self) -> bool {
        !self.work.is_empty()
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
            .filter(|_| !self.is_working())
            .map(|(_, outcome)| outcome)
    }

    pub fn label(&self) -> &str {
        match self.outcome() {
            Some(Outcome::Completed) => return "已完成",
            Some(Outcome::Incomplete) => return "未完成",
            None => {}
        }
        self.work
            .values()
            .find(|work| !work.label.is_empty())
            .map(|work| work.label.as_str())
            .unwrap_or("正在设计")
    }

    fn finish_work(&mut self, owner: Owner, work_id: Option<&str>, outcome: Option<Outcome>) {
        let before = self.work.len();
        self.work.retain(|(source, id), work| {
            *source != owner || work_id.is_some_and(|ending| id != ending && !work.implicit)
        });
        if let Some(outcome) = outcome
            && self.work.len() != before
            && (outcome == Outcome::Incomplete || (!self.is_working() && self.outcome.is_none()))
        {
            self.outcome = Some((owner, outcome));
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ActivityView {
    #[default]
    Following,
    Overview,
    Canvas,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PageActivity {
    pages: Vec<ActivityPage>,
    last_focus: Option<u64>,
    view: ActivityView,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivityCall {
    #[serde(default = "main_work")]
    work_id: String,
    updates: Vec<Update>,
}
fn main_work() -> String {
    "main".into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Update {
    page_id: u64,
    status: Status,
    #[serde(default)]
    label: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Working,
    Completed,
}

impl PageActivity {
    pub fn pages(&self) -> &[ActivityPage] {
        &self.pages
    }
    pub fn ids(&self) -> Vec<u64> {
        self.pages.iter().map(|page| page.id).collect()
    }
    pub fn has_work(&self) -> bool {
        self.pages.iter().any(ActivityPage::is_working)
    }
    pub fn follows_page(&self) -> bool {
        self.view == ActivityView::Following
    }
    pub fn overview_visible(&self) -> bool {
        self.view == ActivityView::Overview
    }
    pub fn can_return_to_overview(&self) -> bool {
        self.view == ActivityView::Canvas
    }
    pub fn open_canvas(&mut self) {
        self.view = ActivityView::Canvas;
    }
    pub fn return_to_overview(&mut self) {
        self.view = ActivityView::Overview;
    }
    pub fn last_focus(&self) -> Option<u64> {
        self.last_focus
    }
    pub fn focus(&mut self, id: u64) {
        self.last_focus = Some(id);
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn clear_owner(&mut self, owner: Owner) {
        for page in &mut self.pages {
            page.finish_work(owner, None, None);
            if page.outcome.is_some_and(|(source, _)| source == owner) {
                page.outcome = None;
            }
        }
        self.update_view();
    }
    pub fn finish_owner(&mut self, owner: Owner, succeeded: bool) {
        let outcome = (!self.follows_page()).then_some(if succeeded {
            Outcome::Completed
        } else {
            Outcome::Incomplete
        });
        for page in &mut self.pages {
            page.finish_work(owner, None, outcome);
        }
        self.update_view();
    }
    pub fn interrupt_work(&mut self, owner: Owner, work_id: &str, ids: &[u64]) {
        let outcome = (!self.follows_page()).then_some(Outcome::Incomplete);
        for page in &mut self.pages {
            if ids.contains(&page.id) {
                page.finish_work(owner, Some(work_id), outcome);
            }
        }
        self.update_view();
    }
    fn update_view(&mut self) {
        self.pages
            .retain(|page| page.is_working() || page.outcome.is_some());
        if self.pages.is_empty() {
            self.view = ActivityView::Following;
        } else if self.follows_page() && self.pages.len() > 1 {
            self.view = ActivityView::Overview;
        }
    }
    pub fn prune(&mut self, project: &Project) -> bool {
        let old = self.pages.len();
        self.pages
            .retain(|page| project.pages.iter().any(|existing| existing.id == page.id));
        if self
            .last_focus
            .is_some_and(|id| !project.pages.iter().any(|page| page.id == id))
        {
            self.last_focus = None;
        }
        self.update_view();
        old != self.pages.len()
    }
    fn start_page_work(&mut self, id: u64) -> &mut ActivityPage {
        if let Some(page) = self.pages.iter().position(|page| page.id == id) {
            let page = &mut self.pages[page];
            if !page.is_working() {
                page.outcome = None;
            }
            return page;
        }
        self.pages.push(ActivityPage {
            id,
            edits: 0,
            work: BTreeMap::new(),
            outcome: None,
        });
        self.pages.last_mut().unwrap()
    }
    fn begin_external_work(&mut self, owner: Owner) {
        // External tools have no turn boundary. New work after their last job
        // ends starts a new batch instead of pulling in the previous results.
        if owner == Owner::External
            && !self
                .pages
                .iter()
                .any(|page| page.work.keys().any(|(source, _)| *source == owner))
        {
            self.clear_owner(owner);
        }
    }
    /// Batch writes register each affected page until it is explicitly completed.
    /// If a worker already announced the page, do not create an extra implicit job.
    pub fn edited(&mut self, owner: Owner, ids: &[u64]) {
        if ids.is_empty() {
            return;
        }
        self.begin_external_work(owner);
        for &id in ids {
            let page = self.start_page_work(id);
            page.edits = page.edits.saturating_add(1);
            if !page.work.keys().any(|(source, _)| *source == owner) {
                page.work.insert(
                    (owner, main_work()),
                    Work {
                        label: "正在绘制".into(),
                        implicit: true,
                    },
                );
            }
        }
        self.update_view();
    }
    pub fn apply(
        &mut self,
        project: &Project,
        owner: Owner,
        args: &Value,
    ) -> Result<Value, String> {
        let call: ActivityCall = serde_json::from_value(args.clone()).map_err(|_| {
            "页面任务参数无效：需要 work_id 和 updates[{page_id,status,label}]".to_owned()
        })?;
        if call.work_id.trim().is_empty()
            || call.work_id.len() > 128
            || call.work_id.chars().any(char::is_control)
            || call.updates.is_empty()
            || call.updates.len() > 128
        {
            return Err("work_id 不能为空或超过 128 字节；updates 应包含 1–128 项".into());
        }
        let mut seen = BTreeSet::new();
        for update in &call.updates {
            if !seen.insert(update.page_id)
                || !project.pages.iter().any(|page| page.id == update.page_id)
            {
                return Err("页面任务必须引用存在且不重复的页面 ID".into());
            }
            if update.label.len() > 256 || update.label.chars().any(char::is_control) {
                return Err("任务说明不能包含控制字符或超过 256 字节".into());
            }
        }
        // Validation is complete before changing any active work.
        if call
            .updates
            .iter()
            .any(|update| matches!(update.status, Status::Working))
        {
            self.begin_external_work(owner);
        }
        let outcome = (!self.follows_page()).then_some(Outcome::Completed);
        for update in call.updates {
            match update.status {
                Status::Working => {
                    let page = self.start_page_work(update.page_id);
                    page.work
                        .retain(|(source, _), work| !(*source == owner && work.implicit));
                    page.work.insert(
                        (owner, call.work_id.clone()),
                        Work {
                            label: update.label,
                            implicit: false,
                        },
                    );
                }
                Status::Completed => {
                    if let Some(page) = self.pages.iter_mut().find(|page| page.id == update.page_id)
                    {
                        page.finish_work(owner, Some(&call.work_id), outcome);
                    }
                }
            }
        }
        self.update_view();
        Ok(self.summary())
    }
    pub fn summary(&self) -> Value {
        let active = self.pages.iter().filter(|page| page.is_working()).map(|page| {
            json!({"page_id":page.id,"label":page.label(),"active_work_items":page.work.len(),"applied_batches":page.edits})
        }).collect::<Vec<_>>();
        json!({"active_pages":active,
            "view":if self.overview_visible(){"live_thumbnails"}else{"canvas"},"document_changed":false})
    }
}

impl App {
    pub(super) fn ai_activity_owner(&self, request: Option<&automation::ToolRequest>) -> Owner {
        if request.is_none_or(|request| request.same_task(&self.ai.task_cancelled)) {
            Owner::Builtin
        } else {
            Owner::External
        }
    }
    pub(super) fn ai_sync_activity_view(&mut self) {
        let activity = &mut self.ai.page_activity;
        activity.prune(&self.project);
        if !activity.follows_page() {
            return;
        }
        let target = activity
            .pages()
            .first()
            .map(|page| page.id)
            .or(activity.last_focus());
        if let Some(id) = target {
            let _ = self.ai_show_page(id);
        }
    }
    pub(in crate::ui) fn ai_page_overview_visible(&self) -> bool {
        self.ai.page_activity.overview_visible()
    }
    pub(in crate::ui) fn ai_activity_page_ids(&self) -> Vec<u64> {
        self.ai.page_activity.ids()
    }
    pub(in crate::ui) fn ai_clear_page_activity(&mut self) {
        self.ai.page_activity.clear();
    }
    pub(super) fn ai_activity_demo(&mut self, ctx: &egui::Context) {
        if self.ai.page_agents.has_active() {
            return;
        }
        let updates=self.project.pages.iter().take(6).map(|page|json!({"page_id":page.id,"status":"working","label":"内置预览 · 当前页面实时缩略图"})).collect::<Vec<_>>();
        self.ai.page_activity.clear();
        if updates.len() < 2 {
            self.ai.state = "至少需要两个页面才能预览多页总览".into();
            return;
        }
        let _ = self.ai.page_activity.apply(
            &self.project,
            Owner::Builtin,
            &json!({"work_id":"builtin-preview","updates":updates}),
        );
        self.ai.state = "内置总览预览 · 不调用模型、不修改工程；可用停止退出".into();
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests;
