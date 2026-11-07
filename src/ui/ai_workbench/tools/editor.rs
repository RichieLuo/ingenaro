use super::*;
use crate::ui::{arrange, editing, viewport};
use serde::{Deserialize, de::DeserializeOwned};

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OpenSource {
    #[default]
    File,
    Demo,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenProject {
    #[serde(default)]
    source: OpenSource,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum HistoryAction {
    Undo,
    Redo,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    action: HistoryAction,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ViewFocus {
    FitPage,
    CenterPage,
    FitSelection,
    CenterSelection,
    Origin,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetView {
    page_id: Option<u64>,
    widget_ids: Option<Vec<u64>>,
    zoom: Option<f32>,
    focus: Option<ViewFocus>,
    grid: Option<bool>,
    snap: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WidgetAction {
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    DistributeHorizontal,
    DistributeVertical,
    RotateCounterclockwise,
    RotateClockwise,
    FlipHorizontal,
    FlipVertical,
    Duplicate,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditWidgets {
    page_id: Option<u64>,
    target_page_id: Option<u64>,
    widget_ids: Vec<u64>,
    action: WidgetAction,
}

fn parse<T: DeserializeOwned>(call: &automation::ToolCall) -> Result<T, String> {
    serde_json::from_value(call.arguments.clone()).map_err(|error| format!("工具参数无效：{error}"))
}

fn widget_ids(page: &Page, requested: Vec<u64>) -> Result<BTreeSet<u64>, String> {
    let ids = requested.iter().copied().collect::<BTreeSet<_>>();
    if requested.len() > 5000 || ids.len() != requested.len() {
        return Err("组件 ID 必须唯一且最多可指定 5000 个".into());
    }
    let available = page.widgets.iter().map(|widget| widget.id).collect();
    if !ids.is_subset(&available) {
        return Err("指定组件不在目标页面".into());
    }
    Ok(ids)
}

impl App {
    pub(super) fn ai_editor_tool(
        &mut self,
        ctx: &egui::Context,
        call: &automation::ToolCall,
        request: Option<&automation::ToolRequest>,
    ) -> Result<Value, String> {
        match call.name.as_str() {
            "rpshop_open_project" => {
                let args: OpenProject = parse(call)?;
                self.request_replace(match args.source {
                    OpenSource::File => "open",
                    OpenSource::Demo => "demo",
                });
                Ok(
                    json!({"open_requested":true,"note":"工程切换后旧会话权限失效，请用户发起新一轮。"}),
                )
            }
            "rpshop_history" => {
                let args: History = parse(call)?;
                let changed = match args.action {
                    HistoryAction::Undo if !self.undo.is_empty() => {
                        self.undo();
                        true
                    }
                    HistoryAction::Redo if !self.redo.is_empty() => {
                        self.redo();
                        true
                    }
                    _ => false,
                };
                ctx.request_repaint();
                Ok(
                    json!({"changed":changed,"can_undo":!self.undo.is_empty(),"can_redo":!self.redo.is_empty()}),
                )
            }
            "rpshop_read_view" => {
                if call
                    .arguments
                    .as_object()
                    .is_none_or(|args| !args.is_empty())
                {
                    return Err("此工具无需参数".into());
                }
                Ok(json!({
                    "page_id":self.project.pages[self.page].id,
                    "widget_ids":self.selected,"zoom":self.zoom,"grid":self.grid,"snap":self.snap,
                    "can_undo":!self.undo.is_empty(),"can_redo":!self.redo.is_empty()
                }))
            }
            "rpshop_set_view" => self.ai_set_editor_view(ctx, parse(call)?),
            "rpshop_edit_widgets" => self.ai_apply_widget_action(ctx, parse(call)?, request),
            _ => Err("未知编辑器工具".into()),
        }
    }

    fn ai_editor_page(&self, page_id: Option<u64>) -> Result<usize, String> {
        match page_id {
            Some(id) => self
                .project
                .pages
                .iter()
                .position(|page| page.id == id)
                .ok_or_else(|| "指定页面不存在".into()),
            None => Ok(self.page),
        }
    }

    fn ai_set_editor_view(&mut self, ctx: &egui::Context, args: SetView) -> Result<Value, String> {
        if args
            .zoom
            .is_some_and(|zoom| !(viewport::MIN_ZOOM..=viewport::MAX_ZOOM).contains(&zoom))
        {
            return Err("缩放比例必须在 0.1 至 10 之间".into());
        }
        if args.zoom.is_some() && args.focus.is_some() {
            return Err("zoom 和 focus 请选择一项".into());
        }
        let index = self.ai_editor_page(args.page_id)?;
        let page = &self.project.pages[index];
        let selection = args
            .widget_ids
            .map(|ids| widget_ids(page, ids))
            .transpose()?;
        self.ai_show_page(page.id)?;
        if let Some(selection) = selection {
            self.selected = selection;
        }
        if let Some(grid) = args.grid {
            self.grid = grid;
        }
        if let Some(snap) = args.snap {
            self.snap = snap;
        }
        if let Some(zoom) = args.zoom {
            self.view.pending = Some(ViewCommand::Zoom(zoom));
            self.fit = false;
        }
        if let Some(focus) = args.focus {
            self.fit = matches!(focus, ViewFocus::FitPage);
            self.view.pending = match focus {
                ViewFocus::FitPage => None,
                ViewFocus::CenterPage => Some(ViewCommand::CenterPage),
                ViewFocus::FitSelection => Some(ViewCommand::FitSelection),
                ViewFocus::CenterSelection => Some(ViewCommand::CenterSelection),
                ViewFocus::Origin => Some(ViewCommand::Origin),
            };
        }
        ctx.request_repaint();
        Ok(
            json!({"page_id":self.project.pages[self.page].id,"widget_ids":self.selected,"document_changed":false}),
        )
    }

    fn ai_apply_widget_action(
        &mut self,
        ctx: &egui::Context,
        args: EditWidgets,
        request: Option<&automation::ToolRequest>,
    ) -> Result<Value, String> {
        let index = self.ai_editor_page(args.page_id)?;
        let page = &self.project.pages[index];
        if args.target_page_id.is_some() && !matches!(args.action, WidgetAction::Duplicate) {
            return Err("只有 duplicate 可指定 target_page_id".into());
        }
        let target = self.ai_editor_page(args.target_page_id.or(Some(page.id)))?;
        let page_id = self.project.pages[target].id;
        let ids = widget_ids(page, args.widget_ids)?;
        if ids.is_empty() {
            return Err("至少需要一个组件 ID".into());
        }
        let mut candidate = self.project.clone();
        let mut duplicates = BTreeSet::new();
        let native = self.native.as_ref();
        match args.action {
            WidgetAction::Duplicate => {
                let widgets = page
                    .widgets
                    .iter()
                    .filter(|widget| ids.contains(&widget.id))
                    .cloned()
                    .collect();
                let clipboard = editing::ComponentClipboard::from_selection(page, widgets);
                (duplicates, _) =
                    editing::paste_widgets(&mut candidate, target, clipboard, native.is_some())
                        .map_err(|error| format!("{error:#}"))?;
            }
            WidgetAction::FlipHorizontal | WidgetAction::FlipVertical => {
                editing::flip_widgets(
                    &mut candidate.pages[index],
                    &ids,
                    native,
                    matches!(args.action, WidgetAction::FlipHorizontal),
                )
                .map_err(|error| format!("{error:#}"))?;
            }
            action => {
                let mode = match action {
                    WidgetAction::AlignLeft => 0,
                    WidgetAction::AlignCenter => 1,
                    WidgetAction::AlignRight => 2,
                    WidgetAction::AlignTop => 3,
                    WidgetAction::AlignMiddle => 4,
                    WidgetAction::AlignBottom => 5,
                    WidgetAction::DistributeHorizontal => 6,
                    WidgetAction::DistributeVertical => 7,
                    WidgetAction::RotateCounterclockwise => 12,
                    WidgetAction::RotateClockwise => 13,
                    _ => unreachable!(),
                };
                arrange::arrange_widgets(&mut candidate.pages[index], &ids, native, mode);
            }
        }
        let changed = candidate != self.project;
        if changed {
            candidate.validate().map_err(|error| format!("{error:#}"))?;
            self.ai_validate_native_candidate(&mut candidate)?;
            self.ai_tool_active(request)?;
            let before = std::mem::replace(&mut self.project, candidate);
            self.ai_show_page(page_id)?;
            if !duplicates.is_empty() {
                self.selected = duplicates.clone();
            }
            self.textures.clear();
            self.remember(before);
            self.status = "AI 已修改当前工程 · Ctrl+Z 可撤销".into();
            ctx.request_repaint();
        }
        Ok(json!({"changed":changed,"page_id":page_id,"created_widget_ids":duplicates}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::fixture;

    #[test]
    fn editor_tools_share_document_history_and_keep_view_edits_out_of_it() {
        let ctx = egui::Context::default();
        let mut project = Project::blank();
        let a = project.allocate_id();
        let b = project.allocate_id();
        let target = project.allocate_id();
        let mut second_page = project.pages[0].clone();
        second_page.id = target;
        second_page.name = "目标页".into();
        project.pages.push(second_page);
        project.pages[0].widgets.extend([
            Widget::new(a, WidgetKind::Rectangle, 20.0, 40.0),
            Widget::new(b, WidgetKind::Rectangle, 100.0, 100.0),
        ]);
        let mut app = fixture(project.clone());
        let mut tool = |name: &str, arguments: Value| {
            app.execute_ai_tool(
                &ctx,
                &automation::ToolCall {
                    name: name.into(),
                    arguments,
                },
            )
        };
        assert_eq!(
            tool("rpshop_set_view", json!({"widget_ids":[a,b],"grid":true})).unwrap()["document_changed"],
            false
        );
        assert_eq!(
            tool("rpshop_read_view", json!({})).unwrap()["widget_ids"],
            json!([a, b])
        );
        assert_eq!(
            tool("rpshop_read_view", json!({})).unwrap()["can_undo"],
            false
        );
        assert!(
            tool(
                "rpshop_edit_widgets",
                json!({"action":"align_left","widget_ids":[a,9999]})
            )
            .is_err()
        );
        assert_eq!(
            tool(
                "rpshop_edit_widgets",
                json!({"action":"align_left","widget_ids":[a,b]})
            )
            .unwrap()["changed"],
            true
        );
        let result = tool(
            "rpshop_edit_widgets",
            json!({"action":"duplicate","widget_ids":[a],"target_page_id":target}),
        )
        .unwrap();
        assert_eq!(result["page_id"], target);
        let duplicate = result["created_widget_ids"][0].as_u64().unwrap();
        assert_ne!(duplicate, a);
        assert_eq!(
            tool("rpshop_history", json!({"action":"undo"})).unwrap()["can_redo"],
            true
        );
        assert_eq!(
            tool("rpshop_history", json!({"action":"undo"})).unwrap()["can_undo"],
            false
        );
        assert_eq!(app.project, project);
        assert_eq!(
            app.execute_ai_tool(
                &ctx,
                &automation::ToolCall {
                    name: "rpshop_history".into(),
                    arguments: json!({"action":"redo"})
                }
            )
            .unwrap()["changed"],
            true
        );
        assert_eq!(
            app.project.pages[0].widgets[1].x,
            app.project.pages[0].widgets[0].x
        );
    }

    #[test]
    fn editor_open_tool_uses_the_unsaved_change_prompt() {
        let mut app = fixture(Project::blank());
        app.project.name = "有修改的工程".into();
        let before = app.project.clone();
        let ctx = egui::Context::default();
        let call = |arguments| automation::ToolCall {
            name: "rpshop_open_project".into(),
            arguments,
        };
        assert!(
            app.execute_ai_tool(&ctx, &call(json!({"path":"unknown"})))
                .is_err()
        );
        assert!(app.confirm.is_none());
        assert_eq!(
            app.execute_ai_tool(&ctx, &call(json!({"source":"demo"})))
                .unwrap()["open_requested"],
            true
        );
        assert_eq!(app.confirm.as_deref(), Some("demo"));
        assert_eq!(app.project, before);
    }
}
