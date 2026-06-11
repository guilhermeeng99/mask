//! Tauri command handlers: thin adapters over the modules (CLAUDE.md rule).
//! Every command returns `Result<T, String>` with a user-presentable message.

use tauri::{AppHandle, Emitter, State};

use crate::audio::pipeline::{self, PipelineConfig};
use crate::audio::virtual_mic::{detect_virtual_mic, VirtualMicStatus};
use crate::audio::{self, AudioDevice};
use crate::dsp::DspPreset;
use crate::soundboard::SoundClip;
use crate::state::{AppConfig, AppState};

pub(crate) fn now_iso() -> String {
    // RFC 3339 from the system clock without pulling a chrono dependency.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let days = secs / 86_400;
            let (y, m, dd) = civil_from_days(days as i64);
            let (h, mi, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
            format!("{y:04}-{m:02}-{dd:02}T{h:02}:{mi:02}:{s:02}Z")
        })
        .unwrap_or_default()
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// --- Audio / pipeline -------------------------------------------------------

#[tauri::command]
pub fn audio_list_devices() -> Vec<AudioDevice> {
    audio::list_devices()
}

#[tauri::command]
pub fn virtual_mic_status() -> VirtualMicStatus {
    // Devnode check tells "blocked driver" (e.g. signature rejected, code 52)
    // apart from "nothing installed" — virtual_mic_setup.md rule 9.
    let devnode = crate::driver_devnode::query_devnode(crate::driver_install::HARDWARE_ID);
    detect_virtual_mic(&audio::list_devices(), devnode.problem_code)
}

#[tauri::command]
pub fn pipeline_start(
    app: AppHandle,
    state: State<'_, AppState>,
    config: PipelineConfig,
) -> Result<(), String> {
    let mut pipeline = state.pipeline.lock();
    if let Some(handle) = pipeline.as_mut() {
        handle.stop();
    }
    *pipeline = None;

    let preset = {
        let cfg = state.config.lock();
        cfg.find_preset(&cfg.active_preset_id)
            .unwrap_or_else(|| crate::dsp::builtin_presets().remove(0))
    };

    let event_app = app.clone();
    let handle = pipeline::start(
        config.clone(),
        preset,
        state.dsp_params.clone(),
        move |event| match event {
            pipeline::EngineEvent::VcFallback(reason) => {
                let _ = event_app.emit(
                    "vc://state",
                    serde_json::json!({ "state": "fallback", "reason": reason }),
                );
            }
        },
    )
    .map_err(|e| e.to_string())?;
    *pipeline = Some(handle);

    {
        let mut cfg = state.config.lock();
        cfg.input_device_id = Some(config.input_device_id);
        cfg.output_device_id = Some(config.output_device_id);
        cfg.monitor_device_id = config.monitor_device_id;
        cfg.monitor_enabled = cfg.monitor_device_id.is_some();
    }
    state.save_config();
    let _ = app.emit(
        "pipeline://state",
        serde_json::json!({ "state": "running" }),
    );
    Ok(())
}

#[tauri::command]
pub fn pipeline_stop(app: AppHandle, state: State<'_, AppState>) {
    if let Some(mut handle) = state.pipeline.lock().take() {
        handle.stop();
    }
    // The inference worker has no audio to feed once the engine is gone.
    let mut worker = state.vc_worker.lock();
    if let Some(w) = worker.as_mut() {
        w.stop();
    }
    *worker = None;
    let _ = app.emit(
        "pipeline://state",
        serde_json::json!({ "state": "stopped" }),
    );
}

// --- DSP ---------------------------------------------------------------------

#[tauri::command]
pub fn dsp_list_presets(state: State<'_, AppState>) -> Vec<DspPreset> {
    state.config.lock().all_presets()
}

