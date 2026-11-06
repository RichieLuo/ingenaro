use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
use rpshop::model::{PageTreeNode, Project};

use super::sidebar;

#[derive(Default)]
pub(super) struct NavigationAction {
    pub activate: Option<usize>,
    pub remove: Option<usize>,
}

pub(super) fn show(
    ui: &mut egui::Ui,
    project: &Project,
    current: usize,
    editable: bool,
) -> NavigationAction {
    let search_id = ui.make_persistent_id(("page-search", &project.name));
    let mut search = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(search_id).unwrap_or_default());
    ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text("搜索页面和文件夹")
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .data_mut(|d| d.insert_temp(search_id, search.clone()));
    let query = search.trim().to_lowercase();
    let tree = if project.page_tree.is_empty() {
        project
            .pages
            .iter()
            .map(|p| PageTreeNode::Page {
                page_id: p.id,
                children: Vec::new(),
            })
            .collect::<Vec<_>>()
    } else {
        project.page_tree.clone()
    };
    let page_id = project.pages[current].id;
    let previous_id = ui.make_persistent_id(("page-tree-current", &project.name));
    let changed = ui.ctx().data_mut(|d| {
        let changed = d.get_temp::<u64>(previous_id) != Some(page_id);
        d.insert_temp(previous_id, page_id);
        changed
    });
    let mut action = NavigationAction::default();
    ui.add_space(5.0);
    egui::ScrollArea::vertical()
        .id_salt("page-list")
        .max_height(ui.available_height())
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let mut renderer = TreeRenderer {
                project,
                current,
                editable,
                query: &query,
                changed,
                action: &mut action,
            };
            renderer.nodes(ui, &tree, 0, false);
            if !query.is_empty() && !tree.iter().any(|n| matches_query(n, project, &query)) {
                ui.label(
                    egui::RichText::new("没有匹配的页面")
                        .small()
                        .color(Color32::GRAY),
                );
            }
        });
    action
}

fn matches_query(node: &PageTreeNode, project: &Project, query: &str) -> bool {
    node_name(node, project).to_lowercase().contains(query)
        || node
            .children()
            .iter()
            .any(|child| matches_query(child, project, query))
}

fn contains_page(node: &PageTreeNode, id: u64) -> bool {
    matches!(node, PageTreeNode::Page { page_id, .. } if *page_id == id)
        || node.children().iter().any(|n| contains_page(n, id))
}

fn node_name<'a>(node: &'a PageTreeNode, project: &'a Project) -> &'a str {
    match node {
        PageTreeNode::Folder { name, .. } => name,
        PageTreeNode::Page { page_id, .. } => project
            .pages
            .iter()
            .find(|p| p.id == *page_id)
            .map_or("页面不存在", |p| &p.name),
    }
}

struct TreeRenderer<'a> {
    project: &'a Project,
    current: usize,
    editable: bool,
    query: &'a str,
    changed: bool,
    action: &'a mut NavigationAction,
}

