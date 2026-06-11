# Audio Pipeline Spec

The core of Mask: capture the physical microphone, process the signal, mix soundboard clips, and deliver the result to the virtual microphone device that call apps use as input.

## Pipeline Contract

```
[Physical mic] ──capture (cpal/WASAPI shared)──▶ [input ring buffer]
   ──▶ [processing thread: DSP chain | AI VC engine]
   ──▶ [mixer: voice + soundboard clips]
   ──▶ [output ring buffer] ──render──▶ [Virtual mic device (VB-Cable input)]
                                  └────▶ [Monitor output (user's headphones), optional]
```

```rust
PipelineConfig {
  input_device_id:   String        (required, physical microphone)
  output_device_id:  String        (required, virtual mic render endpoint)
  monitor_device_id: Option<String> (None = monitoring off)
  sample_rate:       u32           (fixed 48_000 internally; device rates resampled)
  block_size:        usize         (processing block, 480 samples = 10 ms at 48 kHz)
  channels:          u16           (internal processing is mono; output upmixed if needed)
}

PipelineMetrics {
  input_peak_db:   f32   (pre-processing level, for UI meter)
  output_peak_db:  f32   (post-mix level, for UI meter)
  latency_ms:      f32   (estimated end-to-end)
  underruns:       u64   (cumulative output underruns since start)
}
```

## Business Rules

1. **Internal format is mono f32 at 48 kHz.** Device streams at other rates are resampled at the boundary (rubato or linear for V1); stereo inputs are downmixed by averaging.
2. **The pipeline has exactly one processing mode active at a time**: `Passthrough`, `Dsp(preset)`, or `AiVc(model)` (phase 2). Switching modes crossfades over one block to avoid clicks.
3. **The audio callback never blocks.** If the input ring buffer is empty, the renderer outputs silence and increments `underruns`; it never waits.
4. **Soundboard audio is mixed post-voice-processing**, so effects never alter clip playback (see [soundboard.md](soundboard.md)).
5. **Output is soft-clipped** (tanh limiter) so voice + clips can never exceed 0 dBFS.
6. **Monitoring is optional and off by default.** When enabled, the user hears their own processed voice on a chosen output device, same signal as the virtual mic.
7. **Metrics are published to the UI via a Tauri event every 100 ms** from a non-real-time thread that reads atomics written by the audio threads.
8. **Device disconnection stops the pipeline gracefully** and emits a `pipeline://error` event; the app never crashes from a yanked USB mic.
9. **Pipeline state persists**: selected devices, mode, and preset are saved to config on change and restored at startup. If a saved device is missing, fall back to `Stopped` with an error banner, never auto-pick another device.
10. **The DSP path must stay ≤ 50 ms end-to-end** (capture buffer + block + render buffer). Block size or buffer changes require re-measuring.

## Module Contract

```rust
// src-tauri/src/audio/

trait AudioBackend {                 // wraps cpal, mockable in tests
  fn list_devices(&self) -> Vec<AudioDevice>;
  fn start(&mut self, config: &PipelineConfig, processor: Box<dyn BlockProcessor>) -> Result<(), AudioError>;
  fn stop(&mut self);
}

trait BlockProcessor: Send {         // implemented by DSP chain and VC engine
  fn process(&mut self, input: &[f32], output: &mut [f32]);  // real-time safe
}

AudioDevice {
  id:         String  (stable device identifier)
  name:       String  (human label, e.g. "CABLE Input (VB-Audio Virtual Cable)")
  kind:       DeviceKind (Input | Output)
  is_default: bool
  is_virtual_mic: bool  (heuristic: name matches known virtual cable products)
}
```

### Tauri commands

```
audio_list_devices() -> Vec<AudioDevice>
pipeline_start(config: PipelineConfig) -> Result<(), String>
pipeline_stop() -> ()
pipeline_set_mode(mode: ProcessingMode) -> Result<(), String>
```

### Tauri events

```
pipeline://metrics   (PipelineMetrics, every 100 ms while running)
pipeline://state     (Stopped | Starting | Running | Error{message})
pipeline://devices   (emitted when the OS device list changes)
```

## State Machine

```
Stopped ──pipeline_start──▶ Starting ──streams open──▶ Running
                                     ──open fails────▶ Error
Running ──pipeline_stop──▶ Stopped
Running ──device lost────▶ Error
Error   ──pipeline_start──▶ Starting
Running ──pipeline_set_mode──▶ Running (crossfade, no restart)
```

## Edge Cases

- **No virtual mic installed** — `audio_list_devices` returns no `is_virtual_mic` device; UI routes the user to onboarding (see [virtual_mic_setup.md](virtual_mic_setup.md)). Pipeline can still run to a normal output for testing.
- **Sample-rate mismatch** (e.g. mic at 44.1 kHz) — resample at the capture boundary; never change the internal rate.
- **Input and output on different clocks** — drift handled by the ring buffer slack; if it exceeds the buffer, drop/insert one block and count it as an underrun.
- **Mode switch while a soundboard clip plays** — clip continues uninterrupted (mixed post-processing).
- **Start with the same device as input and output** — rejected with a validation error (feedback loop).
- **System default device changed by Windows mid-call** — ignored; Mask only follows explicit user selection.

## Open Questions

- TODO: validate whether cpal's WASAPI shared-mode latency is acceptable on real hardware, or whether the `wasapi` crate (IAudioClient3) is needed for the 50 ms budget.
- TODO: decide resampler for V1 (linear vs rubato) after measuring CPU on a low-end machine.