#[tauri::command]
pub fn dsp_set_preset(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let preset = state
        .config
        .lock()
        .find_preset(&id)
        .ok_or_else(|| format!("preset not found: {id}"))?;

    if let Some(handle) = state.pipeline.lock().as_ref() {
        handle
            .ctrl
            .send(pipeline::CtrlMsg::SetPreset(Box::new(preset.clone())))
            .map_err(|_| "audio engine is not responding".to_string())?;
    }
    state.dsp_params.set(preset.pitch, preset.formant);
    state.config.lock().active_preset_id = id;
    state.save_config();
    Ok(())
}

#[tauri::command]
pub fn dsp_set_params(state: State<'_, AppState>, pitch: f32, formant: f32) {
    state.dsp_params.set(pitch, formant);
}

#[tauri::command]
pub fn dsp_save_preset(state: State<'_, AppState>, preset: DspPreset) -> Result<DspPreset, String> {
    let saved = DspPreset {
        id: uuid::Uuid::new_v4().to_string(),
        is_builtin: false,
        pitch: preset.pitch.clamp(-12.0, 12.0),
        formant: preset.formant.clamp(-12.0, 12.0),
        ..preset
    };
    state.config.lock().user_presets.push(saved.clone());
    state.save_config();
    Ok(saved)
}

#[tauri::command]
pub fn dsp_delete_preset(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut cfg = state.config.lock();
    if cfg.all_presets().iter().any(|p| p.id == id && p.is_builtin) {
        return Err("builtin presets cannot be deleted".into());
    }
    let before = cfg.user_presets.len();
    cfg.user_presets.retain(|p| p.id != id);
    if cfg.user_presets.len() == before {
        return Err(format!("preset not found: {id}"));
    }
    // Deleting the active preset falls back to Clean (spec edge case).
    let was_active = cfg.active_preset_id == id;
    if was_active {
        cfg.active_preset_id = "none".into();
    }
    drop(cfg);
    if was_active {
        let _ = dsp_set_preset(state.clone(), "none".into());
    }
    state.save_config();
    Ok(())
}

// --- Soundboard ----------------------------------------------------------------

#[tauri::command]
pub fn soundboard_list(state: State<'_, AppState>) -> Vec<SoundClip> {
    state.store.lock().list()
}

#[tauri::command]
pub fn soundboard_import(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<SoundClip>, String> {
    let mut store = state.store.lock();
    let mut imported = Vec::new();
    for path in paths {
        let clip = store
            .import(std::path::Path::new(&path), now_iso())
            .map_err(|e| format!("{path}: {e}"))?;
        imported.push(clip);
    }
    Ok(imported)
}

#[tauri::command]
pub fn soundboard_play(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let pipeline = state.pipeline.lock();
    let handle = pipeline
        .as_ref()
        .ok_or("start the microphone first".to_string())?;
    // Decode on this (command) thread; only samples cross to the engine.
    let decoded = state.store.lock().decode(&id).map_err(|e| e.to_string())?;
    handle
        .ctrl
        .send(pipeline::CtrlMsg::TriggerClip(decoded))
        .map_err(|_| "audio engine is not responding".to_string())
}

#[tauri::command]
pub fn soundboard_stop(state: State<'_, AppState>, id: String) {
    if let Some(handle) = state.pipeline.lock().as_ref() {
        let _ = handle.ctrl.send(pipeline::CtrlMsg::StopClip(id));
    }
}

#[tauri::command]
pub fn soundboard_stop_all(state: State<'_, AppState>) {
    if let Some(handle) = state.pipeline.lock().as_ref() {
        let _ = handle.ctrl.send(pipeline::CtrlMsg::StopAllClips);
    }
}

#[tauri::command]
pub fn soundboard_update(state: State<'_, AppState>, clip: SoundClip) -> Result<(), String> {
    state.store.lock().update(clip).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn soundboard_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.store.lock().remove(&id).map_err(|e| e.to_string())
}

// --- Config ------------------------------------------------------------------

#[tauri::command]
pub fn config_get(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().clone()
}

#[tauri::command]
pub fn onboarding_complete(state: State<'_, AppState>) {
    state.config.lock().onboarding_done = true;
    state.save_config();
}
