use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) const STORAGE_KEY: &str = "ingenaro-installed-plugins";

#[derive(Clone, Copy)]
pub(super) enum Plugin {
    AiTool,
}

impl Plugin {
    const CATALOG: [Self; 1] = [Self::AiTool];

    fn id(self) -> &'static str {
        match self {
            Self::AiTool => "ai_tool",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::AiTool => "AI 工具",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::AiTool => "用文字和参考资料设计原型，并在画布上继续编辑。",
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Plugins {
    installed: BTreeSet<String>,
    #[serde(skip)]
    pub(super) open: bool,
}

impl Plugins {
    pub(super) fn from_legacy(ai_configured: bool) -> Self {
        let mut plugins = Self::default();
        plugins.set_installed(Plugin::AiTool, ai_configured);
        plugins
    }

    pub(super) fn contains(&self, plugin: Plugin) -> bool {
        self.installed.contains(plugin.id())
    }

    fn set_installed(&mut self, plugin: Plugin, installed: bool) -> bool {
        if installed {
            self.installed.insert(plugin.id().into())
        } else {
            self.installed.remove(plugin.id())
        }
    }

    fn inventory(&self) -> Value {
        json!({"plugins": Plugin::CATALOG.map(|plugin| json!({
            "id": plugin.id(),
            "name": plugin.name(),
            "description": plugin.description(),
            "source": "bundled",
            "installed": self.contains(plugin)
        }))})
    }
}

impl App {
    fn set_plugin_installed(&mut self, plugin: Plugin, installed: bool) {
        if !self.plugins.set_installed(plugin, installed) {
            return;
        }
        match plugin {
            Plugin::AiTool if installed => self.ai_plugin_enable(),
            Plugin::AiTool => self.ai_plugin_disable(),
        }
        self.status = format!(
            "{}已{}",
            plugin.name(),
            if installed { "添加" } else { "移除" }
        );
    }

    pub(super) fn plugin_manager(&mut self, ctx: &egui::Context) {
        if !self.plugins.open {
            return;
        }
        let response = dialog_style::modal(ctx, "app-plugin-manager").show(ctx, |ui| {
            ui.set_width(460.0);
            let close = dialog_style::header(
                ui,
                dialog_style::Symbol::Settings,
                "添加插件",
                "选择要加入 Ingenaro 的工具。",
            );
            let mut selected = None;
            for plugin in Plugin::CATALOG {
                let installed = self.plugins.contains(plugin);
                dialog_style::section(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(304.0);
                            ui.label(RichText::new(plugin.name()).size(16.0).strong());
                            ui.label(plugin.description());
                            if installed {
                                ui.label(RichText::new("已添加").small().color(MUTED));
                            }
                        });
                        let action = if installed {
                            dialog_style::secondary("移除")
                        } else {
                            dialog_style::primary("添加", PURPLE)
                        };
                        if ui.add(action).clicked() {
                            selected = Some((plugin, !installed));
                        }
                    });
                });
            }
            (close, selected)
        });
        let (close, selected) = response.inner;
        if let Some((plugin, installed)) = selected {
            self.set_plugin_installed(plugin, installed);
            if installed {
                self.plugins.open = false;
            }
        }
        if close || response.should_close() {
            self.plugins.open = false;
        }
    }

    pub(super) fn plugin_tool(
        &mut self,
        call: &ingenaro::automation::ToolCall,
    ) -> Result<Value, String> {
        if !call
            .arguments
            .as_object()
            .is_some_and(|args| args.is_empty())
        {
            return Err("插件工具不接受参数".into());
        }
        if call.name == ingenaro::automation::LIST_PLUGINS_TOOL {
            return Ok(self.plugins.inventory());
        }
        self.plugins.open = true;
        Ok(json!({"opened":true,"install_requires_user":true}))
    }
}
