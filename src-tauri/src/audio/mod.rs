//! Audio device enumeration and the pipeline engine.
//! Contract: docs/specs/audio_pipeline.md.

pub mod pipeline;
pub mod virtual_mic;

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("input and output must be different devices")]
    SameDevice,
    #[error("failed to open stream: {0}")]
    Stream(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    /// Stable cpal `DeviceId`, persisted via its `Display` form.
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
    pub is_virtual_mic: bool,
}

fn describe(dev: &cpal::Device) -> Option<(String, String)> {
    let id = dev.id().ok()?.to_string();
    let name = dev.description().ok()?.to_string();
    Some((id, name))
}

/// Enumerate all input and output endpoints on the default host (WASAPI).
pub fn list_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_in = host
        .default_input_device()
        .and_then(|d| d.id().ok().map(|i| i.to_string()));
    let default_out = host
        .default_output_device()
        .and_then(|d| d.id().ok().map(|i| i.to_string()));

    let mut devices = Vec::new();
    if let Ok(inputs) = host.input_devices() {
        for dev in inputs {
            let Some((id, name)) = describe(&dev) else {
                continue;
            };
            devices.push(AudioDevice {
                is_default: default_in.as_deref() == Some(&id),
                is_virtual_mic: false,
                kind: DeviceKind::Input,
                id,
                name,
            });
        }
    }
    if let Ok(outputs) = host.output_devices() {
        for dev in outputs {
            let Some((id, name)) = describe(&dev) else {
                continue;
            };
            devices.push(AudioDevice {
                is_default: default_out.as_deref() == Some(&id),
                is_virtual_mic: virtual_mic::is_virtual_mic_name(&name),
                kind: DeviceKind::Output,
                id,
                name,
            });
        }
    }
    devices
}

pub fn find_device(id: &str, kind: DeviceKind) -> Result<cpal::Device, AudioError> {
    let host = cpal::default_host();
    let iter: Box<dyn Iterator<Item = cpal::Device>> = match kind {
        DeviceKind::Input => Box::new(
            host.input_devices()
                .map_err(|e| AudioError::Stream(e.to_string()))?,
        ),
        DeviceKind::Output => Box::new(
            host.output_devices()
                .map_err(|e| AudioError::Stream(e.to_string()))?,
        ),
    };
    for dev in iter {
        if dev.id().map(|i| i.to_string() == id).unwrap_or(false) {
            return Ok(dev);
        }
    }
    Err(AudioError::DeviceNotFound(id.into()))
}
