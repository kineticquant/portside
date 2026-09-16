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
    /// "127.0.0.1" (local) or "0.0.0.0" (LAN). V1 allows only these two.
    pub bind_ip: String,
    /// User-set at creation (engine default when empty at the API boundary,
    /// resolved to the default before storage). Stored plaintext in SQLite.
    pub password: String,
}

impl Instance {
    pub fn new(id: &str, engine: &str, tag: &str, port: u16, bind_ip: &str, password: &str) -> Self {
        Self {
            id: id.to_string(),
            engine: engine.to_string(),
            tag: tag.to_string(),
            port,
            container: format!("portside-{id}"),
            volume: format!("portside-{id}-data"),
            status: "created".to_string(),
            bind_ip: bind_ip.to_string(),
            password: password.to_string(),
        }
    }

    /// LAN-bound instances serve TLS; localhost stays plaintext (pgAdmin posture).
    pub fn tls(&self) -> bool {
        self.bind_ip != "127.0.0.1" && self.bind_ip != "localhost"
    }

    /// Dedicated volume holding server.crt/server.key with correct Unix
    /// ownership. Needed because Windows bind mounts cannot carry the
    /// permissions postgres demands for its key file.
    pub fn cert_volume(&self) -> String {
        format!("portside-{}-certs", self.id)
    }
}

