//! Agent-harness setup: register Portside in the config files of common
//! coding-agent harnesses (opencode, claude, codex, cursor, windsurf, pi,
//! omp), plus user-PATH management so `portside-cli` resolves by name.
//!
//! File formats verified against primary sources (2026-10-11):
//! pi `packages/coding-agent/docs/mcp.md`, omp `docs/mcp-config.md`.
//! Writes merge into existing files; nothing else in them is touched.
//! Existing-but-invalid files are refused, never overwritten.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// OMP-native `$schema` stamp, added when we create an omp file.
pub const OMP_SCHEMA: &str = "https://raw.githubusercontent.com/can1357/oh-my-pi/main/packages/coding-agent/src/config/mcp-schema.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Host {
    Opencode,
    Claude,
    Codex,
    Cursor,
    Windsurf,
    Pi,
    Omp,
    Generic,
}

impl Host {
    pub fn name(&self) -> &'static str {
        match self {
            Host::Opencode => "opencode",
            Host::Claude => "claude",
            Host::Codex => "codex",
            Host::Cursor => "cursor",
            Host::Windsurf => "windsurf",
            Host::Pi => "pi",
            Host::Omp => "omp",
            Host::Generic => "generic",
        }
    }
}

impl std::str::FromStr for Host {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "opencode" => Ok(Host::Opencode),
            "claude" | "claude-code" => Ok(Host::Claude),
            "codex" => Ok(Host::Codex),
            "cursor" => Ok(Host::Cursor),
            "windsurf" => Ok(Host::Windsurf),
            "pi" => Ok(Host::Pi),
            "omp" | "oh-my-pi" => Ok(Host::Omp),
            "generic" => Ok(Host::Generic),
            other => Err(format!(
                "unknown host {other} (expected opencode, claude, codex, cursor, windsurf, pi, omp or generic)"
            )),
        }
    }
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

/// This binary's own path (absolute when launched normally).
pub fn cli_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("portside-cli"))
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "portside-cli.exe"
    } else {
        "portside-cli"
    }
}

/// First `portside-cli` resolvable via PATH, if any.
pub fn cli_on_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(exe_name()))
            .find(|f| f.is_file())
    })
}

/// Command other tools should invoke: bare name when it resolves on PATH
/// (survives reinstalls elsewhere), else our own absolute path.
pub fn command_name() -> String {
    if cli_on_path().is_some() {
        "portside-cli".to_string()
    } else {
        cli_path().to_string_lossy().into_owned()
    }
}

/// argv for harness configs: `[command, "mcp"]`.
pub fn command_argv() -> Vec<String> {
    vec![command_name(), "mcp".to_string()]
}

// ---- user-PATH management ------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct PathStatus {
    pub on_path: bool,
    pub cli_path: String,
    pub dir: String,
}

fn cli_dir() -> Result<PathBuf, String> {
    cli_path()
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "cannot locate own binary directory".to_string())
}

pub fn path_status() -> PathStatus {
    let cli = cli_path();
    let dir = cli
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    PathStatus {
        on_path: persisted_on_path(&dir),
        cli_path: cli.to_string_lossy().into_owned(),
        dir,
    }
}

#[cfg(windows)]
fn persisted_on_path(dir: &str) -> bool {
    match read_user_path() {
        Ok((text, _)) => std::env::split_paths(&text).any(|d| d == Path::new(dir)),
        Err(_) => false,
    }
}

#[cfg(not(windows))]
fn persisted_on_path(_dir: &str) -> bool {
    cli_on_path().is_some()
}

pub fn add_to_path() -> Result<String, String> {
    let dir = cli_dir()?;
    let dir_s = dir.to_string_lossy();
    add_to_path_impl(&dir_s)
}

pub fn remove_from_path() -> Result<String, String> {
    let dir = cli_dir()?;
    let dir_s = dir.to_string_lossy();
    remove_from_path_impl(&dir_s)
}

#[cfg(windows)]
fn decode_reg_sz(bytes: &[u8]) -> String {
    let mut u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    while u16s.last() == Some(&0) {
        u16s.pop();
    }
    String::from_utf16_lossy(&u16s)
}