impl TreeRenderer<'_> {
    fn nodes(
        &mut self,
        ui: &mut egui::Ui,
        nodes: &[PageTreeNode],
        depth: usize,
        parent_matches: bool,
    ) {
        for node in nodes {
            if !self.query.is_empty()
                && !parent_matches
                && !matches_query(node, self.project, self.query)
            {
                continue;
            }
            let name = node_name(node, self.project);
            let (folder, key, page_index) = match node {
                PageTreeNode::Folder { id, .. } => (true, format!("folder:{id}"), None),
                PageTreeNode::Page { page_id, .. } => (
                    false,
                    format!("page:{page_id}"),
                    self.project.pages.iter().position(|p| p.id == *page_id),
                ),
            };
            let state_id = ui.make_persistent_id(("page-tree-node", &self.project.name, key));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                state_id,
                depth == 0,
            );
            if self.changed && contains_page(node, self.project.pages[self.current].id) {
                state.set_open(true);
            }
            let searching = !self.query.is_empty();
            let open = searching || state.is_open();
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 27.0), Sense::click());
            let selected = page_index == Some(self.current);
            if selected || response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    if selected {
                        Color32::from_rgb(222, 248, 250)
                    } else {
                        Color32::from_rgb(245, 247, 250)
                    },
                );
            }
            let left = rect.left() + depth as f32 * 15.0;
            let center_y = rect.center().y;
            if !node.children().is_empty() {
                sidebar::paint_disclosure(
                    ui.painter(),
                    Pos2::new(left + 6.0, center_y),
                    open,
                    Color32::from_gray(140),
                );
            }
            let icon =
                Rect::from_min_size(Pos2::new(left + 14.0, center_y - 6.0), vec2(13.0, 12.0));
            if folder {
                ui.painter().rect_filled(
                    Rect::from_min_size(icon.min, vec2(6.0, 3.0)),
                    0.0,
                    Color32::from_rgb(0, 145, 218),
                );
                ui.painter().rect_filled(
                    Rect::from_min_max(icon.min + vec2(0.0, 2.0), icon.max),
                    0.0,
                    Color32::from_rgb(0, 161, 232),
                );
            } else {
                let paper = icon.shrink2(vec2(2.0, 0.0));
                ui.painter().rect_stroke(
                    paper,
                    0.0,
                    Stroke::new(1.0, Color32::from_gray(150)),
                    StrokeKind::Inside,
                );
                for y in [3.0, 5.5, 8.0] {
                    ui.painter().line_segment(
                        [paper.min + vec2(2.0, y), paper.min + vec2(7.0, y)],
                        Stroke::new(0.7, Color32::from_gray(150)),
                    );
                }
            }
            ui.painter()
                .with_clip_rect(rect.intersect(ui.clip_rect()))
                .text(
                    Pos2::new(left + 31.0, center_y),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(12.0),
                    Color32::from_rgb(51, 55, 63),
                );
            if response.clicked() {
                let arrow_clicked = response
                    .interact_pointer_pos()
                    .is_some_and(|p| p.x < left + 14.0);
                if folder || (arrow_clicked && !node.children().is_empty()) {
                    state.set_open(!state.is_open());
                    ui.ctx().request_repaint();
                } else if let Some(index) = page_index {
                    self.action.activate = Some(index);
                }
            }
            if self.changed && selected {
                // Reveal offscreen selections without moving rows already in view.
                response.scroll_to_me(None);
            }
            response.clone().on_hover_text(name);
            if let Some(index) = page_index {
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(
                            self.editable && self.project.pages.len() > 1,
                            egui::Button::new("删除页面"),
                        )
                        .clicked()
                    {
                        self.action.remove = Some(index);
                        ui.close();
                    }
                });
            }
            state.store(ui.ctx());
            if open {
                self.nodes(
                    ui,
                    node.children(),
                    depth + 1,
                    parent_matches
                        || (!self.query.is_empty() && name.to_lowercase().contains(self.query)),
                );
            }
        }
    }
}

