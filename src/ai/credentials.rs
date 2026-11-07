use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Separate OS vault namespace; no CLI OAuth files or provider login tokens are read.
pub struct CredentialStore;
impl CredentialStore {
    fn entry(engine_id: &str) -> Result<keyring::Entry, String> {
        if engine_id.is_empty() || engine_id.len() > 128 {
            return Err("Invalid credential identity".into());
        }
        keyring::Entry::new("Ingenaro.AI", engine_id)
            .map_err(|_| "OS credential store is unavailable".into())
    }
    pub fn get(engine_id: &str) -> Result<Option<String>, String> {
        match Self::entry(engine_id)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Cannot read OS credential store; use a session-only key".into()),
        }
    }
    pub fn set(engine_id: &str, key: &str) -> Result<(), String> {
        if key.is_empty() || key.len() > 16384 {
            return Err("Invalid API key".into());
        }
        Self::entry(engine_id)?.set_password(key).map_err(|_| {
            "Cannot save API key to OS credential store; use a session-only key".into()
        })
    }
    pub fn delete(engine_id: &str) -> Result<(), String> {
        match Self::entry(engine_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Cannot delete OS credential".into()),
        }
    }
}

/// Memory-only keys are intentionally neither Serialize nor Debug.
#[derive(Clone, Default)]
pub struct SessionKeys(Arc<Mutex<HashMap<String, String>>>);
impl SessionKeys {
    pub fn set(&self, engine_id: &str, key: String) {
        if let Ok(mut keys) = self.0.lock() {
            keys.insert(engine_id.into(), key);
        }
    }
    pub fn get(&self, engine_id: &str) -> Option<String> {
        self.0.lock().ok()?.get(engine_id).cloned()
    }
    pub fn remove(&self, engine_id: &str) {
        if let Ok(mut keys) = self.0.lock() {
            keys.remove(engine_id);
        }
    }
    pub fn clear(&self) {
        if let Ok(mut keys) = self.0.lock() {
            keys.clear();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_keys_are_shared_but_not_persisted() {
        let keys = SessionKeys::default();
        keys.set("a", "test-secret".into());
        assert_eq!(keys.clone().get("a").as_deref(), Some("test-secret"));
        keys.clear();
        assert_eq!(keys.get("a"), None);
    }
}
