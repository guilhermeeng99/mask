# App Shell Spec

Main window layout, frontend state, and config persistence. The UI is a thin control surface over the Rust pipeline: it renders state pushed by events and issues commands.

## Layout

```
┌────────────────────────────────────────────────┐
│ Header: app name · pipeline status dot · ⚙     │
├──────────────┬─────────────────────────────────┤
│ Devices      │ Effects                          │
│  mic select  │  preset grid (DSP)               │
│  cable status│  custom sliders (pitch/formant)  │
│  monitor tgl │  [VC model picker — phase 2]     │
│  start/stop  │                                  │
│  in/out meter├─────────────────────────────────┤
│              │ Soundboard                       │
│              │  clip grid · import · stop all   │
└──────────────┴─────────────────────────────────┘
```

- Single window, no routing in V1 (settings is a modal).
- Onboarding ([virtual_mic_setup.md](virtual_mic_setup.md)) renders as a full-window overlay on first launch.

## Frontend State (Zustand stores)

```ts
PipelineStore {
  status: 'stopped' | 'starting' | 'running' | 'error'
  errorMessage?: string
  devices: AudioDevice[]
  selectedInputId?: string
  selectedOutputId?: string
  monitorEnabled: boolean
  metrics: { inputPeakDb: number; outputPeakDb: number; latencyMs: number }
}

EffectsStore {
  presets: DspPreset[]
  activePresetId: string
  customPitch: number
  customFormant: number
}

SoundboardStore {
  clips: SoundClip[]
  playing: { id: string; positionMs: number }[]
}
```

Stores are hydrated at startup from commands, then updated only by Tauri events (`pipeline://*`, `soundboard://*`). Components never own server state.

## Business Rules

1. **Every Tauri command goes through one typed wrapper** in `src/lib/ipc.ts` (CLAUDE.md rule); stores call wrappers, components call stores.
2. **The start/stop button is the single pipeline control.** It is disabled with a reason tooltip when no input device is selected.
3. **Meters render from the 100 ms metrics event**, drawn with requestAnimationFrame decay; no extra polling.
4. **Command failures surface as toasts** with the Rust error message; the UI never swallows an error silently.
5. **All persistence lives in Rust.** The frontend never writes files; selections are persisted by the backend on each successful command.
6. **Status dot states**: gray stopped, yellow starting, green running, red error. Error state shows the message inline in the Devices panel.
7. **Dark theme only in V1.** Tokens, primitives, and UX rules live in [design_system.md](design_system.md).
8. **English-only strings in V1**, centralized in `src/lib/strings.ts` so i18n can be added later without hunting literals.

## Config File (Rust-owned)

```jsonc
// <app-data>/config.json
{
  "input_device_id": "...",
  "output_device_id": "...",
  "monitor_device_id": null,
  "monitor_enabled": false,
  "active_preset_id": "none",
  "user_presets": [ /* DspPreset[] */ ],
  "onboarding_done": true
}
```

Versioned with a `"version": 1` field; unknown fields preserved on rewrite (forward compatibility).

## Edge Cases

- **Backend event arrives before store hydration** — stores apply events idempotently by id; hydration overwrites with authoritative state.
- **Window closed while pipeline runs** — app exits and streams stop; minimize-to-tray with keep-alive is a TODO for later (users expect voice changers to keep running).
- **Two app instances** — second instance focuses the first and exits (Tauri single-instance plugin); two pipelines on one mic are never allowed.

## Open Questions

- TODO: system tray + keep running in background (likely phase 2, users will ask).
- TODO: window size/position persistence.