#[cfg(windows)]
fn encode_reg_sz(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

/// (path_text, registry value type), preserving REG_EXPAND_SZ etc.
#[cfg(windows)]
fn read_user_path() -> Result<(String, winreg::enums::RegType), String> {
    use winreg::{enums::*, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu
        .open_subkey_with_flags("Environment", KEY_QUERY_VALUE)
        .map_err(|e| format!("cannot open HKCU\\Environment: {e}"))?;
    match env.get_raw_value("Path") {
        Ok(v) => Ok((decode_reg_sz(&v.bytes), v.vtype)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((String::new(), REG_EXPAND_SZ)),
        Err(e) => Err(format!("cannot read user Path: {e}")),
    }
}

#[cfg(windows)]
fn write_user_path(text: &str, vtype: winreg::enums::RegType) -> Result<(), String> {
    use winreg::{enums::*, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu
        .open_subkey_with_flags("Environment", KEY_SET_VALUE)
        .map_err(|e| format!("cannot open HKCU\\Environment: {e}"))?;
    env.set_raw_value(
        "Path",
        &winreg::RegValue {
            vtype,
            bytes: encode_reg_sz(text),
        },
    )
    .map_err(|e| format!("cannot write user Path: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn add_to_path_impl(dir: &str) -> Result<String, String> {
    let (text, vtype) = read_user_path()?;
    if std::env::split_paths(&text).any(|d| d == Path::new(dir)) {
        return Ok("already on user PATH".to_string());
    }
    let new = if text.is_empty() {
        dir.to_string()
    } else {
        format!("{text};{dir}")
    };
    write_user_path(&new, vtype)?;
    Ok("added to user PATH — restart terminals/agents to pick it up".to_string())
}

#[cfg(windows)]
fn remove_from_path_impl(dir: &str) -> Result<String, String> {
    let (text, vtype) = read_user_path()?;
    let keep: Vec<PathBuf> = std::env::split_paths(&text)
        .filter(|d| d != Path::new(dir))
        .collect();
    if keep.len() == std::env::split_paths(&text).count() {
        return Ok("not on user PATH".to_string());
    }
    let new = keep
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(";");
    write_user_path(&new, vtype)?;
    Ok("removed from user PATH — restart terminals/agents to pick it up".to_string())
}

#[cfg(not(windows))]
fn local_bin() -> PathBuf {
    home_dir().join(".local").join("bin")
}

#[cfg(not(windows))]
fn add_to_path_impl(_dir: &str) -> Result<String, String> {
    let bin = local_bin();
    std::fs::create_dir_all(&bin).map_err(|e| format!("cannot create {}: {e}", bin.display()))?;
    let link = bin.join("portside-cli");
    let target = cli_path();
    match std::fs::read_link(&link) {
        Ok(existing) if existing == target => return Ok("already on PATH (~/.local/bin)".to_string()),
        Ok(_) => {
            return Err(format!(
                "{} exists and points elsewhere; remove it first",
                link.display()
            ))
        }
        Err(_) => {}
    }
    if link.exists() {
        return Err(format!(
            "{} exists and is not a symlink; remove it first",
            link.display()
        ));
    }
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|e| format!("cannot link {}: {e}", link.display()))?;
    Ok("linked into ~/.local/bin — ensure it is on PATH".to_string())
}

#[cfg(not(windows))]
fn remove_from_path_impl(_dir: &str) -> Result<String, String> {
    let link = local_bin().join("portside-cli");
    match std::fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::remove_file(&link).map_err(|e| e.to_string())?;
            Ok("removed ~/.local/bin/portside-cli".to_string())
        }
        _ => Ok("not on PATH".to_string()),
    }
}

// ---- harness config files ------------------------------------------------

/// Target config file. `project=None` means user-global scope.
pub fn config_path(host: Host, home: &Path, project: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(root) = project {
        return Ok(match host {
            Host::Opencode => root.join("opencode.json"),
            Host::Claude => root.join(".mcp.json"),
            Host::Codex => root.join(".codex").join("config.toml"),
            Host::Cursor => root.join(".cursor").join("mcp.json"),
            Host::Windsurf => root.join(".windsurf").join("mcp_config.json"),
            Host::Pi => root.join(".pi").join("mcp.json"),
            Host::Omp => root.join(".omp").join("mcp.json"),
            Host::Generic => return Err("generic prints to stdout, no file".to_string()),
        });
    }
    Ok(match host {
        Host::Opencode => dirs::config_dir()
            .unwrap_or_else(home_dir)
            .join("opencode")
            .join("opencode.json"),
        Host::Claude => home.join(".claude.json"),
        Host::Codex => home.join(".codex").join("config.toml"),
        Host::Cursor => home.join(".cursor").join("mcp.json"),
        Host::Windsurf => home.join(".codeium").join("windsurf").join("mcp_config.json"),
        Host::Pi => home.join(".pi").join("agent").join("mcp.json"),
        Host::Omp => home.join(".omp").join("agent").join("mcp.json"),
        Host::Generic => return Err("generic prints to stdout, no file".to_string()),
    })
}

/// Server entry for the `mcpServers`-shaped (JSON) harnesses.
pub fn json_server_entry(cmd: &str) -> Value {
    json!({ "command": cmd, "args": ["mcp"] })
}

/// Server entry for opencode (`command` is an argv array).
pub fn opencode_server_entry(argv: &[String]) -> Value {
    json!({ "type": "local", "command": argv })
}

/// Merge one server into a JSON object's `map_key` (`mcpServers` or `mcp`).
/// Returns the new file text and whether it changed. Refuses invalid JSON
/// and non-object roots/maps instead of guessing.
pub fn merge_json_server(
    existing: Option<&str>,
    map_key: &str,
    server: &str,
    entry: Value,
    schema: Option<&str>,
) -> Result<(String, bool), String> {
    let mut root = match existing {
        None => json!({}),
        Some(text) => serde_json::from_str(text)
            .map_err(|e| format!("existing config is not valid JSON ({e}); fix or move it aside"))?,
    };
    let obj = root
        .as_object_mut()
        .ok_or("existing config root must be a JSON object")?;
    let map = obj
        .entry(map_key.to_string())
        .or_insert_with(|| json!({}));
    let map_obj = map
        .as_object_mut()
        .ok_or(format!("existing config {map_key:?} must be an object"))?;
    if map_obj.get(server) == Some(&entry) {
        return Ok((serde_json::to_string_pretty(&root).unwrap(), false));
    }
    map_obj.insert(server.to_string(), entry);
    if let Some(schema) = schema {
        obj.entry("$schema".to_string())
            .or_insert_with(|| json!(schema));
    }
    let mut text = serde_json::to_string_pretty(&root).unwrap();
    text.push('\n');
    Ok((text, true))
}

/// TOML section block for Codex (`[mcp_servers.<name>]`). Appended, never
/// rewritten, so comments and formatting elsewhere survive.
pub fn render_codex_block(server: &str, cmd: &str) -> String {
    let escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\n[mcp_servers.{server}]\ncommand = \"{escaped}\"\nargs = [\"mcp\"]\n")
}

pub fn codex_has_server(text: &str, server: &str) -> bool {
    let want = format!("[mcp_servers.{server}]");
    text.lines().any(|l| l.trim() == want)
}

/// Generic paste-block for harnesses we don't know (stdout, no file).
pub fn generic_block(cmd: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "mcpServers": { "portside": json_server_entry(cmd) }
    }))
    .unwrap()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SetupResult {
    pub host: String,
    pub scope: String,
    pub path_written: Option<String>,
    pub command_used: String,
    pub note: String,
}

