#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod docker;
mod health;
mod schemas;
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
            docker::list_instances,
            catalog::get_catalog,
            catalog::suggest_port,
            schemas::list_databases,
            schemas::create_database,
            schemas::drop_database,
            schemas::list_schemas,
            schemas::redis_info,
            health::container_logs,
            health::health,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run portside");
}
