#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod docker;
mod state;

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            docker::detect_runtime,
            docker::create_instance,
            docker::start_instance,
            docker::stop_instance,
            docker::remove_instance,
            docker::list_instances
        ])
        .run(tauri::generate_context!())
        .expect("failed to run portside");
}
