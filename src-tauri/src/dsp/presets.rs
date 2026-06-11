//! Preset data model and the builtin table from docs/specs/dsp_effects.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExtraEffect {
    RingMod { freq_hz: f32 },
    Distortion { drive: f32 },
    Reverb { mix: f32, decay: f32 },
    HighPass { cutoff_hz: f32 },
    LowPass { cutoff_hz: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DspPreset {
    pub id: String,
    pub name: String,
    pub pitch: f32,
    pub formant: f32,
    pub effects: Vec<ExtraEffect>,
    pub is_builtin: bool,
}

fn preset(id: &str, name: &str, pitch: f32, formant: f32, effects: Vec<ExtraEffect>) -> DspPreset {
    DspPreset {
        id: id.into(),
        name: name.into(),
        pitch,
        formant,
        effects,
        is_builtin: true,
    }
}

/// The 10 builtin presets. Values are the spec's starting points; tune by ear,
/// then freeze back into docs/specs/dsp_effects.md.
pub fn builtin_presets() -> Vec<DspPreset> {
    vec![
        preset("none", "Clean", 0.0, 0.0, vec![]),
        preset("deep-voice", "Deep voice", -4.0, -3.0, vec![]),
        preset("deeper-voice", "Very deep", -7.0, -5.0, vec![]),
        preset("high-voice", "High voice", 4.0, 3.0, vec![]),
        preset("woman", "Feminine", 4.0, 5.0, vec![]),
        preset("man", "Masculine", -4.0, -4.0, vec![]),
        preset(
            "robot",
            "Robot",
            0.0,
            0.0,
            vec![
                ExtraEffect::RingMod { freq_hz: 80.0 },
                ExtraEffect::Distortion { drive: 0.2 },
            ],
        ),
        preset("chipmunk", "Chipmunk", 9.0, 8.0, vec![]),
        preset(
            "cave",
            "Cave",
            -2.0,
            0.0,
            vec![ExtraEffect::Reverb {
                mix: 0.4,
                decay: 2.0,
            }],
        ),
        preset(
            "radio",
            "Old radio",
            0.0,
            0.0,
            vec![
                ExtraEffect::HighPass { cutoff_hz: 400.0 },
                ExtraEffect::LowPass { cutoff_hz: 3400.0 },
                ExtraEffect::Distortion { drive: 0.3 },
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_unique_ids_and_clean_first() {
        let presets = builtin_presets();
        assert_eq!(presets[0].id, "none");
        let mut ids: Vec<_> = presets.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), presets.len());
        assert!(presets.iter().all(|p| p.is_builtin));
    }
}
