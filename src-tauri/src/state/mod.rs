//! App-wide state and JSON config persistence (Rust-owned, app_shell.md).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::audio::pipeline::PipelineHandle;
use crate::dsp::{builtin_presets, DspParams, DspPreset};
use crate::soundboard::ClipStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub input_device_id: Option<String>,
    pub output_device_id: Option<String>,
    pub monitor_device_id: Option<String>,
    pub monitor_enabled: bool,
    pub active_preset_id: String,
    pub user_presets: Vec<DspPreset>,
    pub onboarding_done: bool,
    /// Unknown fields are preserved on rewrite (forward compatibility).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            input_device_id: None,
            output_device_id: None,
            monitor_device_id: None,
            monitor_enabled: false,
            active_preset_id: "none".into(),
            user_presets: Vec::new(),
            onboarding_done: false,
            extra: serde_json::Map::new(),
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }

    /// Builtins + user presets; builtins always load (dsp spec edge case).
    pub fn all_presets(&self) -> Vec<DspPreset> {
        let mut presets = builtin_presets();
        presets.extend(self.user_presets.iter().cloned());
        presets
    }

    pub fn find_preset(&self, id: &str) -> Option<DspPreset> {
        self.all_presets().into_iter().find(|p| p.id == id)
    }
}

/// Tauri-managed state. Pipeline handle is None while stopped.
pub struct AppState {
    pub config_path: PathBuf,
    pub config: Mutex<AppConfig>,
    pub store: Mutex<ClipStore>,
    pub pipeline: Mutex<Option<PipelineHandle>>,
    pub dsp_params: Arc<DspParams>,
}

impl AppState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Result<Self, String> {
        let config_path = config_dir.join("config.json");
        let config = AppConfig::load(&config_path);
        let store = ClipStore::open(&data_dir).map_err(|e| e.to_string())?;
        Ok(Self {
            config_path,
            config: Mutex::new(config),
            store: Mutex::new(store),
            pipeline: Mutex::new(None),
            dsp_params: DspParams::new(0.0, 0.0),
        })
    }

    pub fn save_config(&self) {
        self.config.lock().save(&self.config_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_preserves_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("mask-cfg-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(
            &path,
            r#"{ "version": 1, "monitorEnabled": true, "activePresetId": "robot",
                 "userPresets": [], "onboardingDone": true, "futureField": 42 }"#,
        )
        .unwrap();

        let config = AppConfig::load(&path);
        assert!(config.monitor_enabled);
        assert_eq!(config.active_preset_id, "robot");
        assert_eq!(config.extra.get("futureField").unwrap(), 42);

        config.save(&path);
        let reloaded = AppConfig::load(&path);
        assert_eq!(reloaded.extra.get("futureField").unwrap(), 42);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupted_config_falls_back_to_default() {
        let dir = std::env::temp_dir().join(format!("mask-cfg-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{ not json").unwrap();
        let config = AppConfig::load(&path);
        assert_eq!(config.active_preset_id, "none");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn all_presets_merges_user_after_builtin() {
        let mut config = AppConfig::default();
        config.user_presets.push(DspPreset {
            id: "custom-1".into(),
            name: "Robot (custom)".into(),
            pitch: 1.0,
            formant: 0.0,
            effects: vec![],
            is_builtin: false,
        });
        let all = config.all_presets();
        assert_eq!(all.first().unwrap().id, "none");
        assert_eq!(all.last().unwrap().id, "custom-1");
        assert!(config.find_preset("custom-1").is_some());
    }
}
