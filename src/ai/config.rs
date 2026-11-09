use super::{ComposerPreferences, EngineConfig};
use crate::design::palette::Palette;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiSettings {
    pub engines: Vec<EngineConfig>,
    pub default_engine_id: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub composer_preferences: BTreeMap<String, ComposerPreferences>,
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
        ProjectDirs::from("", "Ingenaro", "Ingenaro")
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
            || self.composer_preferences.keys().any(|id| !ids.contains(id))
        {
            return Err("AI engine binding references a missing engine".into());
        }
        for preferences in self.composer_preferences.values() {
            preferences.validate()?;
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
        self.composer_preferences.remove(id);
        self.project_engines.retain(|_, value| value != id);
        if self.default_engine_id.as_deref() == Some(id) {
            self.default_engine_id = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{EngineKind, PermissionMode, ReasoningPreference};
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
        let engine = EngineConfig::default();
        let settings = AiSettings {
            composer_preferences: BTreeMap::from([(
                engine.id.clone(),
                ComposerPreferences {
                    model: Some(String::new()),
                    effort: ReasoningPreference::Auto,
                    permission: PermissionMode::ReadOnly,
                },
            )]),
            engines: vec![engine],
            ..Default::default()
        };
        settings.save(&path).unwrap();
        settings.save(&path).unwrap();
        assert_eq!(settings, AiSettings::load(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