/// Register this binary's MCP server with one harness. `project=None` is
/// user-global scope. Idempotent: reruns report "already configured".
pub fn setup_host(host: Host, project: Option<&Path>) -> Result<SetupResult, String> {
    let argv = command_argv();
    let cmd = argv[0].clone();
    if host == Host::Generic {
        return Ok(SetupResult {
            host: host.name().to_string(),
            scope: "stdout".to_string(),
            path_written: None,
            command_used: cmd,
            note: generic_block(&argv[0]),
        });
    }
    let home = home_dir();
    let path = config_path(host, &home, project)?;
    let scope = if project.is_some() { "project" } else { "global" }.to_string();
    if host == Host::Codex {
        return setup_codex(&path, &cmd, scope);
    }
    let map_key = if host == Host::Opencode { "mcp" } else { "mcpServers" };
    let entry = if host == Host::Opencode {
        opencode_server_entry(&argv)
    } else {
        json_server_entry(&cmd)
    };
    let schema = if host == Host::Omp { Some(OMP_SCHEMA) } else { None };
    let existing = std::fs::read_to_string(&path).ok();
    let (text, changed) = merge_json_server(existing.as_deref(), map_key, "portside", entry, schema)?;
    if changed {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(SetupResult {
        host: host.name().to_string(),
        scope,
        path_written: Some(path.to_string_lossy().into_owned()),
        command_used: cmd,
        note: if changed {
            "wrote MCP config (existing entries preserved)".to_string()
        } else {
            "already configured".to_string()
        },
    })
}

fn setup_codex(path: &Path, cmd: &str, scope: String) -> Result<SetupResult, String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if codex_has_server(&existing, "portside") {
        return Ok(SetupResult {
            host: "codex".to_string(),
            scope,
            path_written: Some(path.to_string_lossy().into_owned()),
            command_used: cmd.to_string(),
            note: "already configured".to_string(),
        });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&render_codex_block("portside", cmd));
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(SetupResult {
        host: "codex".to_string(),
        scope,
        path_written: Some(path.to_string_lossy().into_owned()),
        command_used: cmd.to_string(),
        note: "appended [mcp_servers.portside] (rest of file untouched)".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing_accepts_aliases_and_rejects_unknown() {
        assert_eq!("omp".parse::<Host>().unwrap(), Host::Omp);
        assert_eq!("oh-my-pi".parse::<Host>().unwrap(), Host::Omp);
        assert_eq!("claude-code".parse::<Host>().unwrap(), Host::Claude);
        assert!("oracle".parse::<Host>().is_err());
    }

    #[test]
    fn config_paths_match_documented_harness_layouts() {
        let home = Path::new("/home/alice");
        let proj = Path::new("/repo");
        assert_eq!(
            config_path(Host::Pi, home, None).unwrap(),
            PathBuf::from("/home/alice/.pi/agent/mcp.json")
        );
        assert_eq!(
            config_path(Host::Omp, home, None).unwrap(),
            PathBuf::from("/home/alice/.omp/agent/mcp.json")
        );
        assert_eq!(
            config_path(Host::Pi, home, Some(proj)).unwrap(),
            PathBuf::from("/repo/.pi/mcp.json")
        );
        assert_eq!(
            config_path(Host::Omp, home, Some(proj)).unwrap(),
            PathBuf::from("/repo/.omp/mcp.json")
        );
        assert_eq!(
            config_path(Host::Cursor, home, None).unwrap(),
            PathBuf::from("/home/alice/.cursor/mcp.json")
        );
        assert!(config_path(Host::Generic, home, None).is_err());
    }

    #[test]
    fn merge_creates_preserves_and_is_idempotent() {
        let entry = json_server_entry("/usr/bin/portside-cli");
        let (t1, c1) = merge_json_server(None, "mcpServers", "portside", entry.clone(), None).unwrap();
        assert!(c1);
        let v1: Value = serde_json::from_str(&t1).unwrap();
        assert_eq!(v1["mcpServers"]["portside"]["args"], json!(["mcp"]));
        // Idempotent rerun.
        let (_, c2) = merge_json_server(Some(&t1), "mcpServers", "portside", entry.clone(), None).unwrap();
        assert!(!c2);
        // Existing entries survive.
        let other = r#"{"mcpServers": {"gh": {"command": "x"}}}"#;
        let (t3, c3) = merge_json_server(Some(other), "mcpServers", "portside", entry, None).unwrap();
        assert!(c3);
        let v3: Value = serde_json::from_str(&t3).unwrap();
        assert_eq!(v3["mcpServers"]["gh"]["command"], json!("x"));
        assert!(v3["mcpServers"]["portside"].is_object());
    }

    #[test]
    fn merge_refuses_invalid_json_and_wrong_shapes() {
        let entry = json_server_entry("c");
        assert!(merge_json_server(Some("{nope"), "mcpServers", "portside", entry.clone(), None).is_err());
        assert!(merge_json_server(Some("[1,2]"), "mcpServers", "portside", entry.clone(), None).is_err());
        assert!(merge_json_server(Some(r#"{"mcpServers": []}"#), "mcpServers", "portside", entry, None).is_err());
    }

    #[test]
    fn merge_stamps_omp_schema_once() {
        let entry = json_server_entry("c");
        let (t1, _) =
            merge_json_server(None, "mcpServers", "portside", entry.clone(), Some(OMP_SCHEMA)).unwrap();
        let v1: Value = serde_json::from_str(&t1).unwrap();
        assert_eq!(v1["$schema"], json!(OMP_SCHEMA));
        let (t2, changed) =
            merge_json_server(Some(&t1), "mcpServers", "portside", entry, Some(OMP_SCHEMA)).unwrap();
        assert!(!changed);
        assert_eq!(t2.matches(OMP_SCHEMA).count(), 1);
    }

    #[test]
    fn codex_block_escapes_windows_paths_and_detects_existing() {
        let block = render_codex_block("portside", "C:\\tools\\portside-cli.exe");
        assert!(block.contains("[mcp_servers.portside]"));
        assert!(block.contains("command = \"C:\\\\tools\\\\portside-cli.exe\""));
        assert!(codex_has_server(&format!("other = 1\n{block}"), "portside"));
        assert!(!codex_has_server("[mcp_servers.other]\n", "portside"));
    }

    #[test]
    fn opencode_entry_uses_argv_array() {
        let argv = vec!["portside-cli".to_string(), "mcp".to_string()];
        let e = opencode_server_entry(&argv);
        assert_eq!(e["type"], json!("local"));
        assert_eq!(e["command"], json!(["portside-cli", "mcp"]));
    }
}
