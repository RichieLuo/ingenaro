use super::*;

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rpshop-shell-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn project(&self, name: &str) -> PathBuf {
        let path = self.0.join(format!("{name}.ingenaro"));
        let mut project = Project::blank();
        project.name = name.into();
        project.save(&path).unwrap();
        path
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn shell_project_loads_without_starting_browser_preview() {
    let files = Files::new();
    let path = files.project("中文 空格工程");
    let ctx = egui::Context::default();
    let mut app = super::super::tests::fixture(Project::blank());
    app.screenshot_mode = false;
    crate::shell_open::attach_paths_for_test(&ctx, vec![path.clone()]);
    app.poll_shell_open(&ctx);
    assert_eq!(app.project.name, "中文 空格工程");
    assert_eq!(app.path, Some(path));
    assert!(app.confirm.is_none());
    assert!(app.pending_open.is_none());
    assert!(app.browser_preview.is_none());
    assert_eq!(app.project, app.saved);
}

#[test]
fn shell_requests_preserve_dirty_project_and_queue_until_confirmation_is_resolved() {
    let files = Files::new();
    let first = files.project("first");
    let second = files.project("second");
    let ctx = egui::Context::default();
    let mut app = super::super::tests::fixture(Project::blank());
    app.screenshot_mode = false;
    app.project.name = "未保存的当前工程".into();
    let original = app.project.clone();
    crate::shell_open::attach_paths_for_test(&ctx, vec![first.clone(), second.clone()]);
    app.poll_shell_open(&ctx);
    assert_eq!(app.confirm.as_deref(), Some("open-path"));
    assert_eq!(app.pending_open, Some(first));
    assert_eq!(app.project, original);
    app.poll_shell_open(&ctx);
    assert_eq!(app.project, original);
    assert_eq!(
        app.pending_open.as_ref().unwrap().file_stem().unwrap(),
        "first"
    );
    app.cancel_replace();
    assert!(app.pending_open.is_none());
    app.poll_shell_open(&ctx);
    assert_eq!(app.pending_open, Some(second.clone()));
    assert_eq!(app.project, original);
    // This is the explicit "discard changes and continue" path used by the modal.
    app.confirm = None;
    app.replace("open-path");
    assert_eq!(app.project.name, "second");
    assert_eq!(app.path, Some(second));
    assert!(app.browser_preview.is_none());
}

#[test]
fn missing_shell_project_keeps_unsaved_input_without_creating_a_file() {
    let files = Files::new();
    let missing = files.0.join("does-not-exist.ingenaro");
    let ctx = egui::Context::default();
    let mut app = super::super::tests::fixture(Project::blank());
    app.screenshot_mode = false;
    app.project.name = "未保存".into();
    let original = app.project.clone();
    crate::shell_open::attach_paths_for_test(&ctx, vec![missing.clone()]);
    app.poll_shell_open(&ctx);
    assert_eq!(app.project, original);
    assert!(app.confirm.is_none());
    assert!(!missing.exists());
}
