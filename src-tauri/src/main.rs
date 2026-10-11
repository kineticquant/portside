#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use portside::{agents, catalog, certs, docker, health, import, prereqs, schemas};

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
fn setup_agent(host: String, global: bool) -> Result<agents::SetupResult, String> {
    let host: agents::Host = host.parse()?;
    // The desktop app has no project-dir concept: project scope is CLI-only.
    let project: Option<&std::path::Path> = None;
    if !global {
        return Err("project scope needs a directory (CLI: setup-agent --project-dir)".to_string());
    }
    agents::setup_host(host, project)
}

#[tauri::command]
fn setup_path(remove: bool) -> Result<String, String> {
    if remove {
        agents::remove_from_path()
    } else {
        agents::add_to_path()
    }
}

#[tauri::command]
fn path_status() -> agents::PathStatus {
    agents::path_status()
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
            certs::lan_addr,
            certs::server_cert,
            schemas::list_databases,
            schemas::create_database,
            schemas::drop_database,
            schemas::list_schemas,
            schemas::redis_info,
            health::container_logs,
            health::health,
            import::probe_connection,
            import::import_external,
            import::list_adoptable,
            import::adopt_container,
            import::list_pgadmin_servers,
            import::forget_instance,
            prereqs::check_prereqs,
            prereqs::install_wsl,
            prereqs::start_docker,
            setup_agent,
            setup_path,
            path_status,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run portside");
}
