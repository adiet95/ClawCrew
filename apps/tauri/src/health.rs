//! Background health polling for the ClawCrew gateway.

use crate::gateway_client::GatewayClient;
use crate::state::SharedState;
use crate::tray::icon;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Spawn a background task that polls gateway health and updates state + tray.
pub fn spawn_health_poller<R: Runtime>(app: AppHandle<R>, state: SharedState) {
    tauri::async_runtime::spawn(async move {
        loop {
            let (url, token) = {
                let s = state.read().await;
                (s.gateway_url.clone(), s.token.clone())
            };

            let client = GatewayClient::new(&url, token.as_deref());
            let healthy = client.get_health().await.unwrap_or(false);

            let (connected, agent_status, service_enabled) = {
                let mut s = state.write().await;
                s.connected = healthy;
                (s.connected, s.agent_status, s.service_enabled)
            };

            // Update the tray icon and tooltip to reflect current state.
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_icon(Some(icon::icon_for_state(connected, agent_status)));
                let _ = tray.set_tooltip(Some(icon::tooltip_for_state(connected, agent_status)));
            }

            // Keep the service-toggle and status menu items in sync with live state.
            crate::tray::sync_service_menu(service_enabled, connected);

            let _ = app.emit("clawcrew://status-changed", healthy);

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}
