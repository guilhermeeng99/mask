pub mod audio;
mod commands;
mod commands_vc;
pub mod dsp;
pub mod soundboard;
mod state;
pub mod vc;

use std::time::Duration;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Voice changers keep running during calls: closing the window
            // hides to tray instead of quitting (roadmap item 11).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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
            commands_vc::vc_backend_info,
            commands_vc::vc_list_models,
            commands_vc::vc_import_model,
            commands_vc::vc_import_cloned_voice,
            commands_vc::vc_delete_model,
            commands_vc::vc_download_companions,
            commands_vc::vc_activate,
            commands_vc::vc_deactivate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// System tray: show window / quit. The window close button hides to tray so
/// the voice keeps working mid-call.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Mask", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
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
