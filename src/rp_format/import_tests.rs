use super::*;
use serde_json::json;

#[test]
fn imports_auto_and_each_static_page_axis_without_inventing_a_board() {
    for (width, height, mode) in [
        (0.0, 0.0, CanvasMode::Auto),
        (600.0, 0.0, CanvasMode::FixedWidth),
        (0.0, 400.0, CanvasMode::FixedHeight),
        (600.0, 400.0, CanvasMode::Fixed),
    ] {
        let (mut graph, roots) = test_graph();
        graph.objects.get_mut("doc").unwrap().properties["PageStyleSheet"] =
            json!({"$ref":"page-sheet"});
        graph.objects.insert(
            "page-sheet".into(),
            node(
                "Axure:PageStyleSheet",
                json!({"DefaultPageStyle":{"$ref":"default-page-style"}}),
            ),
        );
        graph.objects.insert(
            "default-page-style".into(),
            node(
                "Axure:PageStyle",
                json!({"PropList":[{"DOAName":105,"Value":{"Width":width,"Height":height}}]}),
            ),
        );
        let styles = Styles::new(&graph);
        let mut converter = Converter {
            graph: &graph,
            roots: &roots,
            styles: &styles,
            next_id: 1,
            warnings: BTreeMap::new(),
            pending_links: Vec::new(),
            bindings: BTreeMap::new(),
            preview_parents: BTreeMap::new(),
            group_bindings: BTreeMap::new(),
        };
        let page = converter.page("package-a", "size fixture").unwrap();
        assert_eq!(page.canvas, mode);
        assert_eq!(page.width, if width > 0.0 { width } else { 204.0 });
        assert_eq!(page.height, if height > 0.0 { height } else { 72.0 });
    }
}

fn node(kind: &str, properties: Value) -> Node {
    Node {
        kind: kind.into(),
        properties,
    }
}

fn test_graph() -> (Graph, BTreeMap<String, String>) {
    let objects = BTreeMap::from([
        (
            "doc".into(),
            node("Axure:DesignDocument", json!({"sitemap":{"$ref":"map"}})),
        ),
        (
            "map".into(),
            node(
                "Axure:SiteTreeMap",
                json!({"tree":{"$ref":"tree"},"node-table":{
                    "node-b":{"$ref":"package-b"},"node-a":{"$ref":"package-a"}
                }}),
            ),
        ),
        (
            "tree".into(),
            node(
                "Axure:Tree",
                json!({"children":[{"$ref":"node-b"},{"$ref":"node-a"}]}),
            ),
        ),
        ("node-a".into(), node("Axure:Node", json!({"children":[]}))),
        ("node-b".into(), node("Axure:Node", json!({"children":[]}))),
        (
            "info-a".into(),
            node(
                "Axure:PackageInfo",
                json!({"package-id":{"$ref":"package-a","$type":"Axure:Page"},"package-name":"页面 A"}),
            ),
        ),
        (
            "info-b".into(),
            node(
                "Axure:PackageInfo",
                json!({"package-id":{"$ref":"package-b","$type":"Axure:Page"},"package-name":"页面 B"}),
            ),
        ),
        (
            "page-a".into(),
            node("Axure:Page", json!({"diagram":{"$ref":"diagram-a"}})),
        ),
        (
            "page-b".into(),
            node("Axure:Page", json!({"diagram":{"$ref":"diagram-b"}})),
        ),
        (
            "diagram-a".into(),
            node(
                "Axure:Diagram",
                json!({"diagram-objects":[{"$ref":"widget"}]}),
            ),
        ),
        (
            "diagram-b".into(),
            node("Axure:Diagram", json!({"diagram-objects":[]})),
        ),
        (
            "widget".into(),
            node(
                "Axure:DiagramObject:VectorShape",
                json!({"Shape":0,
                    "VisibleFromSettings":true,"IsLocked":true,"MyStyle":{"$ref":"style"},
                    "name-block":{"text":"下一页"},"InteractionMap":{"$ref":"interaction"},
                    "RichTextBlock":{"attributed-string":{"Paragraphs":[{"Inlines":[{"Text":"你好","Size":15,"Color":-65536}]}]}}
                }),
            ),
        ),
        (
            "style".into(),
            node(
                "Axure:Style",
                json!({"PropList":[
                    {"DOAName":-1073741723,"Value":{"X":24,"Y":40}},
                    {"DOAName":-1073741719,"Value":{"Width":180,"Height":32}},
                    {"DOAName":-1073741817,"Value":{"fill-type":"solid","fill-color":-16711936}},
                    {"DOAName":-1073741754,"Value":0.6}
                ]}),
            ),
        ),
        (
            "interaction".into(),
            node(
                "Axure:InteractionTreeMap",
                json!({"node-table":{
                    "e":{"$ref":"event"},"c":{"$ref":"case"},"a":{"$ref":"action"}
                }}),
            ),
        ),
        (
            "event".into(),
            node("Axure:InteractionEvent", json!({"event-type":16})),
        ),
        (
            "case".into(),
            node(
                "Axure:InteractionCase",
                json!({"Condition":{"$ref":EMPTY_ID}}),
            ),
        ),
        (
            "action".into(),
            node(
                "Axure:InteractionLinkWindowAction",
                json!({"link-type":1,"target":{"$ref":"target"}}),
            ),
        ),
        (
            "target".into(),
            node(
                "Axure:InteractionTarget",
                json!({"target-type":1,"target-page-id":{"$ref":"package-b"}}),
            ),
        ),
    ]);
    let graph = Graph::new([11, 0, 0, 0], "doc".into(), "document".into(), objects);
    let roots = BTreeMap::from([
        ("package-a".into(), "page-a".into()),
        ("package-b".into(), "page-b".into()),
    ]);
    (graph, roots)
}

