use super::*;
use std::path::Path;

impl App {
    pub(super) fn ai_local_engine_picker(&mut self, ui: &mut egui::Ui) {
        if ui.button("添加自定义引擎…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title("选择可信的本机引擎 · 将以你的账户权限运行")
                .pick_file()
        {
            match self.ai_add_local_engine(&path) {
                Ok(()) => self.ai.diagnosis.clear(),
                Err(error) => self.ai.diagnosis = error,
            }
        }
        dialog_style::note(
            ui,
            "仅选择可信来源。引擎会收到当前需求和所附参考，工程工具仍由 Ingenaro 审批。",
        );
    }

    pub(in crate::ui::ai_workbench) fn ai_add_local_engine(
        &mut self,
        path: &Path,
    ) -> Result<(), String> {
        let path = path.canonicalize().map_err(|_| "无法打开引擎文件")?;
        let mut engine = ai::EngineConfig::new(ai::EngineKind::Plugin);
        engine.name = path
            .file_stem()
            .map(|name| name.to_string_lossy().chars().take(64).collect())
            .unwrap_or_else(|| "自定义引擎".into());
        engine.executable = Some(path);
        let diagnostics = engine.diagnostics()?;
        if !diagnostics.available {
            return Err(diagnostics.action_hint);
        }
        let mut settings = self.ai.settings.clone();
        let id = if let Some(existing) = settings.engines.iter().find(|existing| {
            existing.kind == ai::EngineKind::Plugin && existing.executable == engine.executable
        }) {
            existing.id.clone()
        } else {
            let id = engine.id.clone();
            settings.engines.push(engine);
            id
        };
        settings.default_engine_id.get_or_insert_with(|| id.clone());
        self.ai_commit_engine_settings(settings)?;
        self.ai_select_local_engine(id);
        Ok(())
    }

    fn ai_use_local_engine_settings(
        &mut self,
        mut settings: ai::AiSettings,
        id: String,
    ) -> Result<(), String> {
        settings.default_engine_id = (!id.is_empty()).then(|| id.clone());
        self.ai_commit_engine_settings(settings)?;
        self.ai_select_local_engine(id);
        Ok(())
    }

    fn ai_select_local_engine(&mut self, id: String) {
        self.ai.settings_engine = id.clone();
        self.ai_choose_local(id);
    }

    fn ai_commit_engine_settings(&mut self, settings: ai::AiSettings) -> Result<(), String> {
        if self.ai.persist {
            settings.save_default()?;
        } else {
            settings.validate()?;
        }
        self.ai.settings = settings;
        Ok(())
    }

    pub(super) fn ai_local_engine_settings(
        &mut self,
        ui: &mut egui::Ui,
        engine: &ai::EngineConfig,
    ) {
        ui.heading(&engine.name);
        ui.label(format!("自定义引擎 · 协议 v{}", ai::AI_PLUGIN_VERSION));
        if let Some(path) = &engine.executable {
            ui.label(path.display().to_string());
        }
        dialog_style::note(ui, "模型与认证由引擎管理。发送消息时启动并验证协议。");
        ui.horizontal(|ui| {
            if ui
                .add(dialog_style::primary("设为默认并使用", PURPLE))
                .clicked()
            {
                match self.ai_use_local_engine_settings(self.ai.settings.clone(), engine.id.clone())
                {
                    Ok(()) => self.ai.settings_open = false,
                    Err(error) => self.ai.diagnosis = error,
                }
            }
            if ui.button("移除引擎配置").clicked() {
                let mut settings = self.ai.settings.clone();
                settings.remove_engine(&engine.id);
                let result = if self.ai.selected_engine == engine.id {
                    let id = settings
                        .default_engine_id
                        .clone()
                        .filter(|id| {
                            settings.engines.iter().any(|engine| {
                                &engine.id == id && ai::is_desktop_engine(engine.kind)
                            })
                        })
                        .or_else(|| {
                            settings
                                .engines
                                .iter()
                                .find(|engine| ai::is_desktop_engine(engine.kind))
                                .map(|engine| engine.id.clone())
                        })
                        .unwrap_or_default();
                    self.ai_use_local_engine_settings(settings, id)
                } else {
                    self.ai_commit_engine_settings(settings)
                };
                match result {
                    Ok(()) => self.ai.diagnosis.clear(),
                    Err(error) => self.ai.diagnosis = error,
                }
            }
        });
    }
}
