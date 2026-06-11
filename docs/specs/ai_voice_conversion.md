# AI Voice Conversion Spec (Phase 2)

Realistic voice identity change (woman, man, characters) using RVC (Retrieval-based Voice Conversion) models exported to ONNX, running locally via ONNX Runtime. Replaces the DSP chain as the pipeline's `BlockProcessor` when active.

Status: implemented (2026-06-11) pending verification with real model files. The engine introspects ONNX graph inputs at load and rejects unknown layouts; the expected layouts follow the RVC-Project export / w-okada conventions. Companion downloads currently run in trust-on-first-use mode (manifest URLs pinned, sha256 fields empty) and MUST be checksum-pinned before VC ships enabled by default.

## Architecture

```
mic blocks (48 kHz) ──▶ chunker (context + lookahead)
  ──▶ resample 16 kHz ──▶ ContentVec encoder (ONNX) ──┐
  ──▶ RMVPE pitch extractor (ONNX) ──────────────────┼──▶ RVC model (ONNX)
                                   pitch shift offset ┘        │
                                              vocoder output ──▶ resample 48 kHz ──▶ pipeline mixer
```

Inference runs on a dedicated thread fed by ring buffers; the audio callback only moves samples. Chunked streaming follows the w-okada pattern: each inference processes `chunk + context` samples and emits `chunk` samples, crossfaded at the seams.

## Contracts

```rust
VcModel {
  id:            String   (UUID)
  name:          String   (display label)
  onnx_path:     PathBuf  (<app-data>/models/<id>/model.onnx)
  index_path:    Option<PathBuf>  (FAISS retrieval index, optional)
  default_pitch: i32      (semitone offset suggested for this voice, e.g. +12 male→female)
  sample_rate:   u32      (model output rate: 32k/40k/48k)
  license_note:  String   (origin and license of the model, shown in UI)
}

VcSettings {
  model_id:     String
  pitch_offset: i32    (-24 ..= 24 semitones, added to default)
  index_ratio:  f32    (0.0 ..= 1.0, retrieval blend; 0 when no index)
  chunk_ms:     u32    (latency vs quality tradeoff, default TODO)
}

enum InferenceBackend { DirectML, Cuda, Cpu }
```

## Business Rules

1. **Runtime is the `ort` crate (ONNX Runtime).** Execution provider selection: CUDA if available and enabled in settings, else DirectML, else CPU with a quality/latency warning. Detection happens at startup; the active backend is shown in settings.
2. **Models are user-imported**, not bundled: the app imports `.onnx` RVC models (plus optional `.index`) from disk. A curated "where to find voices" doc page links to community model hubs. No model weights ship in the repo or installer (size and licensing).
3. **Required companion models (ContentVec encoder, RMVPE pitch) are downloaded on first VC use** from a pinned URL with checksum verification, stored in app data. The app works fully without them until the user enters VC mode.
4. **MIT compatibility rule applies** (CLAUDE.md): RVC code MIT; companion weights must have verified redistribution terms before the download URL is pinned. GPL models (e.g. Seed-VC) are out, even as optional downloads.
5. **End-to-end latency target ≤ 250 ms** on an NVIDIA GPU with 4+ GB VRAM. `chunk_ms` exposed as a "latency vs stability" slider with safe default.
6. **Switching DSP ↔ VC mode crossfades** (pipeline rule 2); switching VC models stops inference, loads the new session, restarts. UI shows a loading state; mic passes through DSP `none` meanwhile.
7. **If inference cannot keep up with real time** (queue grows for > 2 s), the pipeline auto-falls back to Passthrough and emits an error event suggesting a larger chunk or CPU-lighter settings. Garbled audio is never sent to the call.
8. **`.pth` files are not loaded directly.** V1 of this feature accepts ONNX only; a conversion guide (or separate CLI tool) covers exporting `.pth` to ONNX. Keeps Python out of the shipped app.
9. **Every model shows its `license_note`** in the model list; imports without a stated origin default to "unknown origin, personal use".

## Tauri commands

```
vc_list_models() -> Vec<VcModel>
vc_import_model(onnx: String, index: Option<String>, name: String) -> Result<VcModel, String>
vc_delete_model(id: String) -> Result<(), String>
vc_activate(settings: VcSettings) -> Result<(), String>     // also switches pipeline mode
vc_deactivate() -> ()                                        // back to DSP mode
vc_backend_info() -> { backend: InferenceBackend, device_name: String }
vc_download_companions() -> Result<(), String>               // progress via event
```

### Events

```
vc://companion-progress  ({ file, downloaded, total })
vc://state               (Loading | Active{model_id} | Fallback{reason} | Inactive)
```

## State Machine

```
Inactive ──vc_activate──▶ CheckingCompanions ──missing──▶ Downloading ──done──▶ LoadingSession
                                            ──present──▶ LoadingSession
LoadingSession ──ok──▶ Active ──vc_deactivate──▶ Inactive
LoadingSession ──error──▶ Inactive + error event
Active ──can't keep up──▶ Fallback(Passthrough) ──user retry──▶ LoadingSession
Active ──vc_activate(other model)──▶ LoadingSession
```

## Edge Cases

- **No GPU / DirectML unavailable** — CPU backend allowed but warned; if a chunk takes longer than real time, rule 7 fallback triggers immediately.
- **Invalid ONNX file imported** — session creation fails at import time (validated eagerly), import rejected with the runtime error.
- **Companion download interrupted** — partial files discarded (checksum mismatch), retry offered.
- **VRAM exhausted** (other apps using GPU) — session creation error surfaces; suggest closing GPU apps or CPU mode.
- **Model with mismatched sample rate** — output resampled to 48 kHz like any boundary (pipeline rule 1).

## Open Questions

- TODO: confirm ContentVec checkpoint licensing for pinned download (community mirrors are MIT-tagged; verify before release).
- TODO: benchmark chunk sizes (96/192/384 ms) on RTX-class GPU and pick the default.
- TODO: evaluate `obs-rvc` (Rust RVC implementation) as reference or dependency.