#[test]
fn keeps_native_tree_order_and_widget_content() {
    let (graph, roots) = test_graph();
    let names = package_names(&graph);
    assert_eq!(
        page_order(&graph, "sitemap", &names),
        vec![
            ("package-b".into(), "页面 B".into()),
            ("package-a".into(), "页面 A".into())
        ]
    );
    let styles = Styles::new(&graph);
    let mut converter = Converter {
        graph: &graph,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    let page = converter.page("package-a", "页面 A").unwrap();
    let widget = &page.widgets[0];
    assert_eq!(
        (widget.x, widget.y, widget.width, widget.height),
        (24.0, 40.0, 180.0, 32.0)
    );
    assert_eq!(widget.text, "你好");
    assert_eq!(widget.name, "下一页");
    assert_eq!(widget.fill, "#00FF00");
    assert_eq!(widget.text_color, "#FF0000");
    assert_eq!(widget.font_size, 20.0);
    assert_eq!(widget.opacity, 0.6);
    assert!(widget.locked);
    assert_eq!(
        converter.pending_links,
        vec![(widget.id, "package-b".into())]
    );
}

#[test]
fn keeps_folders_and_child_pages_without_prefixing_page_names() {
    let (mut graph, _) = test_graph();
    graph.objects.get_mut("tree").unwrap().properties["children"] = json!([{"$ref":"folder-node"}]);
    graph.objects.get_mut("map").unwrap().properties["node-table"]["folder-node"] =
        json!({"$ref":"folder"});
    graph.objects.insert(
        "folder-node".into(),
        node("Axure:Node", json!({"children":[{"$ref":"node-b"}]})),
    );
    graph.objects.insert(
        "folder".into(),
        node("Axure:Folder", json!({"name":"资料"})),
    );
    graph.objects.get_mut("node-b").unwrap().properties["children"] = json!([{"$ref":"node-a"}]);
    // A malformed graph cycle must not recursively expand forever.
    graph.objects.get_mut("node-a").unwrap().properties["children"] =
        json!([{"$ref":"folder-node"}]);
    let names = package_names(&graph);
    assert_eq!(
        page_order(&graph, "sitemap", &names),
        vec![
            ("package-b".into(), "页面 B".into()),
            ("package-a".into(), "页面 A".into())
        ]
    );
    let ids = BTreeMap::from([("package-b".into(), 3), ("package-a".into(), 7)]);
    let tree = bind_page_tree(&native_page_tree(&graph, "sitemap", &names), &ids);
    assert_eq!(
        tree,
        vec![PageTreeNode::Folder {
            id: "folder-node".into(),
            name: "资料".into(),
            children: vec![PageTreeNode::Page {
                page_id: 3,
                children: vec![PageTreeNode::Page {
                    page_id: 7,
                    children: vec![]
                }],
            }],
        }]
    );
}

#[test]
fn caller_selected_page_tree_survives_native_page_rename() {
    let Some(path) = std::env::var_os("RPSHOP_NATIVE_FIXTURE") else {
        return;
    };
    let path = Path::new(&path);
    let original = std::fs::read(path).unwrap();
    let mut native = open(path).unwrap();
    let original_tree = native.project.page_tree.clone();
    assert!(
        !original_tree.is_empty(),
        "fixture needs a native page navigation tree"
    );
    let directory = std::env::temp_dir().join(format!(
        "rpshop-page-tree-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let destination = directory.join("page-tree-roundtrip.rp");
    let mut edited = native.project.clone();
    edited.pages[0].name = "Ingenaro page rename verification".into();
    let expected_names = edited
        .pages
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>();
    native.save(&edited, &destination).unwrap();
    let mut reopened = open(&destination).unwrap();
    assert_eq!(
        reopened.project.page_tree, original_tree,
        "renaming a page must keep folder IDs, labels, and parentage"
    );
    assert_eq!(
        reopened
            .project
            .pages
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        expected_names
    );
    let before = std::fs::read(&destination).unwrap();
    let mut restructured = reopened.project.clone();
    restructured.page_tree.reverse();
    if restructured.page_tree == reopened.project.page_tree {
        restructured.page_tree.clear();
    }
    assert!(reopened.save(&restructured, &destination).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), before);
    let mut missing_tree_state = reopened.session_state();
    missing_tree_state["project"]
        .as_object_mut()
        .unwrap()
        .remove("page_tree");
    assert!(reopened.restore_session_state(&missing_tree_state).is_err());
    assert_eq!(
        std::fs::read(path).unwrap(),
        original,
        "fixture source must remain untouched"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn preserves_panel_offsets_and_inherited_visibility() {
    let (mut graph, roots) = test_graph();
    graph.objects.insert(
        "diagram-a".into(),
        node(
            "Axure:Diagram",
            json!({"diagram-objects":[{"$ref":"panel"}]}),
        ),
    );
    graph.objects.insert("panel".into(),node("Axure:DiagramObject:DynamicPanel",json!({
        "MyStyle":{"$ref":"panel-style"},"VisibleFromSettings":false,"panel-diagrams":[{"$ref":"panel-state"}]
    })));
    graph.objects.insert(
        "panel-style".into(),
        node(
            "Axure:Style",
            json!({"PropList":[
                {"DOAName":-1073741723,"Value":{"X":100,"Y":200}}
            ]}),
        ),
    );
    graph.objects.insert(
        "panel-state".into(),
        node(
            "Axure:PanelState",
            json!({"diagram":{"$ref":"panel-diagram"}}),
        ),
    );
    graph.objects.insert(
        "panel-diagram".into(),
        node(
            "Axure:PanelDiagram",
            json!({"diagram-objects":[{"$ref":"widget"}]}),
        ),
    );
    let styles = Styles::new(&graph);
    let mut converter = Converter {
        graph: &graph,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    let page = converter.page("package-a", "页面 A").unwrap();
    assert_eq!(page.widgets.len(), 2);
    assert_eq!((page.widgets[1].x, page.widgets[1].y), (124.0, 240.0));
    assert!(page.widgets.iter().all(|w| w.hidden));
    assert!(
        converter
            .warnings
            .contains_key("编辑画布显示动态面板首状态；原生预览保留所有状态与裁剪")
    );
}

#[test]
fn conditional_actions_are_not_misrepresented_as_unconditional_links() {
    let (mut graph, roots) = test_graph();
    graph.objects.get_mut("case").unwrap().properties["Condition"] = json!({"$ref":"condition"});
    let styles = Styles::new(&graph);
    let mut converter = Converter {
        graph: &graph,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    converter.page("package-a", "页面 A").unwrap();
    assert!(converter.pending_links.is_empty());
    assert!(!converter.warnings.is_empty());
}

#[test]
fn gallery_paginates_large_libraries_without_dropping_components() {
    let (graph, roots) = test_graph();
    let styles = Styles::new(&graph);
    let mut converter = Converter {
        graph: &graph,
        roots: &roots,
        styles: &styles,
        next_id: 1,
        warnings: BTreeMap::new(),
        pending_links: Vec::new(),
        bindings: BTreeMap::new(),
        preview_parents: BTreeMap::new(),
        group_bindings: BTreeMap::new(),
    };
    let order = (0..129)
        .map(|i| ("package-a".to_owned(), format!("组件{i}")))
        .collect::<Vec<_>>();
    let mut project = Project {
        version: FORMAT_VERSION,
        name: "测试元件库".into(),
        pages: Vec::new(),
        page_tree: Vec::new(),
        interaction_events: Vec::new(),
        next_id: 1,
    };
    converter
        .library_pages(&order, &mut project, &mut BTreeMap::new())
        .unwrap();
    project.next_id = converter.next_id;
    assert_eq!(project.pages.len(), 6);
    assert_eq!(
        project.pages.iter().map(|p| p.widgets.len()).sum::<usize>(),
        129 * 2
    );
    project.validate().unwrap();
}

#[test]
fn reads_bundled_rp11_projects_and_all_widget_libraries_when_available() {
    let base = Path::new(r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings");
    if !base.is_dir() {
        return;
    }
    for (folder, file, pages) in [
        ("Training", "Quick Win.rp", 2),
        ("Training", "Prototyping Basics.rp", 1),
        ("Training", "Prototype Starter.rp", 8),
        ("Training", "UX Prototyping.rp", 10),
        ("Libraries", "Default.rplib", 2),
        ("Libraries", "Flow.rplib", 2),
        ("Libraries", "Icons.rplib", 39),
        ("Libraries", "Sample form patterns.rplib", 2),
        ("Libraries", "Sample UI patterns.rplib", 1),
    ] {
        let source = base.join(folder).join(file);
        let result = import(&source).unwrap_or_else(|e| panic!("{file}: {e:#}"));
        result.project.validate().unwrap();
        assert_eq!(
            result.project.pages.len(),
            pages,
            "{file}: unexpected page count"
        );
        let count = result
            .project
            .pages
            .iter()
            .map(|p| p.widgets.len())
            .sum::<usize>();
        assert!(count > 0, "{file}: no content");
        if file == "Quick Win.rp" {
            assert_eq!(result.project.pages[0].name, "Quick win");
            assert_eq!(result.project.pages[0].widgets.len(), 12);
            assert!(
                result.project.pages[0]
                    .widgets
                    .iter()
                    .any(|w| w.image_data.is_some())
            );
        }
        if file == "Icons.rplib" {
            assert!(count >= 1840, "all 920 icons and labels should be imported");
            assert_eq!(
                result
                    .project
                    .pages
                    .iter()
                    .flat_map(|p| &p.widgets)
                    .filter(|w| w.image_data.is_some())
                    .count(),
                920
            );
        }
        eprintln!(
            "{file}: {} pages, {count} widgets, {} notices",
            result.project.pages.len(),
            result.warnings.len()
        );
    }
}

#[test]
fn native_save_preserves_source_and_roundtrips_edits_additions_and_deletions() {
    let source = Path::new(
        r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings\Training\Quick Win.rp",
    );
    if !source.is_file() {
        return;
    }
    let original = std::fs::read(source).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "rpshop-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let destination = directory.join("native-roundtrip.rp");
    let mut native = open(source).unwrap();
    native.save(&native.project.clone(), &destination).unwrap();
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        original,
        "unchanged Save As must be byte exact"
    );
    let mut project = native.project.clone();
    let second_page_before = project.pages[1].widgets.clone();
    let edited = &mut project.pages[0].widgets[1];
    edited.name = "原生编辑测试".into();
    edited.text = "中文文本，保存回 RP".into();
    edited.x += 37.0;
    edited.fill = "#123456".into();
    edited.text_color = "#ABCDEF".into();
    edited.font_size = 28.0;
    edited.locked = true;
    let id = project.allocate_id();
    let mut added = Widget::new(id, WidgetKind::Rectangle, 222.0, 333.0);
    added.name = "新增原生矩形".into();
    added.text = "新增元件".into();
    added.interaction = Interaction {
        action: Action::Navigate,
        target_page: Some(project.pages[1].id),
        ..Default::default()
    };
    project.pages[0].widgets.push(added);
    project.pages[0].widgets.remove(0);
    native.save(&project, &destination).unwrap();
    let reopened = open(&destination).unwrap();
    let widget = reopened.project.pages[0]
        .widgets
        .iter()
        .find(|w| w.name == "原生编辑测试")
        .unwrap();
    assert_eq!(widget.text, "中文文本，保存回 RP");
    assert_eq!(
        widget.x,
        project.pages[0]
            .widgets
            .iter()
            .find(|w| w.name == "原生编辑测试")
            .unwrap()
            .x
    );
    assert_eq!(widget.fill, "#123456");
    assert_eq!(widget.text_color, "#ABCDEF");
    assert_eq!(widget.font_size, 28.0);
    assert!(widget.locked);
    assert!(
        reopened.project.pages[0]
            .widgets
            .iter()
            .any(|w| w.name == "新增原生矩形" && w.text == "新增元件")
    );
    let added = reopened.project.pages[0]
        .widgets
        .iter()
        .find(|w| w.name == "新增原生矩形")
        .unwrap();
    assert_eq!(added.interaction.action, Action::Navigate);
    let mut next = native.project.clone();
    let id = next.allocate_id();
    let mut input = Widget::new(id, WidgetKind::Input, 50.0, 60.0);
    input.text = "保存输入框内容".into();
    input.name = "新增输入框".into();
    next.pages[0].widgets.push(input);
    native.save(&next, &destination).unwrap();
    assert!(
        native.project.pages[0]
            .widgets
            .iter()
            .any(|w| w.name == "新增输入框" && w.text == "保存输入框内容")
    );
    assert_eq!(reopened.project.pages[0].widgets.len(), 12);
    assert_eq!(
        reopened.project.pages[1].widgets, second_page_before,
        "duplicate GUIDs in copied pages must stay scoped to their own package"
    );
    assert_eq!(
        std::fs::read(source).unwrap(),
        original,
        "installed source must never be modified by tests"
    );
    let state = native.session_state();
    let mut restored = open(&destination).unwrap();
    restored.restore_session_state(&state).unwrap();
    assert_eq!(restored.project, native.project);
    let mut invalid = state;
    invalid["fingerprint"] = json!("changed");
    assert!(restored.restore_session_state(&invalid).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
