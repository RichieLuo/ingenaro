use super::Diagnostics;
use crate::design::palette::Palette;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineKind {
    Codex,
    Claude,
    OpenAiCompatible,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EngineConfig {
    pub id: String,
    pub name: String,
    pub kind: EngineKind,
    pub model: String,
    pub executable: Option<PathBuf>,
    /// HTTP(S) API base ending in /v1, without credentials or query parameters.
    pub api_base: Option<String>,
    pub timeout_seconds: u64,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub max_budget_usd: Option<f64>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self::new(EngineKind::Codex)
    }
}
impl EngineConfig {
    pub fn new(kind: EngineKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: match kind {
                EngineKind::Codex => "Codex CLI",
                EngineKind::Claude => "Claude Code CLI",
                EngineKind::OpenAiCompatible => "OpenAI compatible",
            }
            .into(),
            kind,
            model: String::new(),
            executable: None,
            api_base: (kind == EngineKind::OpenAiCompatible)
                .then(|| "https://api.openai.com/v1".into()),
            timeout_seconds: if kind == EngineKind::OpenAiCompatible {
                300
            } else {
                0
            },
            reasoning_effort: None,
            max_budget_usd: None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.id.len() > 128
            || self.name.len() > 256
            || self.model.len() > 256
        {
            return Err("Invalid engine identity or model".into());
        }
        if self.kind == EngineKind::OpenAiCompatible && !(5..=3600).contains(&self.timeout_seconds)
        {
            return Err("Timeout must be between 5 and 3600 seconds".into());
        }
        if self
            .max_budget_usd
            .is_some_and(|n| !n.is_finite() || n <= 0.0 || n > 10000.0)
        {
            return Err("Budget must be a finite positive amount below 10000 USD".into());
        }
        if self.kind == EngineKind::OpenAiCompatible {
            let url = reqwest::Url::parse(self.api_base.as_deref().unwrap_or(""))
                .map_err(|_| "Invalid API base URL")?;
            if !matches!(url.scheme(), "https" | "http")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "API URL must be HTTP(S) without credentials, query or fragment".into(),
                );
            }
            if url.scheme() == "http"
                && !matches!(
                    url.host_str(),
                    Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
                )
            {
                return Err("Plain HTTP is permitted only on loopback".into());
            }
            if self.model.trim().is_empty() {
                return Err("Select an API model".into());
            }
        }
        Ok(())
    }
    pub fn diagnostics(&self) -> Result<Diagnostics, String> {
        self.validate()?;
        super::process::diagnostics(self)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiSettings {
    pub engines: Vec<EngineConfig>,
    pub default_engine_id: Option<String>,
    /// Stable project keys live in app config, never in the project payload.
    pub project_engines: BTreeMap<String, String>,
    /// Canonical paths map to stable app-owned document identities, including first save/rename.
    pub project_document_ids: BTreeMap<String, String>,
    /// Confirmed colors belong to the document, across chat and engine changes.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub project_palettes: BTreeMap<String, Palette>,
}
impl AiSettings {
    pub fn default_path() -> Result<PathBuf, String> {
        ProjectDirs::from("", "RPShop", "RPShop")
            .map(|d| d.config_dir().join("ai-settings.json"))
            .ok_or_else(|| "Application configuration directory is unavailable".into())
    }
    pub fn load_default() -> Result<Self, String> {
        Self::load(&Self::default_path()?)
    }
    pub fn save_default(&self) -> Result<(), String> {
        self.save(&Self::default_path()?)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("Cannot read AI configuration".into()),
        };
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read AI configuration")?;
        if bytes.len() > 1024 * 1024 {
            return Err("AI configuration is too large".into());
        }
        let settings: Self =
            serde_json::from_slice(&bytes).map_err(|_| "AI configuration is invalid")?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|_| "Cannot create configuration directory")?;
        let tmp = dir.join(format!(".ai-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|_| "Cannot write AI configuration")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o600))
                    .map_err(|_| "Cannot protect AI configuration")?;
            }
            let bytes =
                serde_json::to_vec_pretty(self).map_err(|_| "Cannot serialize AI configuration")?;
            if bytes.len() > 1024 * 1024 {
                return Err("AI configuration is too large");
            }
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Cannot write AI configuration")?;
            drop(file);
            // std::fs::rename uses replace-existing semantics on Windows too.
            // Never delete the old config before the atomic replacement succeeds.
            fs::rename(&tmp, path).map_err(|_| "Cannot replace AI configuration")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(tmp);
        }
        result.map_err(str::to_owned)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.engines.len() > 128
            || self.project_engines.len() > 10000
            || self.project_document_ids.len() > 10000
            || self.project_palettes.len() > 10000
        {
            return Err("AI configuration has too many entries".into());
        }
        if self
            .project_engines
            .keys()
            .any(|s| s.is_empty() || s.len() > 4096)
            || self.project_document_ids.iter().any(|(path, id)| {
                path.is_empty() || path.len() > 32768 || uuid::Uuid::parse_str(id).is_err()
            })
        {
            return Err("Invalid project identity mapping".into());
        }
        for (id, palette) in &self.project_palettes {
            if uuid::Uuid::parse_str(id).is_err() || palette.0.is_empty() {
                return Err("Invalid project palette binding".into());
            }
            palette.validate()?;
        }
        let mut ids = HashSet::new();
        for engine in &self.engines {
            engine.validate()?;
            if !ids.insert(&engine.id) {
                return Err("Duplicate engine ID".into());
            }
        }
        if self
            .default_engine_id
            .as_ref()
            .is_some_and(|id| !ids.contains(id))
            || self.project_engines.values().any(|id| !ids.contains(id))
        {
            return Err("AI engine binding references a missing engine".into());
        }
        Ok(())
    }
    pub fn engine_for_project(&self, project: &str) -> Option<&EngineConfig> {
        let id = self
            .project_engines
            .get(project)
            .or(self.default_engine_id.as_ref())?;
        self.engines.iter().find(|e| &e.id == id)
    }
    pub fn remove_engine(&mut self, id: &str) {
        self.engines.retain(|e| e.id != id);
        self.project_engines.retain(|_, value| value != id);
        if self.default_engine_id.as_deref() == Some(id) {
            self.default_engine_id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reasoning_efforts_roundtrip_with_cli_spellings() {
        for effort in [
            ReasoningEffort::Minimal,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::Xhigh,
            ReasoningEffort::Max,
        ] {
            let value = serde_json::to_value(effort).unwrap();
            assert_eq!(value.as_str(), Some(effort.as_str()));
            assert_eq!(
                serde_json::from_value::<ReasoningEffort>(value).unwrap(),
                effort
            );
        }
    }
    #[test]
    fn config_contains_no_credentials_and_bindings_stay_app_scoped() {
        let engine = EngineConfig::new(EngineKind::Codex);
        let mut settings = AiSettings {
            default_engine_id: Some(engine.id.clone()),
            ..Default::default()
        };
        settings
            .project_engines
            .insert("p1".into(), engine.id.clone());
        settings.engines.push(engine.clone());
        let text = serde_json::to_string(&settings).unwrap();
        assert!(!text.contains("token"));
        assert!(!text.contains("api_key"));
        assert_eq!(settings.engine_for_project("p1").unwrap().id, engine.id);
        settings.remove_engine(&engine.id);
        assert!(settings.validate().is_ok());
    }
    #[test]
    fn settings_roundtrip() {
        let path =
            std::env::temp_dir().join(format!("rpshop-ai-test-{}.json", uuid::Uuid::new_v4()));
        let settings = AiSettings {
            engines: vec![EngineConfig::default()],
            ..Default::default()
        };
        settings.save(&path).unwrap();
        settings.save(&path).unwrap();
        assert_eq!(settings, AiSettings::load(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_credentials_in_url_and_bad_timeout() {
        let mut e = EngineConfig::new(EngineKind::OpenAiCompatible);
        e.model = "test".into();
        e.api_base = Some("https://user:secret@example.com/v1".into());
        assert!(e.validate().is_err());
        e.api_base = Some("https://example.com/v1".into());
        e.timeout_seconds = 0;
        assert!(e.validate().is_err());
    }
}
