mod audio;
mod commands;
mod dsp;
mod soundboard;
mod state;

use std::time::Duration;

use tauri::{Emitter, Manager};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Second instance: focus the existing window and exit (app_shell edge case).
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let app_state = AppState::new(config_dir, data_dir).map_err(std::io::Error::other)?;
            app.manage(app_state);
            spawn_metrics_emitter(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio_list_devices,
            commands::virtual_mic_status,
            commands::pipeline_start,
            commands::pipeline_stop,
            commands::dsp_list_presets,
            commands::dsp_set_preset,
            commands::dsp_set_params,
            commands::dsp_save_preset,
            commands::dsp_delete_preset,
            commands::soundboard_list,
            commands::soundboard_import,
            commands::soundboard_play,
            commands::soundboard_stop,
            commands::soundboard_stop_all,
            commands::soundboard_update,
            commands::soundboard_delete,
            commands::config_get,
            commands::onboarding_complete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Push metrics + playing-clip events to the UI every 100 ms while the
/// pipeline runs (audio_pipeline rule 7). Reads atomics only.
fn spawn_metrics_emitter(app: tauri::AppHandle) {
    std::thread::Builder::new()
        .name("mask-metrics".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let snapshot = {
                let pipeline = state.pipeline.lock();
                pipeline
                    .as_ref()
                    .map(|h| (h.shared.metrics(), h.shared.playing.lock().clone()))
            };
            if let Some((metrics, playing)) = snapshot {
                let _ = app.emit("pipeline://metrics", &metrics);
                let _ = app.emit("soundboard://playing", &playing);
            }
        })
        .expect("metrics thread");
}
