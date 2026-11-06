//! Automatic selection references are snapshots, not a dependence on an '@' keystroke.
use super::*;
use serde_json::{Value, json};

#[derive(Clone, Default)]
pub(super) struct SelectionReferences {
    observed: Option<(u64, Vec<u64>)>,
    pub selected: Option<SelectionSnapshot>,
}
#[derive(Clone)]
pub(super) struct SelectionSnapshot {
    pub page_id: u64,
    pub page_name: String,
    pub widgets: Vec<Widget>,
}
impl SelectionReferences {
    pub fn clear(&mut self) {
        self.observed = None;
        self.selected = None;
    }
    pub fn remove(&mut self) {
        self.selected = None;
    }
    pub fn observe(&mut self, project: &Project, page: usize, selection: &BTreeSet<u64>) {
        let page = &project.pages[page];
        let widgets = page
            .widgets
            .iter()
            .filter(|widget| selection.contains(&widget.id))
            .cloned()
            .collect::<Vec<_>>();
        let mut ids = widgets.iter().map(|widget| widget.id).collect::<Vec<_>>();
        ids.sort_unstable();
        let key = (page.id, ids);
        if self.observed.as_ref() != Some(&key) || self.selected.is_some() {
            self.observed = Some(key);
            self.selected = (!widgets.is_empty()).then(|| SelectionSnapshot {
                page_id: page.id,
                page_name: page.name.clone(),
                widgets,
            });
        }
    }
    pub fn value(&self) -> Value {
        self.selected.as_ref().map(|snapshot|json!({"page_id":snapshot.page_id,"page_name":snapshot.page_name,"widget_ids":snapshot.widgets.iter().map(|widget|widget.id).collect::<Vec<_>>(),"widgets":snapshot.widgets})).unwrap_or(Value::Null)
    }
    pub fn label(&self) -> Option<String> {
        self.selected.as_ref().map(|snapshot| {
            if snapshot.widgets.len() == 1 {
                snapshot.widgets[0].name.clone()
            } else {
                format!("已选 {} 个元件", snapshot.widgets.len())
            }
        })
    }
}
impl App {
    pub(in crate::ui) fn ai_observe_selection(&mut self) {
        self.ai
            .references
            .observe(&self.project, self.page, &self.selected);
    }
    pub(super) fn ai_reference_context(&self) -> Value {
        let attached = if self.ai.references.selected.is_some() {
            self.ai.references.value()
        } else {
            match self.ai.context {
                1 => json!({"page":self.project.pages[self.page]}),
                2 => self.ai.references.value(),
                3 => json!({"context":"用户未自动附带工程内容；进一步读取需要授权。"}),
                _ => json!({"project":self.project}),
            }
        };
        json!({"attached_context":attached,"selected_widgets":self.ai.references.value()})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixture;
    #[test]
    fn references_follow_edits_multiselect_page_changes_and_deleted_widgets() {
        let mut app = fixture(Project::default());
        let first = app.project.pages[0].widgets[0].id;
        let second = app.project.pages[0].widgets[1].id;
        app.selected.extend([first, second]);
        app.ai_observe_selection();
        assert_eq!(app.ai.references.label().as_deref(), Some("已选 2 个元件"));
        app.project.pages[0]
            .widgets
            .retain(|widget| widget.id != second);
        app.project.pages[0].widgets[0].name = "编辑后的名字".into();
        app.ai_observe_selection();
        assert_eq!(app.ai.references.label().as_deref(), Some("编辑后的名字"));
        app.project.pages[0]
            .widgets
            .retain(|widget| widget.id != first);
        app.ai_observe_selection();
        assert!(app.ai.references.selected.is_none());
        app.page = 1;
        let id = app.project.pages[1].widgets[0].id;
        app.selected = BTreeSet::from([id]);
        app.ai_observe_selection();
        assert_eq!(
            app.ai.references.selected.as_ref().unwrap().page_id,
            app.project.pages[1].id
        );
        app.selected.clear();
        app.ai_observe_selection();
        assert!(app.ai.references.selected.is_none());
    }
    #[test]
    fn selected_widgets_attach_without_at_and_removal_stays_until_selection_changes() {
        let mut app = fixture(Project::default());
        let id = app.project.pages[0].widgets[0].id;
        app.selected.insert(id);
        app.ai_observe_selection();
        assert_eq!(
            app.ai.references.selected.as_ref().unwrap().widgets[0].id,
            id
        );
        app.ai.references.remove();
        app.ai_observe_selection();
        assert!(app.ai.references.selected.is_none());
        app.selected.clear();
        app.ai_observe_selection();
        app.selected.insert(id);
        app.ai_observe_selection();
        assert!(app.ai.references.selected.is_some());
    }
}
