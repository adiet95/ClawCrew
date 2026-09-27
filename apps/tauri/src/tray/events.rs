//! Tray menu event handling.

use tauri::{AppHandle, Manager, Runtime, menu::MenuEvent};

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        "show" => show_main_window(app, None),
        "browser" => {
            let state = app.state::<crate::state::SharedState>().inner().clone();
            tauri::async_runtime::spawn(async move {
                let url = state.read().await.gateway_url.clone();
                open_browser(&url);
            });
        }
        "chat" => show_main_window(app, Some("/agent")),
        "service-toggle" => {
            let state = app.state::<crate::state::SharedState>().inner().clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::toggle_service(app.clone(), state.clone()).await;
                let (service_enabled, connected) = {
                    let s = state.read().await;
                    (s.service_enabled, s.connected)
                };
                crate::tray::sync_service_menu(service_enabled, connected);
            });
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

fn open_browser(url: &str) {
    let mut command = std::process::Command::new(if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    });
    command.arg(url);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::null());
    command.stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let _ = command.spawn();
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>, navigate_to: Option<&str>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(path) = navigate_to {
            let script = format!("window.location.hash = '{path}'");
            let _ = window.eval(&script);
        }
    }
}
