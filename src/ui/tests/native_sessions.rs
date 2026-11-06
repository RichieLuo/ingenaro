use super::*;

#[test]
fn save_cannot_overwrite_native_files_without_a_native_source_graph() {
    let path = std::env::temp_dir().join(format!(
        "rpshop-ui-save-guard-{}-{}.rp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let bytes = b"original-native-document";
    std::fs::write(&path, bytes).unwrap();
    let mut app = fixture(Project::blank());
    assert!(app.write_document(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_ctrl_s_keeps_rp_format_and_session_recovers_unsaved_native_edits() {
    let source = std::path::Path::new(
        r"D:\Program Files (x86)\Axure\Axure RP 11\DefaultSettings\Training\Quick Win.rp",
    );
    if !source.is_file() {
        return;
    }
    let source_bytes = std::fs::read(source).unwrap();
    let path = std::env::temp_dir().join(format!(
        "rpshop-ui-native-{}-{}.rp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, &source_bytes).unwrap();
    let document = load_document(&path).unwrap();
    assert_eq!(document.save_path.as_ref(), Some(&path));
    assert!(document.native.is_some());
    let mut app = fixture(document.project);
    app.path = document.save_path;
    app.native = document.native;
    app.screenshot_mode = false;
    let widget = app.project.pages[0]
        .widgets
        .iter_mut()
        .find(|w| !w.text.is_empty() && w.kind != WidgetKind::Image)
        .unwrap();
    widget.text = "Ctrl+S 原生保存成功".into();
    assert!(app.save_project(false), "{}", app.status);
    assert_eq!(std::fs::read(&path).unwrap()[..4], [0xac, 0xef, 11, 0]);
    let reopened = load_document(&path).unwrap();
    assert!(
        reopened.project.pages[0]
            .widgets
            .iter()
            .any(|w| w.text == "Ctrl+S 原生保存成功")
    );
    assert_eq!(app.project, app.saved);
    let widget = app.project.pages[0]
        .widgets
        .iter_mut()
        .find(|w| w.text == "Ctrl+S 原生保存成功")
        .unwrap();
    widget.text = "恢复尚未保存的原生修改".into();
    let mut storage = MemoryStorage::default();
    app.save(&mut storage);
    let mut creation = eframe::CreationContext::_new_kittest(egui::Context::default());
    creation.storage = Some(&storage);
    let mut restored = App::new(&creation, None);
    assert!(restored.native.is_some(), "{}", restored.status);
    assert_ne!(restored.project, restored.saved);
    assert!(restored.save_project(false), "{}", restored.status);
    assert!(
        load_document(&path).unwrap().project.pages[0]
            .widgets
            .iter()
            .any(|w| w.text == "恢复尚未保存的原生修改")
    );
    assert_eq!(std::fs::read(source).unwrap(), source_bytes);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_view_metadata_upgrade_preserves_unsaved_edits_and_old_tree_upgrade() {
    let Some((path, original, document)) = native_tree_upgrade_fixture() else {
        return;
    };
    for missing_tree in [false, true] {
        let mut session = if missing_tree {
            old_tree_session(&document, path.clone())
        } else {
            Session {
                ai_document_id: "test-document".into(),
                project: document.project.clone(),
                saved: document.project.clone(),
                path: Some(path.clone()),
                import_warnings: vec![],
                native_state: Some(document.native.as_ref().unwrap().session_state()),
            }
        };
        remove_legacy_view_metadata(&mut session);
        session.project.pages[0].name = "unsaved view upgrade page name".into();
        let widget = &mut session.project.pages[0].widgets[0];
        let id = widget.id;
        widget.text = "unsaved view upgrade text".into();
        widget.x += 7.0;
        let x = widget.x;
        let restored = recover_test_session(&session);
        assert!(restored.native.is_some(), "{}", restored.status);
        assert_eq!(restored.saved, document.project);
        assert_eq!(restored.project.page_tree, document.project.page_tree);
        assert_eq!(
            restored.project.pages[0].canvas,
            document.project.pages[0].canvas
        );
        assert_eq!(
            restored.project.pages[0].width,
            document.project.pages[0].width
        );
        assert_eq!(
            restored.project.pages[0].height,
            document.project.pages[0].height
        );
        assert_eq!(
            restored.project.pages[0].name,
            "unsaved view upgrade page name"
        );
        let widget = restored.project.pages[0]
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap();
        assert_eq!(widget.text, "unsaved view upgrade text");
        assert_eq!(widget.x, x);
        assert_eq!(
            widget.text_layout,
            document.project.pages[0].widgets[0].text_layout
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn native_view_upgrade_rejects_unrelated_baseline_changes_without_losing_draft() {
    let Some((path, original, document)) = native_tree_upgrade_fixture() else {
        return;
    };
    let mut session = old_tree_session(&document, path.clone());
    remove_legacy_view_metadata(&mut session);
    session.saved.pages[0].widgets[0].x += 30.0;
    session.native_state.as_mut().unwrap()["project"]["pages"][0]["widgets"][0]["x"] =
        serde_json::json!(session.saved.pages[0].widgets[0].x);
    session.project = session.saved.clone();
    session.project.pages[0].widgets[0].text = "retain incompatible draft".into();
    let restored = recover_test_session(&session);
    assert!(restored.native.is_none());
    assert_eq!(restored.project, session.project);
    assert_eq!(restored.saved, session.saved);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn upgrading_legacy_native_session_keeps_unsaved_edits_and_restores_tree() {
    let Some((path, original, document)) = native_tree_upgrade_fixture() else {
        return;
    };
    assert!(!document.project.page_tree.is_empty());
    let mut session = old_tree_session(&document, path.clone());
    let widget = session.project.pages[0].widgets.first_mut().unwrap();
    let id = widget.id;
    widget.text = "unsaved native draft during tree upgrade".into();
    session.project.pages[0].name = "unsaved page rename".into();
    let restored = recover_test_session(&session);
    assert!(restored.native.is_some(), "{}", restored.status);
    assert_eq!(restored.project.page_tree, document.project.page_tree);
    assert_eq!(restored.saved, document.project);
    assert_eq!(restored.project.pages[0].name, "unsaved page rename");
    assert_eq!(
        restored.project.pages[0]
            .widgets
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .text,
        "unsaved native draft during tree upgrade"
    );
    for (page, fresh) in restored
        .project
        .pages
        .iter()
        .zip(&document.project.pages)
        .skip(1)
    {
        assert_eq!(page.name, fresh.name);
    }
    assert_ne!(restored.project, restored.saved);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn clean_legacy_session_reopens_source_but_unmatched_dirty_draft_is_preserved() {
    let Some((path, original, document)) = native_tree_upgrade_fixture() else {
        return;
    };
    let mut session = old_tree_session(&document, path.clone());
    session.native_state.as_mut().unwrap()["fingerprint"] =
        serde_json::json!("old fingerprint cannot rebind");
    let restored = recover_test_session(&session);
    assert!(restored.native.is_some());
    assert_eq!(restored.project, document.project);
    assert_eq!(restored.saved, document.project);
    session.project.pages[0].widgets[0].text = "must retain unmatched dirty draft".into();
    let expected = session.project.clone();
    let restored = recover_test_session(&session);
    assert!(restored.native.is_none());
    assert_eq!(restored.project, expected);
    assert_eq!(restored.saved, session.saved);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn customer_master_preview_groups_save_rebind_and_ungroup_without_touching_source() {
    use rpshop::rp_format::{binary, container};
    use std::collections::BTreeMap;

    let Some(source) = std::env::var_os("RPSHOP_NATIVE_FIXTURE").map(PathBuf::from) else {
        return;
    };
    let source_bytes = std::fs::read(&source).unwrap();
    let document = load_document(&source).unwrap();
    let native = document.native.expect("fixture must be a native document");
    let source_container = container::from_bytes(&source_bytes).unwrap();
    let state = native.session_state();

    fn package_offset(node: &serde_json::Value, package: &str) -> Option<u64> {
        let children = node.get("children")?.as_object()?;
        if let Some(offset) = children
            .get(package)
            .and_then(|child| child.pointer("/parts/package"))
            .and_then(serde_json::Value::as_u64)
        {
            return Some(offset);
        }
        children
            .values()
            .find_map(|child| package_offset(child, package))
    }

    // Prove both groups belong to the same native root diagram. Equal editor
    // paths alone cannot distinguish a root layer from a dynamic-panel state.
    let mut root_objects: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut root_groups = BTreeMap::new();
    for (id, binding) in state["group_bindings"].as_object().unwrap() {
        let Some((package, object)) = binding["object"]
            .as_str()
            .and_then(|object| object.rsplit_once('/'))
        else {
            continue;
        };
        if !root_objects.contains_key(package) {
            let Some(offset) = package_offset(&source_container.index["root"], package) else {
                continue;
            };
            let graph = binary::parse(&source_container.part(offset).unwrap()).unwrap();
            let roots = graph
                .root()
                .and_then(|root| root.properties.get("diagram"))
                .and_then(binary::reference_id)
                .and_then(|diagram| graph.node(diagram))
                .and_then(|diagram| diagram.properties["diagram-objects"].as_array())
                .map(|objects| {
                    objects
                        .iter()
                        .filter_map(binary::reference_id)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            root_objects.insert(package.to_owned(), roots);
        }
        if root_objects[package].contains(object) {
            root_groups.insert(id.parse::<u64>().unwrap(), package.to_owned());
        }
    }

    let eligible = |widget: &Widget| {
        native.selection_owner(widget.id) != widget.id
            || (!widget.locked && native.is_editable_widget(widget.id))
    };
    let mut fallback = None;
    let mut preferred = None;
    for (page_index, page) in document.project.pages.iter().enumerate() {
        for preview in &page.widgets {
            let owner = native.selection_owner(preview.id);
            if owner == preview.id {
                continue;
            }
            let Some(instance) = page
                .widgets
                .iter()
                .find(|widget| widget.id == owner && eligible(widget))
            else {
                continue;
            };
            let Some(group) = instance.groups.first().copied() else {
                continue;
            };
            let Some(package) = root_groups.get(&group) else {
                continue;
            };
            let members = page
                .widgets
                .iter()
                .filter(|widget| widget.groups.first() == Some(&group))
                .collect::<Vec<_>>();
            if !members.iter().all(|widget| eligible(widget)) {
                continue;
            }
            let selected = members
                .iter()
                .map(|widget| widget.id)
                .collect::<BTreeSet<_>>();
            if fallback.is_none() {
                fallback = Some((page_index, owner, selected.clone(), true));
            }
            let sibling = root_groups.iter().find_map(|(id, sibling_package)| {
                if *id == group || sibling_package != package {
                    return None;
                }
                let members = page
                    .widgets
                    .iter()
                    .filter(|widget| widget.groups.first() == Some(id))
                    .collect::<Vec<_>>();
                (!members.is_empty() && members.iter().all(|widget| eligible(widget))).then(|| {
                    members
                        .iter()
                        .map(|widget| widget.id)
                        .collect::<BTreeSet<_>>()
                })
            });
            if let Some(sibling) = sibling {
                preferred = Some((
                    page_index,
                    owner,
                    selected.union(&sibling).copied().collect(),
                    false,
                ));
                break;
            }
        }
        if preferred.is_some() {
            break;
        }
    }
    let (page_index, owner, selected, regroup_existing) = preferred
        .or(fallback)
        .expect("fixture needs an editable native root group containing a master instance preview");
    eprintln!(
        "native preview group regression: {} widgets on page {}, regroup existing: {regroup_existing}",
        selected.len(),
        page_index + 1
    );
    let path = std::env::temp_dir().join(format!(
        "rpshop-ui-preview-group-{}-{}.rp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert_ne!(path, source);
    struct TemporaryFile(PathBuf);
    impl Drop for TemporaryFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _temporary = TemporaryFile(path.clone());
    let mut app = fixture(document.project);
    app.native = Some(native);
    app.page = page_index;
    app.path = Some(path.clone());
    app.selected = selected;
    let marker = "RPShop master preview group regression";
    app.project.pages[page_index]
        .widgets
        .iter_mut()
        .find(|widget| widget.id == owner)
        .unwrap()
        .name = marker.into();
    if regroup_existing {
        app.group_selection(true);
        assert_eq!(app.status, "已解组");
    }
    app.group_selection(false);
    assert_eq!(app.status, "已组合");
    app.project.validate().unwrap();
    let grouped = app.project.pages[page_index]
        .widgets
        .iter()
        .find(|widget| widget.id == owner)
        .unwrap()
        .groups[0];
    assert!(
        app.project.pages[page_index]
            .widgets
            .iter()
            .filter(|widget| app.selected.contains(&widget.id))
            .all(|widget| widget.groups.first() == Some(&grouped))
    );
    let assert_preview_locks = |app: &App| {
        let native = app.native.as_ref().unwrap();
        let previews = app
            .project
            .pages
            .iter()
            .flat_map(|page| &page.widgets)
            .filter(|widget| native.selection_owner(widget.id) != widget.id)
            .collect::<Vec<_>>();
        assert!(!previews.is_empty());
        assert!(
            previews.iter().all(|widget| widget.locked),
            "synthetic preview locks must survive grouping and saving"
        );
    };
    assert_preview_locks(&app);
    assert!(app.save_project(false), "{}", app.status);
    assert_eq!(app.project, app.native.as_ref().unwrap().project);
    assert_eq!(app.project, app.saved);
    assert_preview_locks(&app);
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);

    // Native saving reopens the result and can allocate different widget IDs.
    // A second Ctrl+S must use those refreshed bindings and preview ownership.
    let before_move = app.project.clone();
    let instance = app
        .project
        .pages
        .iter_mut()
        .flat_map(|page| &mut page.widgets)
        .find(|widget| widget.name == marker)
        .unwrap();
    instance.x += 7.0;
    let expected_x = instance.x;
    app.remember(before_move);
    assert!(app.save_project(false), "second Ctrl+S: {}", app.status);
    assert_eq!(app.project, app.native.as_ref().unwrap().project);
    assert_preview_locks(&app);
    let instance = app
        .project
        .pages
        .iter()
        .flat_map(|page| &page.widgets)
        .find(|widget| widget.name == marker)
        .unwrap();
    assert!((instance.x - expected_x).abs() < 0.01);
    let grouped_depth = instance.groups.len();
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);

    // Exercise a true disk reopen, rather than reusing the first native graph.
    let reopened = load_document(&path).unwrap();
    let mut reopened_app = fixture(reopened.project);
    reopened_app.native = reopened.native;
    reopened_app.path = Some(path.clone());
    let (page_index, id) = reopened_app
        .project
        .pages
        .iter()
        .enumerate()
        .find_map(|(index, page)| {
            page.widgets
                .iter()
                .find(|widget| widget.name == marker)
                .map(|widget| (index, widget.id))
        })
        .unwrap();
    reopened_app.page = page_index;
    reopened_app.selected = rpshop::grouping::expanded_selection(
        &reopened_app.project.pages[page_index],
        &BTreeSet::from([id]),
        false,
    );
    let previous_paths = reopened_app.project.pages[page_index]
        .widgets
        .iter()
        .filter(|widget| reopened_app.selected.contains(&widget.id))
        .map(|widget| (widget.id, widget.groups.clone()))
        .collect::<BTreeMap<_, _>>();
    reopened_app.group_selection(true);
    assert_eq!(reopened_app.status, "已解组");
    reopened_app.project.validate().unwrap();
    for widget in reopened_app.project.pages[page_index]
        .widgets
        .iter()
        .filter(|widget| previous_paths.contains_key(&widget.id))
    {
        assert_eq!(widget.groups, previous_paths[&widget.id][1..]);
    }
    assert_preview_locks(&reopened_app);
    assert!(
        reopened_app.save_project(false),
        "save after reopened ungroup: {}",
        reopened_app.status
    );
    assert_eq!(
        reopened_app.project,
        reopened_app.native.as_ref().unwrap().project
    );
    let instance = reopened_app
        .project
        .pages
        .iter()
        .flat_map(|page| &page.widgets)
        .find(|widget| widget.name == marker)
        .unwrap();
    assert_eq!(instance.groups.len() + 1, grouped_depth);
    assert_preview_locks(&reopened_app);
    assert_eq!(
        std::fs::read(&source).unwrap(),
        source_bytes,
        "the user-selected source must remain byte-exact"
    );
}
