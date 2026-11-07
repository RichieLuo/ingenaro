//! Isolated screenshot fixtures only. They never run models, persist settings or edit user files.
use super::*;
use serde_json::json;

impl App {
    pub(in crate::ui) fn ai_seed_qa(&mut self, kind: &str) {
        if !self.screenshot_mode {
            return;
        }
        self.ai.open_qa_dialog(kind);
        self.ai.last_project_key = self.ai_project_key();
        let id = self.ai.selected_engine.clone();
        if kind == "modes" {
            // Interactive control previews use the real CLI catalog immediately,
            // not the single-row fixture used by offline screenshot scenarios.
            self.ai_request_models(&id, false);
            return;
        }
        let (tx, rx) = mpsc::channel();
        let catalog = ai::ModelCatalog {
            models: vec![ai::ModelInfo {
                id: "default".into(),
                name: "CLI 默认模型".into(),
                description: "内置界面验证数据，不代表已调用模型".into(),
                is_default: true,
                reasoning_efforts: vec![
                    "low".into(),
                    "medium".into(),
                    "high".into(),
                    "xhigh".into(),
                    "max".into(),
                ],
            }],
            source: "内置 UI 验证目录".into(),
            warning: None,
        };
        let _ = tx.send(Ok(catalog.clone()));
        self.ai
            .catalogs
            .insert(id, models::CatalogState::qa(catalog, rx));
        if kind == "conversation" {
            self.ai.messages = vec![
                response::Message::new(
                    design::ChatRole::User,
                    "把首页改成旅行规划工作台。保留侧边导航，重点突出精选路线和预约入口。",
                ),
                response::Message::new(
                    design::ChatRole::Assistant,
                    "## 首页调整已完成\n\n保留了原有导航结构，将首屏聚焦在**发现路线 → 查看详情 → 预约出行**。\n\n### 本轮修改\n\n- **精选路线**：统一卡片比例与留白，补齐行程和价格。\n- **预约入口**：主按钮使用品牌色，次要操作降低对比度。\n- **移动端**：卡片改为单列，保留清晰的阅读顺序。\n\n交互示例使用 `selectedRoute` 保存当前选择：\n\n```javascript\nconst selectRoute = (route) => {\n  selectedRoute = route.id;\n  openBookingPanel(route);\n};\n```\n\n> 所有画布修改都可以撤销；未覆盖已有工程文件。\n\n你可以继续选中路线卡片，让我调整内容或样式。",
                ),
            ];
            let mut response = response::Response::default();
            response.thinking_delta(
                "先核对当前页面的导航与卡片分组，再分区域调整信息层级；保留未要求修改的元件。",
                false,
            );
            response.tool_call("read".into(), "rpshop_read_pages".into());
            response.tool_result("read", true);
            response.tool_call("edit".into(), "rpshop_batch_edit".into());
            response.tool_result("edit", true);
            self.ai.messages[1].steps = response.steps;
            self.ai.state = "内置对话排版示例 · 不调用模型，不修改画布".into();
        }
        if kind == "conversation-tables" {
            self.ai.messages = vec![response::Message::new(
                design::ChatRole::Assistant,
                "建议优先补齐这些基础功能：\n\n\
                 | 模块 | 应包含的基础功能 |\n\
                 | --- | --- |\n\
                 | **基础资料** | 统一管理客户、供应商、商品分类和仓库资料，支持导入与批量编辑。 |\n\
                 | **系统管理** | 配置角色权限、审批规则和操作日志。 |\n\n\
                 以下是示例库存明细：\n\n\
                 | 商品编号 | 商品名称 | 所属仓库 | 可用库存 | 预警阈值 | 最近更新 |\n\
                 | --- | --- | --- | ---: | ---: | --- |\n\
                 | `SP-001` | 便携阅读灯 | 华东中心仓 | 128 | 30 | 11 月 6 日 |\n\
                 | `SP-002` | 多功能收纳盒 | 华南中心仓 | 26 | 40 | 11 月 6 日 |\n\n\
                 库存低于预警阈值时，可在工作台显示补货提醒。",
            )];
            self.ai.state = "内置表格排版示例 · 不调用模型，不修改画布".into();
        }
        if kind == "composer-attachments" {
            self.ai.images.push(
                ai::ImageAttachment::from_rgba(
                    "内置参考图",
                    64,
                    48,
                    &[80, 145, 210, 255].repeat(64 * 48),
                )
                .expect("QA pixels"),
            );
            self.ai.prompt = "根据参考图片调整选中元件，保留已有布局。".into();
            for index in 1..=8 {
                self.ai.prompt = format!("排队需求 {index} · 内置界面验证，不调用模型");
                if let Ok(entry) = self.ai_capture_submission() {
                    let _ = self.ai.queue.push(entry);
                }
            }
            self.ai.queue.paused = true;
            self.ai.prompt = "根据参考图片调整选中元件，保留已有布局。".into();
            self.ai.state = "内置附件与队列验证 · 已暂停，不调用模型".into();
        }
        if kind == "overview" {
            let updates = self
                .project
                .pages
                .iter()
                .map(|page| json!({"page_id":page.id,"status":"working","label":"内置总览示例 · 实时页面"}))
                .collect::<Vec<_>>();
            let _ = self.ai.page_activity.apply(
                &self.project,
                page_activity::Owner::Builtin,
                &json!({"updates":updates}),
            );
            if self.ai_page_overview_visible() {
                let _ = self.ai.page_activity.apply(
                    &self.project,
                    page_activity::Owner::Builtin,
                    &json!({"updates":[{"page_id":self.project.pages[0].id,"status":"completed"}]}),
                );
            }
            self.ai.state = "内置总览示例 · 未调用模型".into();
        }
    }
}
