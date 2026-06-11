//! AI voice conversion commands (phase 2) and the consent-gated clone import
//! (phase 3, guide-only path). Contracts: docs/specs/ai_voice_conversion.md,
//! docs/specs/voice_cloning.md.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::audio::pipeline::CtrlMsg;
use crate::state::AppState;
use crate::vc::engine::{detect_backend, spawn_worker, validate_onnx, RvcSession};
use crate::vc::{
    companion_paths, companions_installed, download, InferenceBackend, VcModel, VcSettings,
};

fn now_iso() -> String {
    crate::commands::now_iso()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendInfo {
    pub backend: InferenceBackend,
    pub companions_installed: bool,
}

#[tauri::command]
pub fn vc_backend_info(state: State<'_, AppState>) -> BackendInfo {
    BackendInfo {
        backend: detect_backend(),
        companions_installed: companions_installed(&state.data_dir),
    }
}

#[tauri::command]
pub fn vc_list_models(state: State<'_, AppState>) -> Vec<VcModel> {
    state.vc_store.lock().list()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn vc_import_model(
    state: State<'_, AppState>,
    onnx: String,
    index: Option<String>,
    name: String,
    default_pitch: i32,
    sample_rate: u32,
    license_note: Option<String>,
) -> Result<VcModel, String> {
    let note = license_note
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "unknown origin, personal use".to_string());
    state
        .vc_store
        .lock()
        .import(
            std::path::Path::new(&onnx),
            index.as_deref().map(std::path::Path::new),
            name,
            default_pitch.clamp(-24, 24),
            sample_rate,
            note,
            now_iso(),
            validate_onnx,
        )
        .map_err(|e| e.to_string())
}

/// Phase 3 (voice_cloning rule 1): importing a personal clone requires the
/// explicit consent confirmation; the license note records it permanently.
#[tauri::command]
pub fn vc_import_cloned_voice(
    state: State<'_, AppState>,
    onnx: String,
    index: Option<String>,
    name: String,
    default_pitch: i32,
    sample_rate: u32,
    consent_confirmed: bool,
) -> Result<VcModel, String> {
    if !consent_confirmed {
        return Err("cloning requires confirming the voice owner's consent".into());
    }
    let note = format!("personal clone, consent confirmed on {}", &now_iso()[..10]);
    state
        .vc_store
        .lock()
        .import(
            std::path::Path::new(&onnx),
            index.as_deref().map(std::path::Path::new),
            name,
            default_pitch.clamp(-24, 24),
            sample_rate,
            note,
            now_iso(),
            validate_onnx,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vc_delete_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut worker = state.vc_worker.lock();
    let active = state.vc_settings.lock().model_id == id;
    if active {
        if let Some(w) = worker.as_mut() {
            w.stop();
        }
        *worker = None;
    }
    state.vc_store.lock().remove(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vc_download_companions(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let data_dir = state.data_dir.clone();
    // Long download: run off the command thread, report via events.
    std::thread::Builder::new()
        .name("mask-vc-download".into())
        .spawn(move || {
            let manifest = download::default_manifest();
            let paths = companion_paths(&data_dir);
            let files = [
                (
                    "contentvec",
                    &manifest.contentvec_url,
                    &manifest.contentvec_sha256,
                    &paths.contentvec,
                ),
                (
                    "rmvpe",
                    &manifest.rmvpe_url,
                    &manifest.rmvpe_sha256,
                    &paths.rmvpe,
                ),
            ];
            for (label, url, sha, dest) in files {
                if dest.exists() {
                    continue;
                }
                let progress_app = app.clone();
                let result = download::download_verified(url, sha, dest, move |done, total| {
                    let _ = progress_app.emit(
                        "vc://companion-progress",
                        serde_json::json!({ "file": label, "downloaded": done, "total": total }),
                    );
                });
                if let Err(e) = result {
                    let _ = app.emit(
                        "vc://state",
                        serde_json::json!({ "state": "inactive", "error": e.to_string() }),
                    );
                    return;
                }
            }
            let _ = app.emit("vc://companions-ready", serde_json::json!({}));
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn vc_activate(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: VcSettings,
) -> Result<(), String> {
    if !companions_installed(&state.data_dir) {
        return Err("companion models are not installed yet".into());
    }
    let model = state
        .vc_store
        .lock()
        .get(&settings.model_id)
        .cloned()
        .map_err(|e| e.to_string())?;

    let pipeline = state.pipeline.lock();
    let handle = pipeline
        .as_ref()
        .ok_or("start the microphone first".to_string())?;

    let _ = app.emit("vc://state", serde_json::json!({ "state": "loading" }));
    let backend = detect_backend();
    let paths = companion_paths(&state.data_dir);
    let session = RvcSession::load(
        &paths.contentvec,
        &paths.rmvpe,
        &model.onnx_path,
        model.sample_rate,
        model.default_pitch + settings.pitch_offset.clamp(-24, 24),
        backend,
    )
    .map_err(|e| {
        let _ = app.emit(
            "vc://state",
            serde_json::json!({ "state": "inactive", "error": e.to_string() }),
        );
        e.to_string()
    })?;

    let error_app = app.clone();
    let (link, worker) = spawn_worker(session, settings.chunk_ms.clamp(80, 1000), move |msg| {
        let _ = error_app.emit(
            "vc://state",
            serde_json::json!({ "state": "fallback", "reason": msg }),
        );
    });

    handle
        .ctrl
        .send(CtrlMsg::SetVcLink(Some(Box::new(link))))
        .map_err(|_| "audio engine is not responding".to_string())?;

    let mut worker_slot = state.vc_worker.lock();
    if let Some(old) = worker_slot.as_mut() {
        old.stop();
    }
    *worker_slot = Some(worker);
    *state.vc_settings.lock() = settings.clone();
    let _ = app.emit(
        "vc://state",
        serde_json::json!({ "state": "active", "modelId": settings.model_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn vc_deactivate(app: AppHandle, state: State<'_, AppState>) {
    if let Some(handle) = state.pipeline.lock().as_ref() {
        let _ = handle.ctrl.send(CtrlMsg::SetVcLink(None));
    }
    let mut worker = state.vc_worker.lock();
    if let Some(w) = worker.as_mut() {
        w.stop();
    }
    *worker = None;
    let _ = app.emit("vc://state", serde_json::json!({ "state": "inactive" }));
}
