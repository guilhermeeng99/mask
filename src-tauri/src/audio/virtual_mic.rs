//! Virtual microphone detection. Contract: docs/specs/virtual_mic_setup.md.
//! Mask never ships a driver; it detects user-installed virtual cables by
//! render-endpoint name.

use serde::Serialize;

use super::{AudioDevice, DeviceKind};

/// Known virtual-cable render endpoints, matched case-insensitively.
/// VB-Audio VB-Cable ("CABLE Input") is the primary recommendation;
/// VirtualDrivers/Virtual-Audio-Driver is the MIT-licensed alternative.
const KNOWN_VIRTUAL_NAMES: &[&str] = &["cable input", "vb-audio", "virtual audio device"];

pub fn is_virtual_mic_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    KNOWN_VIRTUAL_NAMES.iter().any(|k| lower.contains(k))
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum VirtualMicStatus {
    NotInstalled,
    Installed { device: AudioDevice },
}

/// Pick the virtual cable among output endpoints. VB-Cable wins when both
/// known cables are installed (spec edge case).
pub fn detect_virtual_mic(devices: &[AudioDevice]) -> VirtualMicStatus {
    let outputs: Vec<&AudioDevice> = devices
        .iter()
        .filter(|d| d.kind == DeviceKind::Output && d.is_virtual_mic)
        .collect();
    let vb_cable = outputs
        .iter()
        .find(|d| d.name.to_lowercase().contains("cable input"));
    match vb_cable.or(outputs.first()) {
        Some(device) => VirtualMicStatus::Installed {
            device: (*device).clone(),
        },
        None => VirtualMicStatus::NotInstalled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, kind: DeviceKind) -> AudioDevice {
        AudioDevice {
            id: name.into(),
            name: name.into(),
            kind,
            is_default: false,
            is_virtual_mic: kind == DeviceKind::Output && is_virtual_mic_name(name),
        }
    }

    #[test]
    fn detects_vb_cable() {
        let devices = vec![
            dev("Microphone (Realtek Audio)", DeviceKind::Input),
            dev("Speakers (Realtek Audio)", DeviceKind::Output),
            dev("CABLE Input (VB-Audio Virtual Cable)", DeviceKind::Output),
        ];
        match detect_virtual_mic(&devices) {
            VirtualMicStatus::Installed { device } => {
                assert!(device.name.contains("CABLE Input"));
            }
            _ => panic!("should detect VB-Cable"),
        }
    }

    #[test]
    fn prefers_vb_cable_over_other_cables() {
        let devices = vec![
            dev("Virtual Audio Device (WDM)", DeviceKind::Output),
            dev("CABLE Input (VB-Audio Virtual Cable)", DeviceKind::Output),
        ];
        match detect_virtual_mic(&devices) {
            VirtualMicStatus::Installed { device } => {
                assert!(device.name.contains("CABLE Input"));
            }
            _ => panic!("should detect a cable"),
        }
    }

    #[test]
    fn no_cable_means_not_installed() {
        let devices = vec![dev("Speakers (Realtek Audio)", DeviceKind::Output)];
        assert!(matches!(
            detect_virtual_mic(&devices),
            VirtualMicStatus::NotInstalled
        ));
    }

    #[test]
    fn input_named_cable_output_is_not_the_render_target() {
        // "CABLE Output" is the capture side that call apps use; only the
        // render side ("CABLE Input") is Mask's output target.
        let devices = vec![dev(
            "CABLE Output (VB-Audio Virtual Cable)",
            DeviceKind::Input,
        )];
        assert!(matches!(
            detect_virtual_mic(&devices),
            VirtualMicStatus::NotInstalled
        ));
    }
}
