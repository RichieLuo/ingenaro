mod native_sessions;

mod persistence;

mod plugin_manager;

mod canvas_editing;

mod editing;

mod behavior;

use super::*;
use eframe::App as _;

mod keyboard;
mod navigation;

mod sidebar_tests;

mod browser_preview;

mod interaction_editor;

#[cfg(target_os = "macos")]
mod titlebar;

#[derive(Default)]
struct MemoryStorage {
    values: HashMap<String, String>,
    writes: usize,
}

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.writes += 1;
        self.values.insert(key.to_owned(), value);
    }

    fn flush(&mut self) {}
}

pub(super) fn fixture(project: Project) -> App {
    App {
        plugins: plugins::Plugins::from_legacy(true),
        ai: ai_workbench::Workbench::new(false, "test-document".into()),
        ai_reveal: Default::default(),
        saved: project.clone(),
        project,
        page: 0,
        selected: BTreeSet::new(),
        path: None,
        undo: Vec::new(),
        redo: Vec::new(),
        clipboard: Default::default(),
        expanded_layer_groups: BTreeSet::new(),
        layer_name_dialog: None,
        page_folder_dialog: None,
        zoom: 0.5,
        view: CanvasView::default(),
        sidebar_layout: sidebar::SidebarLayout::default(),
        fit: false,
        grid: false,
        snap: true,
        tab: 0,
        tool: None,
        drag: None,
        pending_widget: None,
        library_query: String::new(),
        library_category: None,
        browser_preview: None,
        textures: HashMap::new(),
        logo: None,
        status: String::new(),
        last_recovery: Instant::now(),
        screenshot: None,
        // Headless UI checks must never write recovery files into the user's profile.
        screenshot_mode: true,
        history_changed: false,
        editing: None,
        frames: 0,
        confirm: None,
        help: false,
        import_warnings: Vec::new(),
        pending_open: None,
        native: None,
        cached_native_preview_source: None,
    }
}

pub(super) fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1500.0, 950.0))),
        time: Some(f64::from(app.frames) / 60.0),
        events,
        ..Default::default()
    };
    let mut native_frame = eframe::Frame::_new_kittest();
    ctx.run(input, |ctx| app.update(ctx, &mut native_frame))
}

pub(super) fn text_center(output: &egui::FullOutput, text: &str) -> Pos2 {
    fn find(shape: &egui::epaint::Shape, text: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(shape) if shape.galley.text() == text => {
                Some(shape.pos + shape.galley.size() / 2.0)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, text)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, text))
        .unwrap_or_else(|| panic!("UI text {text:?} was not painted"))
}

fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

pub(super) fn click(app: &mut App, ctx: &egui::Context, pos: Pos2) -> egui::FullOutput {
    let _ = frame(
        app,
        ctx,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
    );
    frame(app, ctx, vec![pointer(pos, false)])
}

pub(super) fn right_click(app: &mut App, ctx: &egui::Context, pos: Pos2) -> egui::FullOutput {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(app, ctx, vec![egui::Event::PointerMoved(pos), button(true)]);
    frame(app, ctx, vec![button(false)]);
    frame(app, ctx, vec![])
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn add(project: &mut Project, kind: WidgetKind, x: f32, y: f32) -> u64 {
    let id = project.allocate_id();
    project.pages[0].widgets.push(Widget::new(id, kind, x, y));
    id
}

fn native_tree_upgrade_fixture() -> Option<(PathBuf, Vec<u8>, OpenedDocument)> {
    let source = std::path::Path::new(
        r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings\Training\Quick Win.rp",
    );
    if !source.is_file() {
        return None;
    }
    let bytes = std::fs::read(source).unwrap();
    let path = std::env::temp_dir().join(format!(
        "rpshop-tree-upgrade-{}-{}.rp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let document = load_document(&path).unwrap();
    Some((path, bytes, document))
}

fn old_tree_session(document: &OpenedDocument, path: PathBuf) -> Session {
    let mut previous = document.project.clone();
    previous.page_tree.clear();
    for page in &mut previous.pages {
        page.name = format!("Legacy folder / {}", page.name);
    }
    let mut state = document.native.as_ref().unwrap().session_state();
    state["project"] = serde_json::to_value(&previous).unwrap();
    state["project"]
        .as_object_mut()
        .unwrap()
        .remove("page_tree");
    Session {
        ai_document_id: "test-document".into(),
        project: previous.clone(),
        saved: previous,
        path: Some(path),
        import_warnings: vec![],
        native_state: Some(state),
    }
}

fn recover_test_session(session: &Session) -> App {
    let mut storage = MemoryStorage::default();
    eframe::set_value(&mut storage, "rpshop-session", session);
    let mut creation = eframe::CreationContext::_new_kittest(egui::Context::default());
    creation.storage = Some(&storage);
    App::new(&creation, None)
}

fn remove_legacy_view_metadata(session: &mut Session) {
    let state = session.native_state.as_mut().unwrap();
    for page in state["project"]["pages"].as_array_mut().unwrap() {
        let object = page.as_object_mut().unwrap();
        object.remove("canvas");
        object["width"] = serde_json::json!(object["width"].as_f64().unwrap().max(1280.0));
        object["height"] = serde_json::json!(object["height"].as_f64().unwrap().max(800.0));
        for widget in object["widgets"].as_array_mut().unwrap() {
            widget.as_object_mut().unwrap().remove("text_layout");
        }
    }
    session.saved = serde_json::from_value(state["project"].clone()).unwrap();
    session.project = session.saved.clone();
}
