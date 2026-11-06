use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::model::Project;

const DOCUMENT: &str = "<!doctype html><html lang=\"zh-CN\"><head><title>设计</title></head><body><h1>你好</h1><script>document.body.dataset.ready='yes'</script></body></html>";
static NEXT_TEST: AtomicU64 = AtomicU64::new(1);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "rpshop-design-test-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scope() -> Scope {
    Scope::new("document:one", "html:provider:model").unwrap()
}
fn html() -> ArtifactContent {
    ArtifactContent::Html {
        source: DOCUMENT.into(),
    }
}

#[test]
fn requires_a_bounded_complete_html_document() {
    for invalid in [
        "",
        " ",
        ">",
        "é>",
        "```html\n<html></html>\n```",
        "<html></html>",
        "<!doctype html><html><body>no head</body></html>",
        "<!doctype html><!-- <html><head></head><body></body></html> -->",
        "<!doctype html><html><head></head><body>open",
        "<!doctype html><html><head></head><body></body></html><script>alert(1)</script>",
        "<!doctype html><html><head></head><body>\0</body></html>",
        "<!doctype html><html><head><script>const fake='<body></body></html>';</script></head>",
        "<!doctype html><html><head></head><p>outside body</p><body></body></html>",
        "<!doctype html><html><head></head>outside body<body></body></html>",
    ] {
        assert!(validate_html(invalid).is_err(), "accepted {invalid}");
    }
    assert!(validate_html(DOCUMENT).is_ok());
    assert!(validate_html(&DOCUMENT.replace("doctype html", "DOCTYPE HTML")).is_ok());
    assert!(validate_html(&format!("\u{feff}  {DOCUMENT} \n")).is_ok());
    assert!(validate_html(&format!("{DOCUMENT}{}", " ".repeat(MAX_HTML_BYTES))).is_err());
}

#[test]
fn boundary_tags_inside_quotes_comments_and_raw_text_do_not_count() {
    assert!(validate_html("<!doctype html><html><head><title>&lt;html&gt;</title></head><body><p title=\"</body></html>\">fine</p><!-- </body> --><textarea></html></textarea><script>const fake=\"</body>\";</script><style>p:before{content:'</html>'}</style></body></html>").is_ok());
    assert!(
        validate_html(
            "<!doctype html><html><head></head><body><script>const fake='</body></html>';"
        )
        .is_err()
    );
    assert!(
        validate_html(
            "<!doctype html><html><head></head><body><p title=\"unterminated></p></body></html>"
        )
        .is_err()
    );
}

#[test]
fn sandbox_markup_cannot_break_out_and_policy_precedes_generated_scripts() {
    let attack = "<!doctype html><html><head></head><body><p>\"'><&</p><script>parent.document.body.innerHTML='attack';</script></body></html>";
    let viewer = preview_document(attack).unwrap();
    assert!(!viewer.contains(attack));
    assert!(viewer.contains("&quot;&#39;&gt;&lt;&amp;"));
    assert!(viewer.contains("sandbox=\"allow-scripts\""));
    assert!(!viewer.contains("allow-same-origin"));
    assert!(!viewer.contains("allow-top-navigation"));
    assert!(!viewer.contains("allow-popups"));
    assert!(viewer.contains("frame-src about:"));
    assert!(viewer.contains("connect-src &#39;none&#39;"));
    assert!(viewer.contains("form-action &#39;none&#39;"));
    assert!(viewer.contains("worker-src &#39;none&#39;"));
    assert!(
        viewer.find("content=&quot;default-src").unwrap()
            < viewer.find("parent.document.body").unwrap()
    );
    assert!(
        viewer.find("RTCPeerConnection").unwrap() < viewer.find("parent.document.body").unwrap()
    );
    assert!(viewer.contains("data-size=\"mobile\""));
}

