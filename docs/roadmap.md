# Mask Roadmap

Updated: 2026-06-11

## Done

- 2026-06-11 — Feasibility research: real-time VC landscape, Windows virtual mic options, Tauri/Rust stack fit. Decisions: Tauri 2 + React + Rust + Bun, MIT license, user-installed virtual cable, RVC over ONNX for AI voices.
- 2026-06-11 — Project documentation: CLAUDE.md, specs, roadmap.
- 2026-06-11 — Design system spec (Toolzy as structural reference): dark theme tokens, shared primitives, UX rules ([design_system.md](specs/design_system.md)).

## In Progress

- (nothing — awaiting docs approval to start phase 1)

## Planned

### Phase 1 — MVP: DSP voice changer + soundboard

1. Project scaffolding: Tauri 2 + React + Bun, CI (clippy, tests, lint), MIT license, README.
2. Audio pipeline core: capture → passthrough → output, device selection, metrics, meters ([audio_pipeline.md](specs/audio_pipeline.md)).
3. Virtual mic onboarding: detection + guided VB-Cable install ([virtual_mic_setup.md](specs/virtual_mic_setup.md)).
4. DSP effects: pitch/formant via signalsmith-stretch, built-in presets, custom sliders ([dsp_effects.md](specs/dsp_effects.md)).
5. Soundboard: import, grid, playback mixing, polyphony ([soundboard.md](specs/soundboard.md)).
6. App shell polish: config persistence, error surfaces, dark theme ([app_shell.md](specs/app_shell.md)).
7. First public release: v0.1.0 installer on GitHub Releases.

### Phase 2 — AI voice conversion

8. ONNX inference engine via ort (DirectML default, CUDA optional) ([ai_voice_conversion.md](specs/ai_voice_conversion.md)).
9. RVC streaming pipeline: ContentVec + RMVPE + vocoder, chunked with crossfade.
10. Model management UI: import, license notes, latency slider, fallback handling.
11. System tray / keep running in background.

### Phase 3 — Voice cloning

12. Training path decision (companion tool vs guide) and consent-gated cloning flow ([voice_cloning.md](specs/voice_cloning.md)).
