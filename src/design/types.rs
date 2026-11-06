use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::model::Project;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Native,
    Html,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "原生原型",
            Self::Html => "HTML 页面",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ArtifactContent {
    Native(Project),
    Html { source: String },
}

impl ArtifactContent {
    pub fn kind(&self) -> ArtifactKind {
        match self {
            Self::Native(_) => ArtifactKind::Native,
            Self::Html { .. } => ArtifactKind::Html,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Native(project) => {
                project.validate()?;
                // Native image/text data can be large even after model validation.
                super::store::bounded_json(self, 8 * 1024 * 1024)?;
                Ok(())
            }
            Self::Html { source } => super::validate_html(source),
        }
    }
}

/// Stable editor document identity + provider/engine identity; never a display name.
/// For unsaved documents the UI should persist a fresh identity with its session.
/// Switching engines or projects must switch scopes, not rename an existing scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub project_key: String,
    pub engine_key: String,
}

impl Scope {
    pub fn new(project_key: impl Into<String>, engine_key: impl Into<String>) -> Result<Self> {
        let scope = Self {
            project_key: project_key.into(),
            engine_key: engine_key.into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub(super) fn validate(&self) -> Result<()> {
        text(&self.project_key, 2048, "项目标识")?;
        text(&self.engine_key, 256, "引擎标识")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRecord {
    pub id: u64,
    pub role: ChatRole,
    pub text: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionOrigin {
    Generated,
    BuiltinSample,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub id: u64,
    pub title: String,
    pub created_at_ms: u64,
    pub origin: RevisionOrigin,
    pub content: ArtifactContent,
}

impl Revision {
    pub fn label(&self) -> &str {
        if self.origin == RevisionOrigin::BuiltinSample {
            super::BUILTIN_SAMPLE_LABEL
        } else {
            &self.title
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: u64,
    pub scope: Scope,
    pub title: String,
    pub kind: ArtifactKind,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub messages: Vec<ChatRecord>,
    pub revisions: Vec<Revision>,
}

pub(super) fn text(value: &str, max: usize, label: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= max && !value.contains('\0'),
        "{label}不能为空、包含空字符或超过 {max} 字节"
    );
    Ok(())
}
