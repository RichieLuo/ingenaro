//! Model discovery uses CLI control messages only: no user prompt or model turn.
#[cfg(test)]
#[path = "catalog/protocol_tests.rs"]
mod protocol_tests;
use super::{
    Diagnostics, EngineConfig, EngineKind,
    process::{Process, cli_command},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::mpsc::RecvTimeoutError,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCatalog {
    pub models: Vec<ModelInfo>,
    pub source: String,
    pub warning: Option<String>,
}
#[derive(Clone, Debug)]
pub struct LocalEngineInfo {
    pub config: EngineConfig,
    pub diagnostics: Diagnostics,
}

pub fn is_local(kind: EngineKind) -> bool {
    kind.supports_page_delegation() || kind == EngineKind::Antigravity
}
pub fn local_defaults() -> Vec<EngineConfig> {
    vec![
        EngineConfig::new(EngineKind::Claude),
        EngineConfig::new(EngineKind::Codex),
        EngineConfig::new(EngineKind::Antigravity),
    ]
}
pub fn discover_local_engines() -> Vec<LocalEngineInfo> {
    local_defaults()
        .into_iter()
        .map(|config| {
            let diagnostics = config.diagnostics().unwrap_or_else(|_| Diagnostics {
                available: false,
                version: None,
                login_hint: "未能读取工具状态".into(),
                action_hint: "检查 CLI 路径、安装和登录后重试".into(),
            });
            LocalEngineInfo {
                config,
                diagnostics,
            }
        })
        .collect()
}

pub fn discover_models(config: &EngineConfig) -> Result<ModelCatalog, String> {
    if !is_local(config.kind) {
        return Err("当前引擎不支持本机模型发现".into());
    }
    if config.kind == EngineKind::Antigravity {
        return super::antigravity::discover_models(config);
    }
    let directory = std::env::temp_dir().join(format!("ai-model-catalog-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).map_err(|_| "无法创建模型目录检查工作区")?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let mut command = cli_command(config)?;
    match config.kind {
        EngineKind::Codex => {
            command.args([
                "app-server",
                "--listen",
                "stdio://",
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.apps=false",
                "-c",
                "features.plugins=false",
                "-c",
                "features.hooks=false",
            ]);
        }
        EngineKind::Claude => {
            command.args([
                "--print",
                "--verbose",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--permission-prompt-tool",
                "stdio",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--disable-slash-commands",
                "--settings",
                "{\"disableAllHooks\":true}",
            ]);
        }
        _ => unreachable!(),
    }
    let process = Process::spawn(&mut command, &directory)?;
    let mut catalog = ModelCatalog {
        models: vec![],
        source: match config.kind {
            EngineKind::Claude => "Claude Code 实时模型目录",
            _ => "Codex app-server 实时模型目录",
        }
        .into(),
        warning: None,
    };
    let mut seen = HashSet::new();
    let mut cursors = HashSet::new();
    let mut pages = 0;
    match config.kind {
        EngineKind::Codex=>process.send(json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"ai-model-picker","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}}))?,
        _=>process.send(json!({"type":"control_request","request_id":"ai-runtime-models","request":{"subtype":"initialize","hooks":{}}}))?,
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() >= deadline {
            return Err("读取本地模型列表超时；检查工具登录和网络后重试".into());
        }
        let frame = match process.lines.recv_timeout(Duration::from_millis(50)) {
            Ok(frame) => frame?,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(_) => return Err("CLI 在返回模型目录前退出；请检查安装版本".into()),
        };
        match config.kind {
            EngineKind::Claude => {
                if frame["type"] != "control_response"
                    || frame["response"]["request_id"] != "ai-runtime-models"
                {
                    continue;
                }
                if frame["response"]["subtype"] == "error" {
                    return Err("Claude Code 初始化失败，未读取模型目录".into());
                }
                append_models(
                    &mut catalog,
                    &mut seen,
                    &frame["response"]["response"]["models"],
                    config.kind,
                )?;
                break;
            }
            _ => {
                if frame.get("error").is_some() {
                    return Err("Codex 拒绝模型目录请求，请检查版本与登录".into());
                }
                if frame["id"] == 1 && frame.get("result").is_some() {
                    process.send(json!({"method":"initialized","params":{}}))?;
                    process.send(json!({"id":2,"method":"model/list","params":{"limit":100,"includeHidden":false}}))?;
                } else if frame["id"] == 2 && frame.get("result").is_some() {
                    pages += 1;
                    if pages > 16 {
                        return Err("模型目录页数过多".into());
                    }
                    append_models(
                        &mut catalog,
                        &mut seen,
                        &frame["result"]["data"],
                        config.kind,
                    )?;
                    if let Some(cursor) = frame["result"]["nextCursor"]
                        .as_str()
                        .filter(|c| !c.is_empty())
                    {
                        if cursor.len() > 4096 || !cursors.insert(cursor.to_owned()) {
                            return Err("模型目录分页无效".into());
                        }
                        process.send(json!({"id":2,"method":"model/list","params":{"limit":100,"includeHidden":false,"cursor":cursor}}))?;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    if catalog.models.is_empty() {
        return Err("CLI 没有返回可选模型；仍可选择沿用 CLI 默认模型".into());
    }
    if !config.model.is_empty()
        && let Some(model) = catalog
            .models
            .iter_mut()
            .find(|model| model.id == config.model)
    {
        model.is_default = true;
    }
    Ok(catalog)
}

fn append_models(
    catalog: &mut ModelCatalog,
    seen: &mut HashSet<String>,
    value: &Value,
    kind: EngineKind,
) -> Result<(), String> {
    let rows = value.as_array().ok_or("CLI 返回的模型列表格式不受支持")?;
    if rows.len() > 512 || catalog.models.len() + rows.len() > 2048 {
        return Err("CLI 模型目录超过安全上限".into());
    }
    for row in rows {
        if row["hidden"].as_bool() == Some(true) {
            continue;
        }
        let id = if kind == EngineKind::Claude {
            row["value"].as_str()
        } else {
            row["model"].as_str().or_else(|| row["id"].as_str())
        };
        let Some(id) = id.map(str::trim).filter(|id| valid_text(id, 256)) else {
            continue;
        };
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let name = row["displayName"]
            .as_str()
            .filter(|name| valid_text(name, 256))
            .unwrap_or(id)
            .to_owned();
        let description = row["description"]
            .as_str()
            .filter(|text| valid_text(text, 2048))
            .unwrap_or("")
            .to_owned();
        let efforts = if kind == EngineKind::Claude {
            // Claude initialize advertises these separately from Codex's model/list.
            // A bare supportsEffort flag is not enough to invent model capabilities.
            (row["supportsEffort"].as_bool() != Some(false))
                .then(|| row["supportedEffortLevels"].as_array())
                .flatten()
        } else {
            row["supportedReasoningEfforts"].as_array()
        };
        let mut reasoning_efforts = Vec::new();
        for effort in efforts.into_iter().flatten() {
            let value = if kind == EngineKind::Claude {
                effort.as_str()
            } else {
                effort["reasoningEffort"].as_str()
            };
            if let Some(value) = value
                && matches!(
                    value,
                    "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                )
                && (kind != EngineKind::Claude || value != "minimal")
                && !reasoning_efforts.iter().any(|effort| effort == value)
            {
                reasoning_efforts.push(value.to_owned());
            }
        }
        catalog.models.push(ModelInfo {
            id: id.into(),
            name,
            description,
            is_default: row["isDefault"].as_bool().unwrap_or(false),
            reasoning_efforts,
        });
    }
    Ok(())
}
fn valid_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

/// Explicit fallback aliases supported by the CLI, not a claim of account access.
pub fn builtin_models(kind: EngineKind) -> ModelCatalog {
    let models = if kind == EngineKind::Claude {
        [
            ("opus", "Opus", "Claude Code 的 Opus 模型别名"),
            ("sonnet", "Sonnet", "Claude Code 的 Sonnet 模型别名"),
            ("haiku", "Haiku", "Claude Code 的 Haiku 模型别名"),
        ]
        .into_iter()
        .map(|(id, name, description)| ModelInfo {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            is_default: false,
            reasoning_efforts: vec![],
        })
        .collect()
    } else {
        vec![]
    };
    ModelCatalog {
        models,
        source: "CLI 模型别名（备用列表）".into(),
        warning: Some(
            "正在读取实际目录；备用别名不代表当前账户已开放。可始终选择沿用 CLI 默认。".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_current_cli_shapes_and_hides_duplicates() {
        let mut c = builtin_models(EngineKind::Codex);
        let mut seen = HashSet::new();
        append_models(&mut c,&mut seen,&json!([{"id":"visible","model":"wire-id","displayName":"Readable","isDefault":true},{"model":"secret","hidden":true},{"model":"wire-id"}]),EngineKind::Codex).unwrap();
        assert_eq!(c.models.len(), 1);
        assert_eq!(c.models[0].id, "wire-id");
        assert!(c.models[0].is_default);
        append_models(
            &mut c,
            &mut seen,
            &json!([{"value":"custom-model","displayName":"Custom"}]),
            EngineKind::Claude,
        )
        .unwrap();
        assert_eq!(c.models[1].name, "Custom");
    }
    #[test]
    fn efforts_come_only_from_each_cli_model_capability_metadata() {
        let mut catalog = builtin_models(EngineKind::Codex);
        let mut seen = HashSet::new();
        append_models(&mut catalog, &mut seen, &json!([
            {"value":"opus", "supportsEffort":true, "supportedEffortLevels":["low","medium","high","xhigh","max","max","unknown",null,"minimal"]},
            {"value":"haiku"},
            {"value":"legacy", "supportsEffort":true},
            {"value":"disabled", "supportsEffort":false, "supportedEffortLevels":["high"]}
        ]), EngineKind::Claude).unwrap();
        assert_eq!(
            catalog.models[0].reasoning_efforts,
            ["low", "medium", "high", "xhigh", "max"]
        );
        assert!(
            catalog.models[1..]
                .iter()
                .all(|model| model.reasoning_efforts.is_empty())
        );
        append_models(&mut catalog, &mut seen, &json!([
            {"model":"codex", "supportedReasoningEfforts":[{"reasoningEffort":"minimal"},{"reasoningEffort":"xhigh"},{"reasoningEffort":"xhigh"},{"reasoningEffort":"max"},{"reasoningEffort":"future"}]}
        ]), EngineKind::Codex).unwrap();
        assert_eq!(
            catalog.models.last().unwrap().reasoning_efforts,
            ["minimal", "xhigh", "max"]
        );
        assert!(
            builtin_models(EngineKind::Claude)
                .models
                .iter()
                .all(|model| model.reasoning_efforts.is_empty())
        );
    }
    #[test]
    fn malformed_catalog_never_surfaces_provider_bodies() {
        let mut c = builtin_models(EngineKind::Codex);
        assert!(
            append_models(
                &mut c,
                &mut HashSet::new(),
                &json!({"error":"private"}),
                EngineKind::Codex
            )
            .unwrap_err()
            .find("private")
            .is_none()
        );
    }
    #[test]
    fn bundled_local_engines_have_defaults() {
        let configs = local_defaults();
        assert_eq!(configs.len(), 3);
        assert!(configs.iter().all(|c| is_local(c.kind)));
    }
}
