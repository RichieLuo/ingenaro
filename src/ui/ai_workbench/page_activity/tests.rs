use super::*;
use crate::ui::tests::fixture;

fn project() -> Project {
    let mut project = Project::blank();
    let first = project.pages[0].clone();
    for index in 0..2 {
        let mut page = first.clone();
        page.id = project.allocate_id();
        page.name = format!("页面{}", index + 2);
        project.pages.push(page);
    }
    project
}
#[test]
fn completed_pages_stay_in_place_and_external_work_can_start_a_fresh_batch() {
    let project = project();
    let ids = project.pages.iter().map(|page| page.id).collect::<Vec<_>>();
    let mut state = PageActivity::default();
    let start = ids
        .iter()
        .map(|id| json!({"page_id":id,"status":"working"}))
        .collect::<Vec<_>>();
    state
        .apply(&project, Owner::External, &json!({"updates":start}))
        .unwrap();
    assert_eq!(state.ids(), ids);
    for (remaining, index) in [(2, 1), (1, 0), (0, 2)] {
        let result = state
            .apply(
                &project,
                Owner::External,
                &json!({"updates":[{"page_id":ids[index],"status":"completed"}]}),
            )
            .unwrap();
        assert_eq!(result["active_pages"].as_array().unwrap().len(), remaining);
        assert_eq!(result["view"], "live_thumbnails");
        assert_eq!(state.ids(), ids);
        assert_eq!(state.pages()[index].outcome(), Some(Outcome::Completed));
    }
    assert!(!state.has_work());
    state
        .apply(
            &project,
            Owner::External,
            &json!({"updates":[{"page_id":ids[2],"status":"working"}]}),
        )
        .unwrap();
    assert_eq!(state.ids(), [ids[2]]);
    assert!(!state.overview_visible());
    assert!(state.has_work());
}
#[test]
fn a_page_is_completed_only_after_all_its_workers_finish_successfully() {
    let project = project();
    let [id, other] = [project.pages[0].id, project.pages[1].id];
    let mut state = PageActivity::default();
    state.edited(Owner::Builtin, &[id, other]);
    for work in ["layout", "copy"] {
        state
            .apply(
                &project,
                Owner::Builtin,
                &json!({"work_id":work,"updates":[{"page_id":id,"status":"working"}]}),
            )
            .unwrap();
    }
    state
        .apply(
            &project,
            Owner::Builtin,
            &json!({"work_id":"layout","updates":[{"page_id":id,"status":"completed"}]}),
        )
        .unwrap();
    assert!(state.pages()[0].is_working());
    assert_eq!(state.pages()[0].outcome(), None);
    state.edited(Owner::Builtin, &[id]);
    state
        .apply(
            &project,
            Owner::Builtin,
            &json!({"work_id":"copy","updates":[{"page_id":id,"status":"completed"}]}),
        )
        .unwrap();
    assert_eq!(state.pages()[0].outcome(), Some(Outcome::Completed));

    state.edited(Owner::Builtin, &[id]);
    assert_eq!(state.ids(), [id, other]);
    assert!(state.pages()[0].is_working());
    assert_eq!(state.pages()[0].outcome(), None);
    state.interrupt_work(Owner::Builtin, "main", &[id]);
    state.finish_owner(Owner::Builtin, true);
    assert_eq!(state.pages()[0].outcome(), Some(Outcome::Incomplete));
    assert_eq!(state.pages()[1].outcome(), Some(Outcome::Completed));
    assert!(state.overview_visible());
    assert!(!state.has_work());
}
#[test]
fn invalid_updates_are_atomic_and_finishing_builtin_does_not_clear_external() {
    let project = project();
    let id = project.pages[0].id;
    let mut state = PageActivity::default();
    state.edited(Owner::Builtin, &[id]);
    state.edited(Owner::External, &[id]);
    let before = state.summary();
    assert!(state.apply(&project,Owner::Builtin,&json!({"updates":[{"page_id":id,"status":"completed"},{"page_id":999999,"status":"working"}]})).is_err());
    assert_eq!(state.summary(), before);
    state.clear_owner(Owner::Builtin);
    assert!(state.has_work());
    state.clear_owner(Owner::External);
    assert!(state.pages().is_empty());
}
#[test]
fn tool_lifecycle_keeps_overview_until_explicit_stop_without_editing_document() {
    let mut app = fixture(project());
    let original = app.project.clone();
    let ids = app
        .project
        .pages
        .iter()
        .map(|page| page.id)
        .collect::<Vec<_>>();
    let context = egui::Context::default();
    app.execute_ai_tool(&context,&automation::ToolCall{name:"rpshop_page_activity".into(),arguments:json!({"updates":[{"page_id":ids[0],"status":"working"},{"page_id":ids[1],"status":"working"},{"page_id":ids[2],"status":"working"}]})}).unwrap();
    assert!(app.ai_page_overview_visible());
    assert!(app.undo.is_empty());
    assert_eq!(app.project, original);
    app.execute_ai_tool(&context,&automation::ToolCall{name:"rpshop_page_activity".into(),arguments:json!({"updates":[{"page_id":ids[0],"status":"completed"},{"page_id":ids[1],"status":"completed"}]})}).unwrap();
    assert!(app.ai_page_overview_visible());
    assert_eq!(app.ai_activity_page_ids(), ids);
    assert_eq!(app.page, 0);
    assert_eq!(app.project, original);
    assert!(app.undo.is_empty());
    app.ai_stop_design();
    app.ai_sync_activity_view();
    assert!(app.ai.page_activity.pages().is_empty());
}
#[test]
fn deleted_pages_are_pruned_without_guessing_other_completion() {
    let mut project = project();
    let ids = project.pages.iter().map(|page| page.id).collect::<Vec<_>>();
    let mut state = PageActivity::default();
    state.edited(Owner::Builtin, &ids);
    project.pages.remove(1);
    assert!(state.prune(&project));
    assert_eq!(state.ids(), vec![ids[0], ids[2]]);
}
