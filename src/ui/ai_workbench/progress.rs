//! Honest task feedback: elapsed wall time and public CLI summaries, never synthetic thought.
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
            } else if !self.ai.streamed.is_empty() {
                "正在回复"
            } else if !self.ai.reasoning.is_empty() {
                "正在思考"
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