pub(super) fn remove_entry(nodes: &mut Vec<PageTreeNode>, id: u64) {
    let mut i = 0;
    while i < nodes.len() {
        if matches!(nodes[i], PageTreeNode::Page { page_id, .. } if page_id == id) {
            let PageTreeNode::Page { children, .. } = nodes.remove(i) else {
                unreachable!()
            };
            let count = children.len();
            nodes.splice(i..i, children);
            i += count;
        } else {
            let children = match &mut nodes[i] {
                PageTreeNode::Folder { children, .. } | PageTreeNode::Page { children, .. } => {
                    children
                }
            };
            remove_entry(children, id);
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_tree_roundtrips_through_desktop_storage() {
        #[derive(Default)]
        struct Storage(String);
        impl eframe::Storage for Storage {
            fn get_string(&self, _: &str) -> Option<String> {
                Some(self.0.clone())
            }
            fn set_string(&mut self, _: &str, value: String) {
                self.0 = value;
            }
            fn flush(&mut self) {}
        }
        let mut project = Project::blank();
        let mut storage = Storage::default();
        eframe::set_value(&mut storage, "project", &project);
        assert_eq!(
            eframe::get_value::<Project>(&storage, "project"),
            Some(project.clone())
        );
        project.page_tree = vec![PageTreeNode::Page {
            page_id: project.pages[0].id,
            children: vec![],
        }];
        eframe::set_value(&mut storage, "project", &project);
        assert_eq!(
            eframe::get_value::<Project>(&storage, "project"),
            Some(project)
        );
    }

    #[test]
    fn search_finds_folder_or_renamed_leaf_and_removal_keeps_descendants() {
        let mut project = Project::default();
        let first = project.pages[0].id;
        let second = project.pages[1].id;
        let tree = PageTreeNode::Folder {
            id: "docs".into(),
            name: "文档".into(),
            children: vec![PageTreeNode::Page {
                page_id: first,
                children: vec![PageTreeNode::Page {
                    page_id: second,
                    children: vec![],
                }],
            }],
        };
        assert!(matches_query(&tree, &project, "文档"));
        project.pages[1].name = "查找页面".into();
        assert!(matches_query(&tree, &project, "查找"));
        assert!(!matches_query(&tree, &project, "不存在"));
        project.page_tree = vec![tree];
        for page in &mut project.pages {
            page.widgets.clear();
        }
        remove_entry(&mut project.page_tree, first);
        project.pages.remove(0);
        project.validate().unwrap();
        assert!(contains_page(&project.page_tree[0], second));
        assert!(!contains_page(&project.page_tree[0], first));
    }

    #[test]
    fn folder_click_expands_child_and_page_click_navigates() {
        let mut project = Project::default();
        project.pages[0].name = "first-page".into();
        project.pages[1].name = "second-page".into();
        project.page_tree = vec![PageTreeNode::Folder {
            id: "root".into(),
            name: "root-folder".into(),
            children: vec![
                PageTreeNode::Page {
                    page_id: project.pages[0].id,
                    children: vec![],
                },
                PageTreeNode::Folder {
                    id: "child".into(),
                    name: "child-folder".into(),
                    children: vec![PageTreeNode::Page {
                        page_id: project.pages[1].id,
                        children: vec![],
                    }],
                },
            ],
        }];
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |events: Vec<egui::Event>| {
            time += 0.1;
            let mut action = NavigationAction::default();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 800.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        action = show(ui, &project, 0, true);
                    });
                },
            );
            (output, action)
        };
        fn text_position(output: &egui::FullOutput, text: &str) -> Option<Pos2> {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(shape) if shape.galley.text() == text => {
                    Some(shape.pos + shape.galley.size() / 2.0)
                }
                _ => None,
            })
        }
        fn button(pos: Pos2, pressed: bool) -> egui::Event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }
        }
        let (initial, _) = frame(vec![]);
        assert!(text_position(&initial, "second-page").is_none());
        let folder = text_position(&initial, "child-folder").unwrap();
        frame(vec![
            egui::Event::PointerMoved(folder),
            button(folder, true),
        ]);
        frame(vec![button(folder, false)]);
        let (expanded, _) = frame(vec![]);
        let page = text_position(&expanded, "second-page")
            .expect("opening a folder reveals its child page");
        frame(vec![egui::Event::PointerMoved(page), button(page, true)]);
        let (_, action) = frame(vec![button(page, false)]);
        assert_eq!(action.activate, Some(1));
        frame(vec![
            egui::Event::PointerMoved(folder),
            button(folder, true),
        ]);
        frame(vec![button(folder, false)]);
        assert!(text_position(&frame(vec![]).0, "second-page").is_none());
    }
}