/// Copy host certs into the instance's cert volume with server-safe
/// ownership (600 key / 644 cert, owned by the engine's service user) via a
/// short-lived helper container running the instance's own (already pulled)
/// image as root.
async fn populate_cert_volume(
    docker: &Docker,
    inst: &Instance,
    cert_file: &str,
    key_file: &str,
    image: &str,
) -> Result<(), String> {
    let cert_vol = inst.cert_volume();
    docker
        .create_volume(CreateVolumeOptions {
            name: cert_vol.clone(),
            labels: labels_for(&inst.id),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("cert volume create failed ({cert_vol}): {e}"))?;

    let src_path = std::path::Path::new(cert_file);
    let src_dir = src_path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Host files are named {id}.crt/{id}.key (multi-instance dir); the
    // server always reads /certs/server.crt + /certs/server.key.
    let crt_name = src_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let key_name = std::path::Path::new(key_file)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let svc = match inst.engine.as_str() {
        "mysql" | "mariadb" => "mysql",
        "redis" | "valkey" => "redis",
        _ => "postgres",
    };
    let script = format!(
        "cp /src/{crt_name} /certs/server.crt && cp /src/{key_name} /certs/server.key \
         && chmod 644 /certs/server.crt && chmod 600 /certs/server.key \
         && chown {svc}:{svc} /certs/server.crt /certs/server.key"
    );
    let helper = format!("portside-{}-certinit", inst.id);
    let created = docker
        .create_container(
            Some(CreateContainerOptions {
                name: helper.clone(),
                platform: None,
            }),
            Config::<String> {
                image: Some(image.to_string()),
                cmd: Some(vec!["sh".to_string(), "-c".to_string(), script]),
                host_config: Some(HostConfig {
                    binds: Some(vec![
                        format!("{cert_vol}:/certs"),
                        format!("{src_dir}:/src:ro"),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("cert helper create failed: {e}"))?;
    let helper_name = created.id.clone();
    // The helper must go away on EVERY path past this point (a lingering
    // exited helper pins the cert volume). Run first, remove always.
    let run: Result<i64, String> = async {
        docker
            .start_container(&helper_name, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| format!("cert helper start failed: {e}"))?;
        // Poll inspect until the helper exits (avoids wait-API quirks).
        for _ in 0..120 {
            let info = docker
                .inspect_container(&helper_name, None)
                .await
                .map_err(|e| format!("cert helper inspect failed: {e}"))?;
            let running = info
                .state
                .as_ref()
                .and_then(|s| s.running)
                .unwrap_or(false);
            if !running {
                return Ok(info
                    .state
                    .as_ref()
                    .and_then(|s| s.exit_code)
                    .unwrap_or(-1));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err("cert helper timed out after 60s".to_string())
    }
    .await;
    docker
        .remove_container(
            &helper_name,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| format!("cert helper remove failed: {e}"))?;
    match run {
        Ok(0) => Ok(()),
        Ok(code) => Err(format!("cert helper exited with code {code}")),
        Err(e) => Err(e),
    }
}

fn labels_for(id: &str) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    labels.insert("app".to_string(), "portside".to_string());
    labels.insert("portside.id".to_string(), id.to_string());
    labels
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

fn env_for_engine(engine: &str, password: &str) -> Vec<String> {
    match engine {
        "mysql" | "mariadb" => vec![format!("MYSQL_ROOT_PASSWORD={password}")],
        "postgres" => vec![format!("POSTGRES_PASSWORD={password}")],
        _ => vec![],
    }
}

/// Extra server command args. SQL engines take TLS cert flags; redis/valkey
/// switch the mapped port to TLS-only when the instance is LAN-bound.
fn cmd_for_engine(engine: &str, tls: bool, password: &str) -> Option<Vec<String>> {
    if !tls {
        // Plaintext localhost: only redis needs a password flag.
        return match engine {
            "redis" | "valkey" if !password.is_empty() => {
                Some(vec!["redis-server".to_string(), "--requirepass".to_string(), password.to_string()])
            }
            _ => None,
        };
    }
    match engine {
        "postgres" => Some(vec![
            "postgres".to_string(),
            "-c".to_string(), "ssl=on".to_string(),
            "-c".to_string(), "ssl_cert_file=/certs/server.crt".to_string(),
            "-c".to_string(), "ssl_key_file=/certs/server.key".to_string(),
        ]),
        "mysql" | "mariadb" => Some(vec![
            "--require-secure-transport=ON".to_string(),
            "--ssl-cert=/certs/server.crt".to_string(),
            "--ssl-key=/certs/server.key".to_string(),
        ]),
        // NOTE: valkey images accept the same redis-server flags.
        "redis" | "valkey" => {
            let mut args = vec![
                "redis-server".to_string(),
                "--port".to_string(), "0".to_string(),
                "--tls-port".to_string(), "6379".to_string(),
                "--tls-cert-file".to_string(), "/certs/server.crt".to_string(),
                "--tls-key-file".to_string(), "/certs/server.key".to_string(),
                "--tls-ca-cert-file".to_string(), "/certs/server.crt".to_string(),
                // Default posture is encryption without client certs: with a
                // CA configured, redis would otherwise REQUIRE them.
                "--tls-auth-clients".to_string(), "no".to_string(),
            ];
            if !password.is_empty() {
                args.push("--requirepass".to_string());
                args.push(password.to_string());
            }
            Some(args)
        }
        _ => None,
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
    bind_ip: Option<String>,
    password: Option<String>,
) -> Result<Instance, String> {
    check_engine(&engine)?;
    if tag.is_empty() {
        return Err("tag must not be empty".to_string());
    }
    if port == 0 {
        return Err("port must be non-zero".to_string());
    }
    let bind_ip = bind_ip.unwrap_or_else(|| "127.0.0.1".to_string());
    if bind_ip != "127.0.0.1" && bind_ip != "0.0.0.0" {
        return Err("bind_ip must be 127.0.0.1 (local) or 0.0.0.0 (LAN)".to_string());
    }
    let password =
        crate::certs::check_password(password.as_deref().unwrap_or(""), &engine)?;
    let short_id: String = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    let mut inst = Instance::new(&short_id, &engine, &tag, port, &bind_ip, &password);
    let docker = connect()?;

    pull_image(&docker, &engine, &tag).await?;

    let labels = labels_for(&inst.id);
    docker
        .create_volume(CreateVolumeOptions {
            name: inst.volume.clone(),
            labels: labels.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("volume create failed ({}): {e}", inst.volume))?;
    if inst.tls() {
        let (cert_file, key_file) = crate::certs::ensure_cert(&inst.id)?;
        let image = image_name(&engine, &tag);
        if let Err(e) = populate_cert_volume(&docker, &inst, &cert_file, &key_file, &image).await {
            // Best-effort rollback: no row saved yet, drop the volumes.
            let _ = docker.remove_volume(&inst.cert_volume(), None).await;
            let _ = docker.remove_volume(&inst.volume, None).await;
            return Err(e);
        }
    }

    let container_port = engine_port(&engine);
    let binding_key = format!("{container_port}/tcp");
    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        binding_key.clone(),
        Some(vec![PortBinding {
            host_ip: Some(inst.bind_ip.clone()),
            host_port: Some(port.to_string()),
        }]),
    );
    let mut exposed = HashMap::new();
    exposed.insert(binding_key, HashMap::new());
    let mut binds = vec![format!(
        "{}:{}",
        inst.volume,
        data_dir_for_engine(&engine)
    )];
    if inst.tls() {
        binds.push(format!("{}:/certs:ro", inst.cert_volume()));
    }
    let host_config = HostConfig {
        binds: Some(binds),
        port_bindings: Some(port_bindings),
        ..Default::default()
    };
    let config = Config::<String> {
        image: Some(image_name(&engine, &tag)),
        env: Some(env_for_engine(&engine, &inst.password)),
        exposed_ports: Some(exposed),
        host_config: Some(host_config),
        cmd: cmd_for_engine(&engine, inst.tls(), &inst.password),
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
    if let Err(e) = docker
        .start_container(&inst.container, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| format!("container start failed ({}): {e}", inst.container))
    {
        // Rollback: no row saved yet - drop container + volumes so a failed
        // create (e.g. forbidden port) leaves nothing behind.
        let _ = docker
            .remove_container(
                &inst.container,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;
        let _ = docker.remove_volume(&inst.cert_volume(), None).await;
        let _ = docker.remove_volume(&inst.volume, None).await;
        return Err(e);
    }

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
        // Cert volume is derived from the id; absent for localhost instances.
        let _ = docker.remove_volume(&inst.cert_volume(), None).await;
        // Host cert files too - best effort, never fail the wipe for these.
        if let Ok(dir) = std::fs::read_dir(crate::state::data_dir().join("certs")) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == format!("{id}.crt") || name == format!("{id}.key") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
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
        let inst = Instance::new("abc123", "postgres", "17", 5433, "127.0.0.1", "portside");
        assert_eq!(inst.container, "portside-abc123");
        assert_eq!(inst.volume, "portside-abc123-data");
        assert!(!inst.tls());
    }

    #[test]
    fn lan_bound_instance_serves_tls() {
        let inst = Instance::new("abc123", "postgres", "17", 5433, "0.0.0.0", "s3cret!");
        assert!(inst.tls());
        let cmd = cmd_for_engine("postgres", true, "s3cret!").expect("tls cmd");
        assert!(cmd.contains(&"ssl=on".to_string()));
    }
}
