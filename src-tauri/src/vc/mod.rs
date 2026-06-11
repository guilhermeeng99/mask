//! AI voice conversion (phase 2): RVC models exported to ONNX, run locally
//! through ONNX Runtime (`ort`). Contract: docs/specs/ai_voice_conversion.md.

pub mod download;
pub mod engine;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VcError {
    #[error("model not found: {0}")]
    NotFound(String),
    #[error("invalid ONNX model: {0}")]
    InvalidModel(String),
    #[error("inference runtime error: {0}")]
    Runtime(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("model library corrupted: {0}")]
    Library(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VcModel {
    pub id: String,
    pub name: String,
    pub onnx_path: PathBuf,
    pub index_path: Option<PathBuf>,
    /// Suggested semitone offset for this voice (e.g. +12 for male→female).
    pub default_pitch: i32,
    /// Model output sample rate (32k/40k/48k RVC variants).
    pub sample_rate: u32,
    /// Origin and license, shown in the UI (spec rule 9). For personal clones
    /// this records the consent confirmation (voice_cloning rule 1).
    pub license_note: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcSettings {
    pub model_id: String,
    /// Added to the model's default pitch, -24..=24 semitones.
    pub pitch_offset: i32,
    /// Latency vs stability tradeoff (spec rule 5).
    pub chunk_ms: u32,
}

impl Default for VcSettings {
    fn default() -> Self {
        Self {
            model_id: String::new(),
            pitch_offset: 0,
            chunk_ms: 320,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InferenceBackend {
    DirectMl,
    Cuda,
    Cpu,
}

/// Companion models required by every RVC voice: the ContentVec feature
/// encoder and the RMVPE pitch extractor. They are downloaded on first VC
/// use (spec rule 3), never bundled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionManifest {
    pub contentvec_url: String,
    pub contentvec_sha256: String,
    pub rmvpe_url: String,
    pub rmvpe_sha256: String,
}

pub struct CompanionPaths {
    pub contentvec: PathBuf,
    pub rmvpe: PathBuf,
}

pub fn companion_paths(data_dir: &Path) -> CompanionPaths {
    let dir = data_dir.join("models").join("companions");
    CompanionPaths {
        contentvec: dir.join("contentvec.onnx"),
        rmvpe: dir.join("rmvpe.onnx"),
    }
}

pub fn companions_installed(data_dir: &Path) -> bool {
    let paths = companion_paths(data_dir);
    paths.contentvec.exists() && paths.rmvpe.exists()
}

/// JSON-backed model library; files are copied under `<data>/models/<id>/`.
pub struct ModelStore {
    library_path: PathBuf,
    models_dir: PathBuf,
    models: Vec<VcModel>,
}

impl ModelStore {
    pub fn open(data_dir: &Path) -> Result<Self, VcError> {
        let models_dir = data_dir.join("models");
        fs::create_dir_all(&models_dir)?;
        let library_path = data_dir.join("vc_models.json");
        let models = if library_path.exists() {
            let raw = fs::read_to_string(&library_path)?;
            serde_json::from_str(&raw).map_err(|e| VcError::Library(e.to_string()))?
        } else {
            Vec::new()
        };
        Ok(Self {
            library_path,
            models_dir,
            models,
        })
    }

    pub fn list(&self) -> Vec<VcModel> {
        self.models.clone()
    }

    pub fn get(&self, id: &str) -> Result<&VcModel, VcError> {
        self.models
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| VcError::NotFound(id.into()))
    }

    /// Import copies the .onnx (and optional .index) into the library.
    /// The session is created eagerly at import to validate the file
    /// (spec edge case: invalid ONNX rejected at import time).
    #[allow(clippy::too_many_arguments)]
    pub fn import(
        &mut self,
        onnx: &Path,
        index: Option<&Path>,
        name: String,
        default_pitch: i32,
        sample_rate: u32,
        license_note: String,
        now_iso: String,
        validate: impl Fn(&Path) -> Result<(), VcError>,
    ) -> Result<VcModel, VcError> {
        validate(onnx)?;

        let id = uuid::Uuid::new_v4().to_string();
        let dir = self.models_dir.join(&id);
        fs::create_dir_all(&dir)?;
        let onnx_dest = dir.join("model.onnx");
        fs::copy(onnx, &onnx_dest)?;
        let index_dest = match index {
            Some(src) => {
                let dest = dir.join("model.index");
                fs::copy(src, &dest)?;
                Some(dest)
            }
            None => None,
        };

        let model = VcModel {
            id,
            name,
            onnx_path: onnx_dest,
            index_path: index_dest,
            default_pitch,
            sample_rate,
            license_note,
            created_at: now_iso,
        };
        self.models.push(model.clone());
        self.save()?;
        Ok(model)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), VcError> {
        let idx = self
            .models
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| VcError::NotFound(id.into()))?;
        let model = self.models.remove(idx);
        if let Some(dir) = model.onnx_path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
        self.save()
    }

    fn save(&self) -> Result<(), VcError> {
        let raw = serde_json::to_string_pretty(&self.models)
            .map_err(|e| VcError::Library(e.to_string()))?;
        fs::write(&self.library_path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_validates_then_copies_and_persists() {
        let dir = std::env::temp_dir().join(format!("mask-vc-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let fake_onnx = dir.join("voice.onnx");
        fs::write(&fake_onnx, b"onnx-bytes").unwrap();

        let mut store = ModelStore::open(&dir).unwrap();
        let model = store
            .import(
                &fake_onnx,
                None,
                "Friend".into(),
                12,
                40_000,
                "community model, unknown origin".into(),
                "2026-06-11T00:00:00Z".into(),
                |_| Ok(()),
            )
            .unwrap();
        assert!(model.onnx_path.exists());

        // Reload from disk.
        let store2 = ModelStore::open(&dir).unwrap();
        assert_eq!(store2.list().len(), 1);
        assert_eq!(store2.get(&model.id).unwrap().name, "Friend");

        let mut store3 = store2;
        store3.remove(&model.id).unwrap();
        assert!(store3.list().is_empty());
        assert!(!model.onnx_path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn import_rejects_invalid_model_before_copying() {
        let dir = std::env::temp_dir().join(format!("mask-vc-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let fake_onnx = dir.join("bad.onnx");
        fs::write(&fake_onnx, b"junk").unwrap();

        let mut store = ModelStore::open(&dir).unwrap();
        let result = store.import(
            &fake_onnx,
            None,
            "Bad".into(),
            0,
            40_000,
            String::new(),
            "t".into(),
            |_| Err(VcError::InvalidModel("not a model".into())),
        );
        assert!(result.is_err());
        assert!(store.list().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