#[test]
fn persists_chats_sessions_and_immutable_revisions_across_reopen() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "山野页面")
        .unwrap();
    let message = store
        .append_message(&scope, session, ChatRole::User, "把周末交给山野")
        .unwrap();
    let first = store
        .append_revision(&scope, session, "初稿", html(), RevisionOrigin::Generated)
        .unwrap();
    let second = store
        .append_revision(
            &scope,
            session,
            "示例",
            builtin_sample(),
            RevisionOrigin::BuiltinSample,
        )
        .unwrap();
    let mut detached = store.restore(&scope, session, first).unwrap();
    if let ArtifactContent::Html { source } = &mut detached {
        source.push_str("changed");
    }
    assert_eq!(store.restore(&scope, session, first).unwrap(), html());
    drop(store);
    let store = Store::open(&directory.0).unwrap();
    let history = store.session(&scope, session).unwrap();
    assert_eq!(history.messages[0].id, message);
    assert_eq!(history.messages[0].text, "把周末交给山野");
    assert_eq!(history.revisions.len(), 2);
    assert_eq!(
        store.revision(&scope, session, first).unwrap().content,
        html()
    );
    assert_eq!(
        store.revision(&scope, session, second).unwrap().label(),
        "内置示例"
    );
}

#[test]
fn scope_isolation_covers_reads_writes_restores_and_delete() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "one")
        .unwrap();
    let revision = store
        .append_revision(&scope, session, "v1", html(), RevisionOrigin::Generated)
        .unwrap();
    for other in [
        Scope::new("document:two", &scope.engine_key).unwrap(),
        Scope::new(&scope.project_key, "other-engine").unwrap(),
    ] {
        assert!(store.sessions(&other).is_empty());
        assert!(store.session(&other, session).is_err());
        assert!(
            store
                .append_message(&other, session, ChatRole::User, "wrong scope")
                .is_err()
        );
        assert!(
            store
                .append_revision(&other, session, "wrong", html(), RevisionOrigin::Generated)
                .is_err()
        );
        assert!(store.restore(&other, session, revision).is_err());
        assert!(store.delete_session(&other, session).is_err());
    }
    let second = store
        .create_session(&scope, ArtifactKind::Html, "two")
        .unwrap();
    assert!(store.restore(&scope, second, revision).is_err());
    store.delete_session(&scope, session).unwrap();
    assert!(store.session(&scope, session).is_err());
    assert_eq!(store.sessions(&scope).len(), 1);
}

#[test]
fn native_restore_is_detached_validated_and_never_touches_project_files() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let project_path = directory.0.join("user.rpshop");
    fs::write(&project_path, "untouched project").unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Native, "native")
        .unwrap();
    let revision = store
        .append_revision(
            &scope,
            session,
            "v1",
            ArtifactContent::Native(Project::blank()),
            RevisionOrigin::Generated,
        )
        .unwrap();
    let mut restored = store.restore(&scope, session, revision).unwrap();
    if let ArtifactContent::Native(project) = &mut restored {
        project.name = "edited after restore".into();
    }
    assert_eq!(
        store.restore(&scope, session, revision).unwrap(),
        ArtifactContent::Native(Project::blank())
    );
    assert_eq!(
        fs::read_to_string(project_path).unwrap(),
        "untouched project"
    );
    let mut bad = Project::blank();
    bad.pages.clear();
    assert!(
        store
            .append_revision(
                &scope,
                session,
                "bad",
                ArtifactContent::Native(bad),
                RevisionOrigin::Generated
            )
            .is_err()
    );
    assert!(
        store
            .append_revision(
                &scope,
                session,
                "wrong kind",
                html(),
                RevisionOrigin::Generated
            )
            .is_err()
    );
    assert_eq!(store.session(&scope, session).unwrap().revisions.len(), 1);
}

#[test]
fn refuses_corrupt_store_and_preserves_original() {
    let directory = Directory::new();
    let path = directory.0.join("store.json");
    fs::write(&path, "broken json").unwrap();
    assert!(Store::open(&directory.0).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "broken json");
}

#[test]
fn validates_persisted_records_and_next_id_before_loading() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "one")
        .unwrap();
    store
        .append_revision(&scope, session, "v1", html(), RevisionOrigin::Generated)
        .unwrap();
    drop(store);
    let path = directory.0.join("store.json");
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let mut bad = original.clone();
    bad["next_id"] = 1.into();
    fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(Store::open(&directory.0).is_err());
    let mut bad = original;
    bad["sessions"][0]["revisions"][0]["content"]["data"]["source"] = "<p>fragment</p>".into();
    fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(Store::open(&directory.0).is_err());
}

