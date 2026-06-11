# Mask — Project Conventions

Real-time voice changer for Windows. Modifies the user's microphone audio live (pitch, formants, robot effects, AI voice conversion) and routes it into a virtual microphone so video-call apps (Discord, Zoom, Meet) receive the modified voice. Includes a soundboard for playing uploaded sound clips into the call. Open source (MIT).

---

## Architecture

**Tauri 2 desktop app**: React frontend (UI only) + Rust backend (all audio and inference).

```
src/                  # React frontend (TypeScript)
├── components/       # Reusable UI components
├── features/         # Feature modules (devices, effects, soundboard, onboarding)
├── hooks/            # Shared React hooks (Tauri command/event wrappers)
└── lib/              # Frontend utilities, Tauri IPC client

src-tauri/            # Rust backend
├── src/
│   ├── audio/        # Capture, output, ring buffers, device management (cpal/WASAPI)
│   ├── dsp/          # Effects: pitch, formant, robot, presets
│   ├── vc/           # AI voice conversion (ONNX via ort) — phase 2+
│   ├── soundboard/   # Clip storage, decoding, mixing
│   ├── commands/     # Tauri command handlers (thin, delegate to modules)
│   └── state/        # Shared app state, config persistence
└── tauri.conf.json
```

Hard boundary rules:

* The webview never touches audio. All capture, DSP, inference, and playback live in Rust on dedicated threads.
* The real-time audio thread never allocates, locks, or does I/O. Communication with it goes through lock-free ring buffers and atomics only.
* Tauri commands are thin adapters: parse input, call a module function, return a result. Business logic lives in the modules.
* Frontend gets state via Tauri events (push) and commands (request/response), never by polling timers under 250 ms.

---

## Code Style

* Functions: 5–25 lines. Split if longer.
* Files: ideally under 400–600 lines.
* One responsibility per function/module (SRP).
* Prefer small, composable components and modules over large ones.

### Naming

* Names must be specific and intention-revealing.
* Avoid generic names like `data`, `manager`, `handler`, `utils`.
* Prefer names that are searchable and unique within the codebase.

### Control Flow

* Prefer early returns over nested conditionals.
* Maximum 2 levels of indentation.
* In Rust, prefer `?` and combinators over deep `match` nesting.

---

## Comments

* Write **WHY**, not WHAT.
* Preserve important context and decisions.
* Do not remove meaningful comments during refactors.
* Public APIs (Rust pub items, exported TS functions) must include:

  * intent
  * parameters
  * usage example when non-obvious

---

## Key Technologies

| Aspect                | Detail                                                              |
| --------------------- | ------------------------------------------------------------------- |
| **App framework**     | Tauri 2 (Rust backend + system webview)                              |
| **Frontend**          | React + TypeScript + Vite, Bun as package manager and script runner  |
| **Styling**           | Tailwind CSS v4; all design tokens in `src/styles.css` `@theme`, shared primitives in `src/components/ui.tsx` (see `docs/specs/design_system.md`) |
| **Audio I/O**         | cpal (WASAPI shared mode); `wasapi` crate if lower latency needed    |
| **DSP**               | signalsmith-stretch (pitch + formant), custom effects in `dsp/`      |
| **AI inference**      | ort (ONNX Runtime). DirectML EP default, CUDA EP optional (phase 2)  |
| **VC model**          | RVC exported to ONNX (ContentVec encoder + RMVPE pitch + vocoder)    |
| **Virtual mic**       | VB-Cable or Virtual-Audio-Driver, user-installed (guided onboarding) |
| **Audio decoding**    | symphonia (soundboard clips: mp3, wav, ogg, flac)                    |
| **State (frontend)**  | Zustand stores fed by Tauri events                                   |
| **Config**            | JSON in Tauri app data dir, serde-backed                             |
| **Error handling**    | Rust: `thiserror` per module + `anyhow` at command boundary; TS: typed command results, never silent catch |
| **Linting**           | clippy (deny warnings) + Biome (frontend)                            |
| **License**           | MIT (all bundled dependencies and model code must be MIT-compatible) |

