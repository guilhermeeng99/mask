# Mask Specs

Spec-driven development: every feature has a spec here before code or tests are written (see CLAUDE.md).

| Spec | Scope | Phase |
|---|---|---|
| [audio_pipeline.md](audio_pipeline.md) | Capture → processing → mixing → virtual mic output, devices, latency budget | 1 |
| [dsp_effects.md](dsp_effects.md) | Pitch/formant shift, robot and other classic effects, presets | 1 |
| [soundboard.md](soundboard.md) | Upload sound clips, play them into the call, mixing rules | 1 |
| [virtual_mic_setup.md](virtual_mic_setup.md) | VB-Cable / Virtual-Audio-Driver detection and guided onboarding | 1 |
| [app_shell.md](app_shell.md) | Main window, frontend stores, config persistence | 1 |
| [design_system.md](design_system.md) | Tokens, shared UI primitives, UX rules, key screens | 1 |
| [ai_voice_conversion.md](ai_voice_conversion.md) | RVC over ONNX, realistic voice identity change | 2 |
| [voice_cloning.md](voice_cloning.md) | Local training of a consenting friend's voice | 3 |
