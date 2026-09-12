mod authentication;
mod commands;
mod error;
mod events;
mod menu;
mod opening;
mod state;
mod streams;
mod transfer_grants;
use tauri::Manager;

pub fn run() {
    let context = tauri::generate_context!();
    #[cfg(feature = "e2e")]
    let context = {
        let mut context = context;
        // Native rendering assertions must also run when another app covers the test window.
        for window in &mut context.config_mut().app.windows {
            window.background_throttling =
                Some(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
        }
        context
    };
    let builder = tauri::Builder::default();
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());
    let application = builder
        .manage(state::AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .on_webview_event(transfer_grants::dropped)
        .setup(|app| {
            menu::install(app)?;
            Ok(())
        })
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.webview_windows().into_values().next() {
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![
            commands::catalog::attach,
            commands::transfer_selection::transfer_pick,
            commands::transfer_selection::transfer_upload,
            commands::transfer_selection::transfer_download,
            commands::transfer_selection::transfer_discard_grant,
            commands::transfers::transfer_list,
            commands::transfers::transfer_remove,
            commands::transfers::transfer_run,
            commands::transfers::transfer_action,
            commands::transfers::transfer_skipped,
            commands::transfers::transfer_reveal,
            commands::authentication::server_authentication,
            commands::catalog::catalog,
            commands::catalog::server_save,
            commands::catalog::server_remove,
            commands::catalog::server_config,
            commands::catalog::workspace_open,
            commands::catalog::workspace_remove,
            commands::catalog::workspace_close,
            commands::directories::directory_open,
            commands::directories::directory_browse,
            commands::directories::directory_close,
            commands::directories::directory_create,
            commands::file_contexts::file_context_open,
            commands::file_contexts::file_context_close,
            commands::files::file_list,
            commands::files::file_read,
            commands::files::file_write,
            commands::files::file_change,
            commands::files::project_mcp,
            commands::terminals::terminal_open,
            commands::terminals::terminal_input,
            commands::terminals::terminal_resize,
            commands::terminals::terminal_close,
            commands::native::native_status,
            commands::native::native_usage,
            commands::native::session_defaults,
            commands::app_preferences::app_preferences,
            commands::native::native_login,
            commands::native::native_select,
            commands::session::session_open,
            commands::session::session_trust,
            commands::session_controls::session_action,
            commands::session_controls::session_history,
            commands::session_controls::session_close,
            commands::session_controls::session_metadata,
            commands::session_controls::session_revert,
            commands::session_controls::session_mcp,
            commands::session_controls::session_snapshot,
            commands::session_controls::session_status,
            commands::window::preferences,
            commands::window::acknowledge,
            commands::window::authentication_answer,
            commands::window::cancel_operation,
            commands::editor_windows::editor_window_ready,
            commands::editor_windows::editor_window_cancel,
            commands::window::new_window,
            commands::window::close_window,
            commands::window::external_link
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                commands::window::destroyed(window.app_handle(), window.label());
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && let Some(webview) = window.app_handle().get_webview_window(window.label())
                && commands::window::request_close(&webview)
            {
                api.prevent_close();
            }
        })
        .build(context);
    match application {
        Ok(app) => app.run(|app, event| {
            if let tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                let state = app.state::<state::AppState>();
                if let Ok(windows) = state.windows.lock()
                    && !windows.is_empty()
                {
                    api.prevent_exit();
                    state
                        .quitting
                        .store(true, std::sync::atomic::Ordering::Release);
                    for window in windows.values() {
                        window.send(state::Event::CloseRequested);
                    }
                }
            }
        }),
        Err(error) => {
            eprintln!("Remote Codex could not start: {error}");
            std::process::exit(1);
        }
    }
}
