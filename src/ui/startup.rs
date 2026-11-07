use super::*;

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>) -> Self {
        #[cfg(target_os = "windows")]
        window_chrome::configure_window(cc);
        setup_style(&cc.egui_ctx);
        let restored: Option<Session> = if screenshot.is_none() {
            cc.storage
                .and_then(|s| eframe::get_value::<Session>(s, "rpshop-session"))
                .map(|mut s| {
                    normalize_draft(&mut s.project);
                    s
                })
                .filter(|s: &Session| s.project.validate().is_ok() && s.saved.validate().is_ok())
        } else {
            None
        };
        let ai_document_id = restored
            .as_ref()
            .map(|s| s.ai_document_id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let native_state = restored.as_ref().and_then(|s| s.native_state.clone());
        let sidebar_layout = if screenshot.is_none() {
            cc.storage
                .and_then(|s| eframe::get_value(s, sidebar::STORAGE_KEY))
                .unwrap_or_default()
        } else {
            sidebar::SidebarLayout::default()
        };
        let (mut project, mut saved, mut path, mut import_warnings) = restored.map_or_else(
            || {
                let p = Project::default();
                (p.clone(), p, None, Vec::new())
            },
            |s| (s.project, s.saved, s.path, s.import_warnings),
        );
        let args: Vec<String> = std::env::args().collect();
        // Screenshot QA is isolated. Normal shell paths go through the inbox
        // after restoration, so a recovered dirty draft receives confirmation.
        let startup_file = screenshot.as_ref().and_then(|_| {
            args.windows(2)
                .find(|w| w[0] == "--open")
                .map(|w| w[1].clone())
                .or_else(|| {
                    if screenshot.is_none() {
                        args.get(1).filter(|a| !a.starts_with('-')).cloned()
                    } else {
                        None
                    }
                })
        });
        let mut startup_status = "准备就绪".to_owned();
        let mut native = None;
        if let Some(path) = &path
            && is_native_path(path)
        {
            match rpshop::rp_format::open(path) {
                Ok(mut document) => {
                    let fresh = document.project.clone();
                    match restore_native_context(
                        &mut document,
                        native_state.as_ref(),
                        &mut project,
                        &mut saved,
                    ) {
                        Ok(migrated) => {
                            if migrated {
                                startup_status = "已恢复原生草稿、页面目录和显示设置".into();
                            }
                            native = Some(document);
                        }
                        Err(_) if project == saved => {
                            project = fresh.clone();
                            saved = fresh;
                            import_warnings = document.warnings.clone();
                            native = Some(document);
                            startup_status = "已重新打开原生文件并恢复页面目录".into();
                        }
                        Err(error) => {
                            startup_status = format!(
                                "原生保存上下文未恢复（{error:#}），当前草稿已保留；请另存或重新打开源文件"
                            );
                        }
                    }
                }
                Err(e) => {
                    startup_status = format!("无法恢复 原生保存上下文：{e:#}；可另存当前会话")
                }
            }
        }
        if let Some(file) = startup_file {
            match load_document(std::path::Path::new(&file)) {
                Ok(document) => {
                    project = document.project.clone();
                    saved = document.project;
                    path = document.save_path;
                    import_warnings = document.warnings;
                    native = document.native;
                    startup_status = format!("已打开：{file}");
                }
                Err(e) => startup_status = format!("打开失败：{e:#}"),
            }
        }
        let interaction_example = screenshot.is_some()
            && path.is_none()
            && native.is_none()
            && args.iter().any(|arg| arg == "--interaction-example");
        if interaction_example && let Some(widget) = project.pages[0].widgets.first() {
            let mut event = rpshop::model::AuthoredEvent::new(
                project.pages[0].id,
                None,
                rpshop::model::EventTrigger::PageLoad,
            );
            event.name = "页面载入".into();
            event.actions.push(rpshop::model::Interaction {
                action: Action::SetText,
                target_page: None,
                target_widget: Some(widget.id),
                value: "欢迎使用 RPShop".into(),
            });
            project.interaction_events.push(event);
            saved = project.clone();
        }
        let tab = args
            .windows(2)
            .find(|pair| pair[0] == "--inspector-tab")
            .and_then(|pair| match pair[1].as_str() {
                "style" => Some(0),
                "interaction" => Some(1),
                "notes" => Some(2),
                _ => None,
            })
            .unwrap_or(if interaction_example { 1 } else { 0 });
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let logo = Some(crate::branding::title_texture(&cc.egui_ctx));
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let logo = image::load_from_memory(include_bytes!("../../assets/branding/rpshop-icon.png"))
            .ok()
            .map(|img| {
                // The header displays this at 38 logical pixels. Keep a prefiltered
                // 2x texture so the GPU does not have to minify the full artwork.
                let img = img
                    .resize_exact(76, 76, image::imageops::FilterType::Lanczos3)
                    .into_rgba8();
                cc.egui_ctx.load_texture(
                    "rpshop-header-icon",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [img.width() as usize, img.height() as usize],
                        &img,
                    ),
                    egui::TextureOptions::LINEAR,
                )
            });
        let screenshot_mode = screenshot.is_some();
        let startup_selection = args
            .windows(2)
            .find(|w| w[0] == "--select")
            .and_then(|w| w[1].parse::<u64>().ok());
        let page = startup_selection
            .and_then(|id| {
                project
                    .pages
                    .iter()
                    .position(|p| p.widgets.iter().any(|w| w.id == id))
            })
            .or_else(|| {
                args.windows(2)
                    .find(|pair| pair[0] == "--page-name")
                    .and_then(|pair| project.pages.iter().position(|page| page.name == pair[1]))
            })
            .unwrap_or(0);
        let selected = startup_selection
            .filter(|id| project.pages[page].widgets.iter().any(|w| w.id == *id))
            .into_iter()
            .collect();
        let mut ai = ai_workbench::Workbench::new(!screenshot_mode && !cfg!(test), ai_document_id);
        if args
            .iter()
            .any(|arg| arg == "--ai-design" || arg == "--ai-example")
        {
            ai.open_startup(args.iter().any(|arg| arg == "--ai-example"));
        }
        let qa_kind = screenshot_mode
            .then(|| std::env::var("RPSHOP_QA_DIALOG").ok())
            .flatten();
        let mut app = Self {
            ai,
            ai_reveal: Default::default(),
            project,
            saved,
            page,
            selected,
            path,
            undo: vec![],
            redo: vec![],
            clipboard: Default::default(),
            expanded_layer_groups: BTreeSet::new(),
            layer_name_dialog: None,
            zoom: 0.65,
            view: CanvasView::default(),
            sidebar_layout,
            fit: true,
            grid: native.is_none(),
            snap: true,
            tab,
            tool: None,
            drag: None,
            pending_widget: None,
            library_query: String::new(),
            library_category: None,
            browser_preview: None,
            textures: HashMap::new(),
            logo,
            status: startup_status,
            last_recovery: Instant::now(),
            screenshot,
            screenshot_mode,
            history_changed: false,
            editing: None,
            frames: 0,
            confirm: None,
            help: false,
            import_warnings,
            pending_open: None,
            native,
            cached_native_preview_source: None,
        };
        if let Some(kind) = qa_kind {
            app.ai_seed_qa(&kind);
        }
        app
    }
}
