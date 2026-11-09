use super::Diagnostics;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineKind {
    Codex,
    Claude,
    OpenAiCompatible,
    Plugin,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPreference {
    #[default]
    Engine,
    Auto,
    Level(ReasoningEffort),
}

impl ReasoningPreference {
    pub fn resolve(self, default: Option<ReasoningEffort>) -> Option<ReasoningEffort> {
        match self {
            Self::Engine => default,
            Self::Auto => None,
            Self::Level(effort) => Some(effort),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    #[default]
    Ask,
    Full,
    ReadOnly,
}

impl PermissionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "逐次确认",
            Self::Full => "完全授权",
            Self::ReadOnly => "只读讨论",
        }
    }
}

/// Composer choices alone. A remembered mode never carries a live tool grant.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComposerPreferences {
    pub model: Option<String>,
    pub effort: ReasoningPreference,
    pub permission: PermissionMode,
}

impl ComposerPreferences {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.model.as_ref().is_some_and(|model| model.len() > 256) {
            return Err("Invalid AI composer model");
        }
        Ok(())
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
                EngineKind::Plugin => "自定义引擎",
            }
            .into(),
            kind,
            model: String::new(),
            executable: None,
            api_base: (kind == EngineKind::OpenAiCompatible)
                .then(|| "https://api.openai.com/v1".into()),
            timeout_seconds: if matches!(kind, EngineKind::OpenAiCompatible | EngineKind::Plugin) {
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
        if matches!(self.kind, EngineKind::OpenAiCompatible | EngineKind::Plugin)
            && !(5..=3600).contains(&self.timeout_seconds)
        {
            return Err("Timeout must be between 5 and 3600 seconds".into());
        }
        if self
            .max_budget_usd
            .is_some_and(|n| !n.is_finite() || n <= 0.0 || n > 10000.0)
        {
            return Err("Budget must be a finite positive amount below 10000 USD".into());
        }
        if self.kind == EngineKind::Plugin
            && !self
                .executable
                .as_ref()
                .is_some_and(|path| path.is_absolute())
        {
            return Err("自定义引擎需要本机可执行文件的绝对路径".into());
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
