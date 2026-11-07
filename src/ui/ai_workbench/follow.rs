//! Follow single-page work and respect manual navigation within multi-page work.
use super::*;
use serde_json::Value;

impl App {
    pub(super) fn ai_show_page(&mut self, page_id: u64) -> Result<(), String> {
        let index = self
            .project
            .pages
            .iter()
            .position(|page| page.id == page_id)
            .ok_or("指定页面不存在")?;
        self.ai.page_activity.focus(page_id);
        if index != self.page {
            self.page = index;
            self.selected.clear();
            self.textures.clear();
            self.fit = true;
            self.view.pending = None;
            self.view.panning = None;
            self.editing = None;
            self.status = format!("切换到页面 · {}", self.project.pages[index].name);
        }
        Ok(())
    }
    pub(super) fn ai_follow_batch(
        &mut self,
        before: &Project,
        call: &automation::ToolCall,
        result: &Value,
        owner: page_activity::Owner,
    ) {
        if call.name != "rpshop_batch_edit" {
            return;
        }
        let old_current = before.pages.get(self.page).map(|page| page.id);
        let operations = call.arguments["operations"].as_array();
        let mut affected = Vec::new();
        for (index, row) in result["results"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let id = row["page_id"].as_u64().or_else(|| {
                operations
                    .and_then(|operations| operations.get(index))
                    .and_then(|operation| operation["page_id"].as_u64())
            });
            if let Some(id) = id.filter(|id| self.project.pages.iter().any(|page| page.id == *id)) {
                affected.retain(|existing| *existing != id);
                affected.push(id);
            }
        }
        // Covers edits whose operation result contains no page_id (events/groups/etc.).
        for page in &self.project.pages {
            if before.pages.iter().find(|old| old.id == page.id) != Some(page)
                && !affected.contains(&page.id)
            {
                affected.push(page.id);
            }
        }
        if affected.is_empty() {
            if let Some(id) =
                old_current.filter(|id| self.project.pages.iter().any(|page| page.id == *id))
            {
                self.ai.page_activity.focus(id);
            }
            return;
        }
        self.ai.page_activity.prune(&self.project);
        self.ai.page_activity.edited(owner, &affected);
        if self.ai.page_activity.follows_page() {
            if let Some(id) = affected.last() {
                let _ = self.ai_show_page(*id);
            }
        } else {
            // Retain the user's focus instead of bouncing between concurrent workers.
            if self.ai.page_activity.last_focus().is_none() {
                self.ai.page_activity.focus(affected[0]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixture;
    use serde_json::json;
    fn two_pages() -> App {
        let mut project = Project::blank();
        let mut page = project.pages[0].clone();
        page.id = project.allocate_id();
        page.name = "第二页面".into();
        project.pages.push(page);
        fixture(project)
    }
    #[test]
    fn batch_edit_other_page_activates_it_and_keeps_single_undo() {
        let mut app = two_pages();
        let page = app.project.pages[1].id;
        let before = app.project.clone();
        app.execute_ai_tool(&egui::Context::default(),&automation::ToolCall{name:"rpshop_batch_edit".into(),arguments:json!({"operations":[{"op":"create_widget","page_id":page,"kind":"Text","properties":{"text":"跟随 AI"}}]})}).unwrap();
        assert_eq!(app.page, 1);
        assert_eq!(app.undo.len(), 1);
        assert!(app.fit);
        app.undo();
        assert_eq!(app.project, before);
    }
    #[test]
    fn explicit_show_page_is_view_only_and_invalid_id_is_atomic() {
        let mut app = two_pages();
        let before = app.project.clone();
        let page = app.project.pages[1].id;
        app.execute_ai_tool(
            &egui::Context::default(),
            &automation::ToolCall {
                name: "rpshop_show_page".into(),
                arguments: json!({"page_id":page}),
            },
        )
        .unwrap();
        assert_eq!(app.page, 1);
        assert_eq!(app.project, before);
        assert!(app.undo.is_empty());
        assert!(app.ai_show_page(99999).is_err());
        assert_eq!(app.page, 1);
    }
    #[test]
    fn created_page_with_assigned_id_is_automatically_shown() {
        let mut app = two_pages();
        app.execute_ai_tool(
            &egui::Context::default(),
            &automation::ToolCall {
                name: "rpshop_batch_edit".into(),
                arguments: json!({"operations":[{"op":"create_page","name":"AI 新页面"}]}),
            },
        )
        .unwrap();
        assert_eq!(app.page, 2);
    }
}
