//! System tray integration for ClawCrew Desktop.

pub mod events;
pub mod icon;
pub mod menu;

use std::sync::{Mutex, OnceLock};

use tauri::{
    App, Manager,
    menu::Menu,
    tray::{TrayIcon, TrayIconBuilder, TrayIconEvent},
};

/// The tray's menu, retained so its items can be updated after construction.
/// `TrayIcon` exposes no menu getter in tauri v2, so the menu handle built at
/// setup is stashed here and used by [`sync_service_menu`].
static TRAY_MENU: OnceLock<Mutex<Menu<tauri::Wry>>> = OnceLock::new();

/// Set up the system tray icon and menu.
pub fn setup_tray(app: &App<tauri::Wry>) -> Result<TrayIcon<tauri::Wry>, tauri::Error> {
    let menu = menu::create_tray_menu(app)?;
    let _ = TRAY_MENU.set(Mutex::new(menu.clone()));

    TrayIconBuilder::with_id("main")
        .tooltip("ClawCrew — Disconnected")
        .icon(icon::icon_for_state(false, crate::state::AgentStatus::Idle))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(events::handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button, .. } = event
                && button == tauri::tray::MouseButton::Left
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)
}

/// The label shown on the tray's fixed `status` menu item.
fn status_label(service_enabled: bool, connected: bool) -> &'static str {
    if !service_enabled {
        "Service: stopped"
    } else if !connected {
        "Service: starting…"
    } else {
        "Service: running"
    }
}

/// Bring the tray menu's service-toggle text and `status` item in line with the
/// live shared state. Called at startup, after every toggle, and from the health
/// poller so the menu never drifts from reality.
pub fn sync_service_menu(service_enabled: bool, connected: bool) {
    let Some(slot) = TRAY_MENU.get() else {
        return;
    };
    let Ok(menu) = slot.lock() else {
        return;
    };
    if let Some(item) = menu.get("service-toggle") {
        if let Some(menu_item) = item.as_menuitem() {
            let title = if service_enabled {
                "Stop Service"
            } else {
                "Start Service"
            };
            let _ = menu_item.set_text(title);
        }
    }
    if let Some(item) = menu.get("status") {
        if let Some(menu_item) = item.as_menuitem() {
            let _ = menu_item.set_text(status_label(service_enabled, connected));
        }
    }
}