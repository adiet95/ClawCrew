//! Mobile entry point for ClawCrew Desktop (iOS/Android).

#[tauri::mobile_entry_point]
fn main() {
    clawcrew_desktop::run();
}
