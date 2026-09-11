use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding};
use bollard::volume::CreateVolumeOptions;
use bollard::{API_DEFAULT_VERSION, Docker};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub engine: String,
    pub tag: String,
    pub port: u16,
    pub container: String,
    pub volume: String,
    pub status: String,
}

impl Instance {
    pub fn new(id: &str, engine: &str, tag: &str, port: u16) -> Self {
        Self {
            id: id.to_string(),
            engine: engine.to_string(),
            tag: tag.to_string(),
            port,
            container: format!("portside-{id}"),
            volume: format!("portside-{id}-data"),
            status: "created".to_string(),
        }
    }
}

pub fn engine_port(engine: &str) -> u16 {
    match engine {
        "postgres" => 5432,
        "redis" | "valkey" => 6379,
        _ => 3306,
    }
}

pub fn image_repo(engine: &str) -> &str {
    match engine {
        "valkey" => "valkey/valkey",
        _ => engine,
    }
}

pub fn image_name(engine: &str, tag: &str) -> String {
    format!("{}:{tag}", image_repo(engine))
}

fn data_dir_for_engine(engine: &str) -> &'static str {
    match engine {
        "mysql" | "mariadb" => "/var/lib/mysql",
        "postgres" => "/var/lib/postgresql/data",
        _ => "/data",
    }
}

fn env_for_engine(engine: &str) -> Vec<String> {
    match engine {
        "mysql" | "mariadb" => vec!["MYSQL_ROOT_PASSWORD=portside".to_string()],
        "postgres" => vec!["POSTGRES_PASSWORD=portside".to_string()],
        _ => vec![],
    }
}

fn check_engine(engine: &str) -> Result<(), String> {
    match engine {
        "mysql" | "mariadb" | "postgres" | "redis" | "valkey" => Ok(()),
        other => Err(format!(
            "unsupported engine {other} (expected mysql, mariadb, postgres, redis or valkey)"
        )),
    }
}

pub fn connect() -> Result<Docker, String> {
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        return Docker::connect_with_http(&host, 5, API_DEFAULT_VERSION)
            .map_err(|e| format!("Docker connect failed ({host}): {e}"));
    }
    #[cfg(windows)]
    {
        Docker::connect_with_socket("//./pipe/docker_engine", 5, API_DEFAULT_VERSION)
            .or_else(|_| {
                Docker::connect_with_http("tcp://127.0.0.1:2375", 5, API_DEFAULT_VERSION)
            })
            .map_err(|e| format!("Docker not reachable. Is Docker Desktop running? {e}"))
    }
    #[cfg(not(windows))]
    {
        Docker::connect_with_socket_defaults()
            .map_err(|e| format!("Docker not reachable. Is Docker Desktop/Podman running? {e}"))
    }
}

#[tauri::command]
pub async fn detect_runtime() -> Result<String, String> {
    let docker = connect()?;
    let ver = docker
        .version()
        .await
        .map_err(|e| format!("Docker ping failed: {e}"))?;
    Ok(format!(
        "runtime ok (docker {}, api {})",
        ver.version.unwrap_or_default(),
        ver.api_version.unwrap_or_default()
    ))
}

async fn pull_image(docker: &Docker, engine: &str, tag: &str) -> Result<(), String> {
    let repo = image_repo(engine);
    let opts = CreateImageOptions {
        from_image: repo,
        tag,
        ..Default::default()
    };
    docker
        .create_image(Some(opts), None, None)
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("image pull failed ({}:{tag}): {e}", repo))?;
    Ok(())
}

