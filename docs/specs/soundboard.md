# Soundboard Spec

User uploads sound clips (laugh, applause, memes) and plays them into the call with one click. Clips are mixed into the virtual mic output alongside the processed voice (see [audio_pipeline.md](audio_pipeline.md)).

## Entity Contract

```rust
SoundClip {
  id:           String    (UUID, generated on import)
  name:         String    (display label, defaults to file stem, editable)
  source_path:  PathBuf   (copy inside app data dir, NOT the original file)
  duration_ms:  u64       (decoded duration)
  volume:       f32       (per-clip gain, 0.0 ..= 2.0, default 1.0)
  sort_index:   u32       (manual ordering in the grid)
  created_at:   DateTime  (import time)
}
```

Library persisted as `soundboard.json` in the app data dir; audio files copied to `<app-data>/sounds/<id>.<ext>`.

## Business Rules

1. **Import copies the file** into the app data dir. The original can be moved/deleted without breaking the library.
2. **Accepted formats**: wav, mp3, ogg, flac (decoded via symphonia). Unsupported files are rejected at import with a clear error naming the file.
3. **No limit on clip count.** The grid is virtualized in the UI; the library file scales linearly.
4. **Decoding happens at import and at trigger time, never on the audio thread.** V1 decodes the whole clip on the command thread before handing samples to the mixer (`Arc<Vec<f32>>`); streaming decode for very large files is a TODO if memory ever becomes a problem.
5. **Clips are resampled to the internal 48 kHz mono format** at decode time.
6. **Playback is mixed post-voice-processing** with the per-clip volume, then the master limiter applies.
7. **Polyphony: up to 4 clips simultaneously.** Triggering a 5th stops the oldest playing clip.
8. **Retriggering a playing clip restarts it** (stop + play from zero), matching soundboard muscle memory.
9. **A global "stop all sounds" action** halts every playing clip immediately.
10. **Soundboard works in every pipeline mode**, including Passthrough and even when voice processing is bypassed.
11. **The user hears the clip in the monitor output** when monitoring is on; clip playback alone never force-enables monitoring.
12. **Deleting a clip removes the library entry and the copied file.** If the file is already gone, the entry is removed anyway.
13. **Renaming and volume edits are instant and persisted on change.**

## Module Contract

```rust
// src-tauri/src/soundboard/

trait ClipStore {                       // persistence, mockable
  fn list(&self) -> Vec<SoundClip>;
  fn import(&mut self, path: &Path) -> Result<SoundClip, SoundboardError>;
  fn update(&mut self, clip: &SoundClip) -> Result<(), SoundboardError>;
  fn remove(&mut self, id: &str) -> Result<(), SoundboardError>;
}

struct ClipMixer { /* owned by the audio pipeline, real-time safe */ }
impl ClipMixer {
  fn trigger(&self, id: &str);          // lock-free message to audio thread
  fn stop(&self, id: &str);
  fn stop_all(&self);
}
```

### Tauri commands

```
soundboard_list() -> Vec<SoundClip>
soundboard_import(paths: Vec<String>) -> Result<Vec<SoundClip>, String>  // multi-select
soundboard_play(id: String) -> Result<(), String>
soundboard_stop(id: String) -> ()
soundboard_stop_all() -> ()
soundboard_update(clip: SoundClip) -> Result<(), String>   // rename, volume, sort_index
soundboard_delete(id: String) -> Result<(), String>
```

### Tauri events

```
soundboard://playing   (Vec<{ id, position_ms }>, every 100 ms while any clip plays)
```

## State Machine (per clip)

```
Idle ──play──▶ Decoding ──buffer ready──▶ Playing ──end of clip──▶ Idle
Playing ──play (retrigger)──▶ Decoding (from zero)
Playing ──stop / stop_all──▶ Idle
Decoding ──decode error──▶ Idle + soundboard://error event
```

## Edge Cases

- **Import duplicate file** — allowed; each import is an independent clip (ids differ).
- **Corrupted file that passed extension check** — symphonia decode error at import; file rejected, nothing persisted.
- **Clip file missing at trigger** (user deleted app data manually) — error event, entry flagged in UI with a broken icon, playable again after re-import.
- **Trigger while pipeline is Stopped** — rejected with "start the microphone first" error; clips require the output stream.
- **Very long clip (e.g. a song)** — allowed, streamed; the playing indicator shows progress and the clip can be stopped.
- **Drag-and-drop onto the window** — same path as `soundboard_import`.

## UI Notes

- Grid of buttons (name + duration), click to play, click again to retrigger.
- Right-click or kebab menu: rename, volume slider, delete.
- Visible progress bar on playing clips; global stop button always reachable.
- TODO: global hotkeys per clip (phase 2+, needs a Tauri global-shortcut plugin and conflict handling).
