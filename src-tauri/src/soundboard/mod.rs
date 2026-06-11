//! Soundboard: clip library (import/persist) and decoding to the pipeline's
//! internal format. Contract: docs/specs/soundboard.md. Mixing happens in the
//! audio pipeline (`crate::audio::pipeline`), fed by `DecodedClip`s.

mod decode;

pub use decode::decode_to_mono_48k;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_VOICES: usize = 4;

#[derive(Debug, Error)]
pub enum SoundboardError {
    #[error("unsupported or corrupted audio file: {0}")]
    Decode(String),
    #[error("clip not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("library file corrupted: {0}")]
    Library(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoundClip {
    pub id: String,
    pub name: String,
    pub source_path: PathBuf,
    pub duration_ms: u64,
    pub volume: f32,
    pub sort_index: u32,
    pub created_at: String,
}

/// Fully decoded clip samples, shared with the audio thread.
pub struct DecodedClip {
    pub id: String,
    pub samples: Arc<Vec<f32>>,
    pub volume: f32,
}

/// JSON-backed clip library. All methods are called from command handlers
/// (never the audio thread), so plain file I/O is fine here.
pub struct ClipStore {
    library_path: PathBuf,
    sounds_dir: PathBuf,
    clips: Vec<SoundClip>,
}

impl ClipStore {
    pub fn open(data_dir: &Path) -> Result<Self, SoundboardError> {
        let sounds_dir = data_dir.join("sounds");
        fs::create_dir_all(&sounds_dir)?;
        let library_path = data_dir.join("soundboard.json");
        let clips = if library_path.exists() {
            let raw = fs::read_to_string(&library_path)?;
            serde_json::from_str(&raw).map_err(|e| SoundboardError::Library(e.to_string()))?
        } else {
            Vec::new()
        };
        Ok(Self {
            library_path,
            sounds_dir,
            clips,
        })
    }

    pub fn list(&self) -> Vec<SoundClip> {
        let mut clips = self.clips.clone();
        clips.sort_by_key(|c| c.sort_index);
        clips
    }

    pub fn get(&self, id: &str) -> Result<&SoundClip, SoundboardError> {
        self.clips
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| SoundboardError::NotFound(id.into()))
    }

    /// Import one file: decode to validate + measure, then copy into the
    /// library dir. Rejects undecodable files before anything is persisted.
    pub fn import(&mut self, source: &Path, now_iso: String) -> Result<SoundClip, SoundboardError> {
        let samples = decode_to_mono_48k(source)?;
        let duration_ms = (samples.len() as u64 * 1000) / 48_000;

        let id = uuid::Uuid::new_v4().to_string();
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let dest = self.sounds_dir.join(format!("{id}.{ext}"));
        fs::copy(source, &dest)?;

        let name = source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("clip")
            .to_string();
        let clip = SoundClip {
            id,
            name,
            source_path: dest,
            duration_ms,
            volume: 1.0,
            sort_index: self.next_sort_index(),
            created_at: now_iso,
        };
        self.clips.push(clip.clone());
        self.save()?;
        Ok(clip)
    }

    pub fn update(&mut self, updated: SoundClip) -> Result<(), SoundboardError> {
        let clip = self
            .clips
            .iter_mut()
            .find(|c| c.id == updated.id)
            .ok_or_else(|| SoundboardError::NotFound(updated.id.clone()))?;
        clip.name = updated.name;
        clip.volume = updated.volume.clamp(0.0, 2.0);
        clip.sort_index = updated.sort_index;
        self.save()
    }

    /// Removes the entry and its copied file; a missing file is not an error
    /// (rule 12).
    pub fn remove(&mut self, id: &str) -> Result<(), SoundboardError> {
        let idx = self
            .clips
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| SoundboardError::NotFound(id.into()))?;
        let clip = self.clips.remove(idx);
        let _ = fs::remove_file(&clip.source_path);
        self.save()
    }

    /// Decode a stored clip for playback.
    pub fn decode(&self, id: &str) -> Result<DecodedClip, SoundboardError> {
        let clip = self.get(id)?;
        let samples = decode_to_mono_48k(&clip.source_path)?;
        Ok(DecodedClip {
            id: clip.id.clone(),
            samples: Arc::new(samples),
            volume: clip.volume,
        })
    }

    fn next_sort_index(&self) -> u32 {
        self.clips
            .iter()
            .map(|c| c.sort_index + 1)
            .max()
            .unwrap_or(0)
    }

    fn save(&self) -> Result<(), SoundboardError> {
        let raw = serde_json::to_string_pretty(&self.clips)
            .map_err(|e| SoundboardError::Library(e.to_string()))?;
        fs::write(&self.library_path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(dir: &Path, name: &str) -> PathBuf {
        // Minimal 16-bit PCM mono WAV, 0.1 s of 440 Hz at 48 kHz.
        let samples: Vec<i16> = (0..4800)
            .map(|i| {
                let v = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin();
                (v * 20_000.0) as i16
            })
            .collect();
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&(48_000u32 * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn import_list_update_remove_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mask-sb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let wav = write_test_wav(&dir, "laugh.wav");

        let mut store = ClipStore::open(&dir).unwrap();
        let clip = store.import(&wav, "2026-06-11T00:00:00Z".into()).unwrap();
        assert_eq!(clip.name, "laugh");
        assert!((clip.duration_ms as i64 - 100).abs() <= 2);

        // Original deletable without breaking the library (rule 1).
        fs::remove_file(&wav).unwrap();
        assert_eq!(store.decode(&clip.id).unwrap().samples.len(), 4800);

        let mut renamed = clip.clone();
        renamed.name = "best laugh".into();
        renamed.volume = 5.0; // clamped to 2.0
        store.update(renamed).unwrap();

        // Reload from disk: persistence works.
        let store2 = ClipStore::open(&dir).unwrap();
        let listed = store2.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "best laugh");
        assert_eq!(listed[0].volume, 2.0);

        let mut store3 = store2;
        store3.remove(&clip.id).unwrap();
        assert!(store3.list().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn import_rejects_non_audio() {
        let dir = std::env::temp_dir().join(format!("mask-sb-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let bogus = dir.join("not-audio.mp3");
        fs::write(&bogus, b"definitely not audio").unwrap();
        let mut store = ClipStore::open(&dir).unwrap();
        assert!(store.import(&bogus, "t".into()).is_err());
        assert!(store.list().is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