#[tauri::command]
pub async fn create_instance(
    engine: String,
    tag: String,
    port: u16,
) -> Result<Instance, String> {
    check_engine(&engine)?;
    if tag.is_empty() {
        return Err("tag must not be empty".to_string());
    }
    if port == 0 {
        return Err("port must be non-zero".to_string());
    }
    let short_id: String = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    let mut inst = Instance::new(&short_id, &engine, &tag, port);
    let docker = connect()?;

    pull_image(&docker, &engine, &tag).await?;

    let mut labels = HashMap::new();
    labels.insert("app".to_string(), "portside".to_string());
    labels.insert("portside.id".to_string(), inst.id.clone());
    docker
        .create_volume(CreateVolumeOptions {
            name: inst.volume.clone(),
            labels: labels.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("volume create failed ({}): {e}", inst.volume))?;

    let container_port = engine_port(&engine);
    let binding_key = format!("{container_port}/tcp");
    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        binding_key.clone(),
        Some(vec![PortBinding {
            host_ip: Some("127.0.0.1".to_string()),
            host_port: Some(port.to_string()),
        }]),
    );
    let mut exposed = HashMap::new();
    exposed.insert(binding_key, HashMap::new());
    let host_config = HostConfig {
        binds: Some(vec![format!(
            "{}:{}",
            inst.volume,
            data_dir_for_engine(&engine)
        )]),
        port_bindings: Some(port_bindings),
        ..Default::default()
    };
    let config = Config::<String> {
        image: Some(image_name(&engine, &tag)),
        env: Some(env_for_engine(&engine)),
        exposed_ports: Some(exposed),
        host_config: Some(host_config),
        labels: Some(labels),
        ..Default::default()
    };
    docker
        .create_container(
            Some(CreateContainerOptions {
                name: inst.container.clone(),
                platform: None,
            }),
            config,
        )
        .await
        .map_err(|e| format!("container create failed ({}): {e}", inst.container))?;
    docker
        .start_container(&inst.container, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| format!("container start failed ({}): {e}", inst.container))?;

    inst.status = "running".to_string();
    crate::state::save_instance(&inst)?;
    Ok(inst)
}

#[tauri::command]
pub async fn start_instance(id: String) -> Result<(), String> {
    let inst = crate::state::load_instance(&id)?;
    let docker = connect()?;
    docker
        .start_container(&inst.container, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| format!("container start failed ({}): {e}", inst.container))?;
    crate::state::update_status(&id, "running")?;
    Ok(())
}

#[tauri::command]
pub async fn stop_instance(id: String) -> Result<(), String> {
    let inst = crate::state::load_instance(&id)?;
    let docker = connect()?;
    docker
        .stop_container(&inst.container, Some(StopContainerOptions { t: 10 }))
        .await
        .map_err(|e| format!("container stop failed ({}): {e}", inst.container))?;
    crate::state::update_status(&id, "stopped")?;
    Ok(())
}

#[tauri::command]
pub async fn remove_instance(id: String, wipe_data: bool) -> Result<(), String> {
    let inst = crate::state::load_instance(&id)?;
    let docker = connect()?;
    docker
        .remove_container(
            &inst.container,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("container remove failed ({}): {e}", inst.container))?;
    if wipe_data {
        docker
            .remove_volume(&inst.volume, None)
            .await
            .map_err(|e| format!("volume remove failed ({}): {e}", inst.volume))?;
    }
    crate::state::delete_instance(&id)?;
    Ok(())
}

#[tauri::command]
pub async fn list_instances() -> Result<Vec<Instance>, String> {
    let mut instances = crate::state::list_saved_instances()?;
    let docker = match connect() {
        Ok(d) => d,
        Err(_) => return Ok(instances),
    };
    let mut filters = HashMap::new();
    filters.insert("label".to_string(), vec!["app=portside".to_string()]);
    let containers = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            filters,
            ..Default::default()
        }))
        .await
        .map_err(|e| format!("container list failed: {e}"))?;
    let by_name: HashMap<String, String> = containers
        .into_iter()
        .filter_map(|c| {
            let names = c.names?;
            let state = c.state.unwrap_or_default();
            names
                .into_iter()
                .next()
                .map(|n| (n.trim_start_matches('/').to_string(), state.clone()))
        })
        .collect();
    for inst in instances.iter_mut() {
        if let Some(state) = by_name.get(&inst.container) {
            let status = if state == "running" {
                "running"
            } else {
                "stopped"
            };
            if inst.status != status {
                inst.status = status.to_string();
                crate::state::update_status(&inst.id, status)?;
            }
        }
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn container_name_is_prefixed() {
        let inst = Instance::new("abc123", "postgres", "17", 5433);
        assert_eq!(inst.container, "portside-abc123");
        assert_eq!(inst.volume, "portside-abc123-data");
    }
}
