# DSP Effects Spec

Classic signal-processing voice effects (phase 1). Fast (≤ 50 ms), CPU-only, no models. Implemented as a `BlockProcessor` chain (see [audio_pipeline.md](audio_pipeline.md)).

## Effect Contract

```rust
DspPreset {
  id:          String        (stable, kebab-case: "deep-voice", "robot", ...)
  name:        String        (display label)
  pitch:       f32           (semitones, -12.0 ..= 12.0)
  formant:     f32           (semitones, -12.0 ..= 12.0, independent of pitch)
  effects:     Vec<ExtraEffect>  (ordered post-shift effects)
  is_builtin:  bool          (builtin presets are read-only)
}

ExtraEffect =
  | RingMod { freq_hz: f32 }            // robot
  | Distortion { drive: f32 }           // megaphone/radio flavor
  | Reverb { mix: f32, decay: f32 }     // cave/hall
  | HighPass { cutoff_hz: f32 }
  | LowPass { cutoff_hz: f32 }
```

## Built-in Presets (V1)

| id | name | pitch | formant | extras |
|---|---|---|---|---|
| `none` | Clean | 0 | 0 | none |
| `deep-voice` | Deep voice | -4 | -3 | none |
| `deeper-voice` | Very deep | -7 | -5 | none |
| `high-voice` | High voice | +4 | +3 | none |
| `woman` | Feminine | +4 | +5 | none |
| `man` | Masculine | -4 | -4 | none |
| `robot` | Robot | 0 | 0 | RingMod 80 Hz + Distortion 0.2 |
| `chipmunk` | Chipmunk | +9 | +8 | none |
| `cave` | Cave | -2 | 0 | Reverb mix 0.4 decay 2.0 |
| `radio` | Old radio | 0 | 0 | HighPass 400 + LowPass 3400 + Distortion 0.3 |

Preset values are starting points; tune by measurement and listening tests during implementation, then freeze in this table.

## Business Rules

1. **Pitch and formant shift via signalsmith-stretch**, configured for its lowest-latency real-time mode. This is the only stretch/shift engine; no parallel implementations.
2. **Pitch and formant are independent parameters.** "Deeper voice" lowers both; "robot" changes neither. The UI exposes both sliders in custom mode.
3. **Parameter changes are click-free**: smoothed over at most one block (10 ms) via the atomics + smoothing rule from CLAUDE.md.
4. **Presets are data, not code.** Built-ins ship as a const list; user-customized presets are saved to config as the same struct with `is_builtin: false`.
5. **The user can edit a builtin's knobs live**, but saving creates a copy ("Robot (custom)"); built-ins themselves are immutable.
6. **Effect chain order is fixed**: pitch/formant shift → extras in declared order → mixer. Reordering is not user-configurable in V1.
7. **DSP must process one 480-sample block in well under 10 ms** on a mid-range CPU (target: < 2 ms). A preset that cannot meet this does not ship.
8. **Bypass (`none` preset) is bit-exact passthrough** apart from the limiter.

## Module Contract

```rust
// src-tauri/src/dsp/

struct DspChain { /* implements BlockProcessor */ }

impl DspChain {
  fn new(sample_rate: u32, block_size: usize) -> Self;
  fn set_preset(&mut self, preset: &DspPreset);      // real-time safe: writes atomics
  fn set_pitch(&mut self, semitones: f32);            // live knob, same safety
  fn set_formant(&mut self, semitones: f32);
}
```

### Tauri commands

```
dsp_list_presets() -> Vec<DspPreset>          // builtins + user presets
dsp_set_preset(id: String) -> Result<(), String>
dsp_set_params(pitch: f32, formant: f32) -> ()  // custom mode live tweak
dsp_save_preset(preset: DspPreset) -> Result<DspPreset, String>
dsp_delete_preset(id: String) -> Result<(), String>
```

## State Machine

```
ActivePreset(id) ──dsp_set_preset(other)──▶ Crossfading ──1 block──▶ ActivePreset(other)
ActivePreset(id) ──dsp_set_params──▶ ActivePreset(id, modified=true)
modified ──dsp_save_preset──▶ new user preset, ActivePreset(new_id)
```

## Edge Cases

- **Out-of-range pitch/formant** — clamped to [-12, +12] at the command boundary, never inside the audio thread.
- **Deleting the active user preset** — pipeline falls back to `none`; UI selects Clean.
- **Deleting a builtin** — rejected with validation error.
- **Duplicate preset name on save** — allowed; ids are generated, names are labels.
- **Preset file corrupted on disk** — user presets that fail to deserialize are skipped with a logged warning; builtins always load.

## Testing Notes

- Feed a 440 Hz sine through pitch +12 and assert dominant FFT bin ≈ 880 Hz.
- Assert `none` preset output equals input within limiter tolerance.
- Assert no allocation in `process()` (assert_no_alloc crate in debug tests).