---

## Commands

```bash
bun install               # Install frontend dependencies
bun run tauri dev         # Run the app in development
bun run tauri build       # Build the Windows installer
bun run lint              # Biome check on src/
bun test                  # Frontend tests
cargo test                # Rust tests (run inside src-tauri/)
cargo clippy -- -D warnings   # Rust lint (must be zero warnings)
cargo fmt                 # Rust formatting
```

---

## Post-Change Checklist

After every code change:

1. Run `cargo fmt` and `cargo clippy -- -D warnings` if Rust changed — zero warnings
2. Run `cargo test` if Rust changed — all tests pass
3. Run `bun run lint` if frontend changed — zero issues
4. Run `bun test` if frontend changed — all tests pass
5. Never add `#[allow(...)]` or `// biome-ignore` without clear justification
6. If the audio pipeline changed, verify the real-time thread still has no allocation, locking, or I/O

---

## Spec-Driven Development

Every feature MUST have a spec at `docs/specs/<feature>.md` before writing new code or tests.

### Workflow

1. Write or update the spec (contracts, business rules, state machines)
2. Write tests based on the spec
3. Implement or modify code to pass the tests
4. Update the spec if requirements change

### Spec Structure

* Entity/type contracts (fields, types, invariants)
* Business rules (numbered, testable)
* Module contract (Rust trait or Tauri command signatures)
* State machines (pipeline states, UI states and transitions)
* Edge cases

---

## Testing Rules

* Every new module function with logic must have tests
* Every bug fix must include a regression test
* Tests must follow F.I.R.S.T principles (Fast, Independent, Repeatable, Self-validating, Timely)

### Test Structure

* Rust: unit tests in-module (`#[cfg(test)]`), integration tests in `src-tauri/tests/`
* DSP and mixing logic tested on synthetic buffers (sine waves, impulses) with measurable assertions, never "by ear"
* Frontend: one test file per component/store with logic; mock the Tauri IPC layer
* Audio device I/O and ONNX inference are mocked at trait boundaries; real-device tests are manual and documented in the spec

---

## Dependencies

* Depend on abstractions, not implementations
* Audio backends, inference runtimes, and the virtual-mic detection are wrapped behind project-owned traits so they can be mocked and swapped
* Every new dependency must be MIT-compatible (MIT, Apache-2.0, BSD). GPL/AGPL dependencies are rejected
* Model weights bundled or downloaded by the app must have a verified redistribution-friendly license

---

## Code Conventions

### Rust

* Errors are `thiserror` enums per module; Tauri commands convert to a serializable error type
* No `unwrap()`/`expect()` outside tests; the audio thread must never panic
* Shared state via `Arc` + atomics or channels; `Mutex` is forbidden on the audio path
* Public module APIs take domain types, not raw primitives, when invariants exist (e.g. `SemitoneShift`, not `f32`)

### TypeScript / React

* Every Tauri command has one typed wrapper function in `src/lib/ipc.ts`; components never call `invoke` directly
* UI state in Zustand stores; components stay presentational
* All user-facing strings centralized in `src/lib/strings.ts` (single locale: English for V1)
* No raw hex colors or one-off shadows in components — design tokens only; a visual used twice moves to `ui.tsx` in the same PR

---

## Real-Time Audio Rules

The audio callback is sacred. Inside it:

* No allocation, no locks, no syscalls, no logging, no panics
* Fixed-size preallocated buffers; communicate via `ringbuf` SPSC queues
* Parameter changes arrive via atomics, applied with smoothing to avoid clicks

Latency budget (capture → virtual mic):

* DSP path: ≤ 50 ms end-to-end
* AI VC path: ≤ 250 ms end-to-end
* Soundboard playback: mixed into the same output stream, start latency ≤ 100 ms

Every change touching the pipeline must state its latency impact in the PR description.
