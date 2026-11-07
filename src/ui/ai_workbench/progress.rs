//! Live turn state based on engine events and elapsed wall time.
use super::*;

impl App {
    pub(super) fn ai_progress_text(&self, time: f64) -> String {
        let unstarted = self.ai.page_agents.unstarted_count();
        if unstarted > 0 {
            let planned = self.ai.page_agents.planned_count();
            return format!("正在启动子代理 · {}/{planned}", planned - unstarted);
        }
        if self.ai.page_agents.has_active() {
            let (active, total) = self.ai.page_agents.progress();
            return if self.ai.pending_tool.is_some() {
                "子代理设计 · 等待授权".into()
            } else {
                format!("子代理设计 · {active}/{total} 运行中")
            };
        }
        if self.ai_reveal.is_active(time) {
            return format!("正在绘制 · {} 个元件", self.ai_reveal.active_count(time));
        }
        if self.ai.running.is_some() {
            let elapsed = self
                .ai
                .task_started
                .map(|start| start.elapsed().as_secs())
                .unwrap_or(0);
            let phase = if self.ai.pending_tool.is_some() || self.ai.pending_approval.is_some() {
                "等待授权"
            } else if let Some(step) = self.ai.response.steps.last()
                && step.after_text == self.ai.response.text.len()
            {
                match step.activity {
                    design::ChatActivity::Thinking { .. } => "正在思考",
                    design::ChatActivity::Tool { .. } => "正在执行工具",
                }
            } else if !self.ai.response.text.is_empty() {
                "正在回复"
            } else if matches!(self.ai.state.as_str(), "starting" | "正在连接引擎…") {
                "正在连接智能体"
            } else {
                "正在思考 / 执行"
            };
            return format!("{phase} · {}", duration(elapsed));
        }
        self.ai.state.clone()
    }
}
fn duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elapsed_is_actual_time_format_not_percent() {
        assert_eq!(duration(5), "5s");
        assert_eq!(duration(125), "2m 05s");
    }
}
