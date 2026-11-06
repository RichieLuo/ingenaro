use crate::{branding, ui};

pub fn run() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if crate::cli::run(&args) {
        return Ok(());
    }
    let screenshot = args
        .windows(2)
        .find(|w| w[0] == "--screenshot")
        .map(|w| std::path::PathBuf::from(&w[1]));
    let instance = if screenshot.is_some() {
        None
    } else {
        let arguments = shell_arguments(&std::env::args_os().skip(1).collect::<Vec<_>>())
            .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?;
        match crate::shell_open::start("RPShop", &arguments)
            .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?
        {
            Some(instance) => Some(instance),
            None => return Ok(()),
        }
    };
    let attachment = instance
        .as_ref()
        .map(crate::shell_open::Instance::attachment);
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(if screenshot.is_some() {
            "RPShop · 界面预览（隔离）"
        } else {
            "RPShop · 原型工作室"
        })
        .with_decorations(!cfg!(target_os = "windows"))
        .with_inner_size([1500.0, 950.0])
        .with_min_inner_size([1100.0, 720.0]);
    viewport = viewport.with_icon(branding::icon_data());
    let options = eframe::NativeOptions {
        viewport,
        persist_window: screenshot.is_none(),
        persistence_path: screenshot
            .as_ref()
            .map(|_| std::env::temp_dir().join(format!("rpshop-qa-{}.ron", std::process::id()))),
        ..Default::default()
    };
    let result = eframe::run_native(
        "RPShop",
        options,
        Box::new(move |cc| {
            if let Some(attachment) = attachment {
                attachment.attach_context(&cc.egui_ctx);
            }
            Ok(Box::new(ui::App::new(cc, screenshot)))
        }),
    );
    drop(instance);
    result
}

fn shell_arguments(arguments: &[std::ffi::OsString]) -> std::io::Result<Vec<std::ffi::OsString>> {
    let mut shell = Vec::new();
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            shell.push(argument.clone());
            shell.extend(arguments.cloned());
            break;
        }
        if argument == "--open" {
            shell.push(argument.clone());
            if let Some(path) = arguments.next() {
                shell.push(path.clone());
            }
        } else if ["--page-name", "--select", "--inspector-tab"]
            .iter()
            .any(|option| argument == *option)
        {
            arguments.next().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "GUI option requires a value",
                )
            })?;
        } else if ![
            "--interaction-example",
            "--ai-design",
            "--ai-example",
            "--qa-interactive",
        ]
        .iter()
        .any(|flag| argument == *flag)
        {
            shell.push(argument.clone());
        }
    }
    Ok(shell)
}
