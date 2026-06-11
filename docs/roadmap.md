# Mask Roadmap

Updated: 2026-06-11

## Done

- 2026-06-11 — Feasibility research: real-time VC landscape, Windows virtual mic options, Tauri/Rust stack fit. Decisions: Tauri 2 + React + Rust + Bun, MIT license, user-installed virtual cable, RVC over ONNX for AI voices.
- 2026-06-11 — Project documentation: CLAUDE.md, specs, roadmap.
- 2026-06-11 — Design system spec (Toolzy as structural reference): dark theme tokens, shared primitives, UX rules ([design_system.md](specs/design_system.md)).
- 2026-06-11 — Phase 1 item 1: scaffolding (Tauri 2 + React + Bun, Tailwind v4, Biome, CI, MIT, README).
- 2026-06-11 — Phase 1 item 2: audio pipeline core (cpal/WASAPI capture → engine thread → virtual mic, metrics, soft limiter) ([audio_pipeline.md](specs/audio_pipeline.md)).
- 2026-06-11 — Phase 1 item 3: virtual mic detection + guided onboarding ([virtual_mic_setup.md](specs/virtual_mic_setup.md)).
- 2026-06-11 — Phase 1 item 4: DSP effects (signalsmith-stretch pitch/formant, 10 builtin presets, user presets) ([dsp_effects.md](specs/dsp_effects.md)).
- 2026-06-11 — Phase 1 item 5: soundboard (symphonia import, 4-voice mixing, grid UI) ([soundboard.md](specs/soundboard.md)).
- 2026-06-11 — Phase 1 item 6: app shell (dark design system, Zustand stores, config persistence, toasts, meters) ([app_shell.md](specs/app_shell.md)).
- 2026-06-11 — Phase 2 items 8–10: ort/ONNX inference engine (DirectML/CUDA/CPU detection), RVC chunked streaming with crossfade and auto-fallback, model management UI with license notes ([ai_voice_conversion.md](specs/ai_voice_conversion.md)).
- 2026-06-11 — Phase 2 item 11: system tray, close-to-tray keeps the voice running.
- 2026-06-11 — Phase 3 item 12: guide-only decision recorded; consent-gated clone import shipped ([voice_cloning.md](specs/voice_cloning.md)).

- 2026-06-11 — Hardware verification (`src-tauri/tests/local_hardware.rs`): real WASAPI pipeline runs with live preset switch + soundboard mix at < 200 ms measured latency (backpressure fix), companion models verified on CUDA (RMVPE tracks 220 Hz exactly via the Rust mel path), full RVC conversion with a real community model runs faster than real time. Companion checksums pinned; vec-256 encoder support added.
- 2026-06-11 — UI rebrand to the warm cream + Electric Lime identity; download progress feedback fixed (event throttling + progress bar).

## In Progress

- v0.1.0 release polish: end-to-end call test by a human (VB-Cable + Discord), GitHub Releases artifacts.

## Planned

- Measure and tune DSP latency further on hardware (cpal vs `wasapi` crate, audio_pipeline TODO).
- Automatic device re-scan via WASAPI notifications ([virtual_mic_setup.md](specs/virtual_mic_setup.md) rule 5 TODO).
- Global hotkeys for soundboard clips ([soundboard.md](specs/soundboard.md) TODO).
- App icon/logotype and font bundling decision ([design_system.md](specs/design_system.md) TODOs).
- Companion training tool for voice cloning (option a) if the guide-only path proves hard.
