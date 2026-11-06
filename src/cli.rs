pub fn run(args: &[String]) -> bool {
    if args.get(1).is_some_and(|arg| arg == "--mcp-stdio") {
        let result = (|| -> Result<(), String> {
            let endpoint = args.get(2).ok_or("用法：rpshop --mcp-stdio http://127.0.0.1:PORT/mcp；密钥通过 RPSHOP_MCP_TOKEN 环境变量提供")?;
            let token = std::env::var("RPSHOP_MCP_TOKEN")
                .map_err(|_| "缺少 RPSHOP_MCP_TOKEN；请在 AI 工作台中启用当前工程 MCP")?;
            rpshop::automation::run_stdio(endpoint, &token)
        })();
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return true;
    }
    // A console host for inspecting the same player used by the desktop app.
    // It stays local and exits when stdin closes or receives a line.
    if args.get(1).is_some_and(|a| a == "--preview-server") {
        let result = (|| -> anyhow::Result<()> {
            let path = std::path::Path::new(args.get(2).ok_or_else(|| {
                anyhow::anyhow!("用法：rpshop --preview-server 工程路径 [--page-name 页面名]")
            })?);
            let (project, page_keys, native) = if path.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("rp") || extension.eq_ignore_ascii_case("rplib")
            }) {
                let document = rpshop::rp_format::open(path)?;
                let keys = document.preview_page_keys();
                let native = Some(std::sync::Arc::new(
                    document.preview_scene(&document.project)?,
                ));
                (document.project, keys, native)
            } else {
                (
                    rpshop::model::Project::load(path)?,
                    Default::default(),
                    None,
                )
            };
            let page_id = if let Some(pair) = args.windows(2).find(|pair| pair[0] == "--page-name")
            {
                project
                    .pages
                    .iter()
                    .find(|page| page.name == pair[1])
                    .ok_or_else(|| anyhow::anyhow!("没有找到指定页面"))?
                    .id
            } else {
                project.pages[0].id
            };
            let server =
                rpshop::preview::PreviewServer::start_with_native(project, page_keys, native)?;
            println!("{}", server.url(page_id));
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return true;
    }
    if args.get(1).is_some_and(|a| a == "--import") {
        let result = (|| -> anyhow::Result<()> {
            anyhow::ensure!(args.len() == 4, "用法：rpshop --import 输入.rp 输出.rpshop");
            let input = std::path::Path::new(&args[2]);
            let output = std::path::Path::new(&args[3]);
            anyhow::ensure!(
                output
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("rpshop")),
                "转换输出必须使用 .rpshop；直接编辑原生文件请使用打开和保存功能"
            );
            let imported = rpshop::rp_format::import(input)?;
            imported.project.save(output)?;
            println!(
                "{}",
                serde_json::json!({"pages":imported.project.pages.len(),"widgets":imported.project.pages.iter().map(|p|p.widgets.len()).sum::<usize>(),"warnings":imported.warnings,"output":output})
            );
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("{e:#}");
            std::process::exit(1);
        }
        return true;
    }
    if args.get(1).is_some_and(|a| a == "--export-demo") {
        let dir = std::path::PathBuf::from(args.get(2).map_or("examples", String::as_str));
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&dir)?;
            let project = rpshop::model::Project::default();
            project.save(&dir.join("welcome.rpshop"))?;
            std::fs::write(dir.join("welcome.html"), rpshop::export::html(&project)?)?;
            std::fs::write(
                dir.join("welcome.svg"),
                rpshop::export::svg(&project.pages[0])?,
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return true;
    }
    false
}