#[test]
fn single_writer_lock_releases_on_drop() {
    let directory = Directory::new();
    let store = Store::open(&directory.0).unwrap();
    assert!(Store::open(&directory.0).is_err());
    drop(store);
    assert!(Store::open(&directory.0).is_ok());
}

#[test]
fn storage_failure_does_not_modify_memory_or_last_committed_snapshot() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "one")
        .unwrap();
    let path = directory.0.join("store.json");
    let before = fs::read(&path).unwrap();
    fs::remove_file(directory.0.join("store.pending")).ok();
    fs::create_dir(directory.0.join("store.pending")).unwrap();
    assert!(
        store
            .append_message(&scope, session, ChatRole::User, "must not commit")
            .is_err()
    );
    assert!(store.session(&scope, session).unwrap().messages.is_empty());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn rejects_limit_overflows_without_pruning_history() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "one")
        .unwrap();
    assert!(
        store
            .append_message(&scope, session, ChatRole::User, &"x".repeat(128 * 1024 + 1))
            .is_err()
    );
    for _ in 0..64 {
        store
            .append_revision(&scope, session, "v", html(), RevisionOrigin::Generated)
            .unwrap();
    }
    assert!(
        store
            .append_revision(&scope, session, "v65", html(), RevisionOrigin::Generated)
            .is_err()
    );
    assert_eq!(store.session(&scope, session).unwrap().revisions.len(), 64);
    assert!(Scope::new("", "engine").is_err());
    assert!(Scope::new("project", "engine\0bad").is_err());
    assert!(super::store::bounded_json(&vec![0u8; 1024], 64).is_err());
}

#[test]
fn interrupted_staging_never_replaces_committed_history() {
    let directory = Directory::new();
    let scope = scope();
    let mut store = Store::open(&directory.0).unwrap();
    let session = store
        .create_session(&scope, ArtifactKind::Html, "committed")
        .unwrap();
    drop(store);
    fs::write(directory.0.join("store.pending"), "incomplete new snapshot").unwrap();
    let mut store = Store::open(&directory.0).unwrap();
    assert_eq!(store.session(&scope, session).unwrap().title, "committed");
    store
        .append_message(&scope, session, ChatRole::User, "recover normally")
        .unwrap();
    drop(store);
    let store = Store::open(&directory.0).unwrap();
    assert_eq!(
        store.session(&scope, session).unwrap().messages[0].text,
        "recover normally"
    );
}

#[test]
fn exports_safe_self_contained_unicode_document_without_overwrite() {
    let directory = Directory::new();
    let path = directory.0.join("山野.HTML");
    export_html(&path, DOCUMENT).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        preview_document(DOCUMENT).unwrap()
    );
    assert!(export_html(&path, demo_html()).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(export_html(&directory.0.join("bad.js"), DOCUMENT).is_err());
    assert!(export_html(&directory.0.join("bad.html"), "<p>fragment</p>").is_err());
    let first = write_preview(DOCUMENT).unwrap();
    let second = write_preview(DOCUMENT).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        preview_document(DOCUMENT).unwrap()
    );
    fs::remove_dir_all(first.parent().unwrap()).unwrap();
    fs::remove_dir_all(second.parent().unwrap()).unwrap();
}

#[test]
fn sample_is_original_offline_complete_and_can_write_browser_fixture() {
    validate_html(demo_html()).unwrap();
    assert!(demo_html().contains("内置示例"));
    assert!(!demo_html().contains("__FIELDNOTES_"));
    assert!(!demo_html().contains("https://"));
    assert!(!demo_html().contains("http://"));
    assert!(demo_html().contains("prefers-reduced-motion"));
    assert_eq!(builtin_sample().kind(), ArtifactKind::Html);
    if let Some(directory) = std::env::var_os("RPSHOP_DESIGN_QA_OUTPUT") {
        fs::create_dir_all(&directory).unwrap();
        let directory = PathBuf::from(directory);
        fs::write(
            directory.join("sample.html"),
            preview_document(demo_html()).unwrap(),
        )
        .unwrap();
    }
}
