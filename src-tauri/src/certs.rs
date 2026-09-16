//! Per-instance TLS certificates and passwords for LAN mode.
//!
//! Default posture mirrors pgAdmin: encryption without friction. Localhost
//! instances stay plaintext; LAN-bound instances serve TLS with a
//! self-signed cert and clients connect with verification OFF. A strict
//! toggle (verify-ca + downloaded server cert) exists for the paranoid.
//! Passwords are user-settable at creation, stored plaintext in SQLite
//! alongside everything else (local dev tool - same exposure as the
//! connect strings we already copy to clipboards).

use std::net::{IpAddr, UdpSocket};

/// Password policy: short and URL-safe. Only RFC 3986 userinfo-safe chars
/// (unreserved + sub-delims); `: @ / ? # [ ]`, quotes, backslash, spaces
/// and controls are rejected so the password can sit in connect strings
/// and container env/flags unescaped.
pub fn check_password(pw: &str, engine: &str) -> Result<String, String> {
    if pw.is_empty() {
        // Engine defaults preserve current behavior.
        return Ok(default_password(engine));
    }
    if pw.len() > 64 {
        return Err("password must be at most 64 characters".to_string());
    }
    if !pw.chars().all(|c| {
        c.is_ascii_alphanumeric() || "_~-!$&'()*+,;=.".contains(c)
    }) {
        return Err(
            "password may only contain A-Z a-z 0-9 and _~-!$&'()*+,;=.".to_string(),
        );
    }
    Ok(pw.to_string())
}

pub fn default_password(engine: &str) -> String {
    match engine {
        "redis" | "valkey" => String::new(), // no auth by default, as before
        _ => "portside".to_string(),
    }
}

/// Best-effort LAN IP (no new crates: UDP "connect" never sends packets).
pub fn lan_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(v) if !v.is_loopback() => Some(v.to_string()),
        _ => None,
    }
}

fn cert_dir() -> Result<std::path::PathBuf, String> {
    let dir = crate::state::data_dir()
        .join("certs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Generate (or reuse) a self-signed server cert for an instance.
/// SANs cover localhost, loopback, and the current LAN IP.
pub fn ensure_cert(id: &str) -> Result<(String, String), String> {
    let dir = cert_dir()?;
    let cert_path = dir.join(format!("{id}.crt"));
    let key_path = dir.join(format!("{id}.key"));
    if cert_path.exists() && key_path.exists() {
        return Ok((
            cert_path.to_string_lossy().into_owned(),
            key_path.to_string_lossy().into_owned(),
        ));
    }
    let mut sans: Vec<String> = vec!["localhost".to_string()];
    if let Some(lan) = lan_ip() {
        sans.push(lan);
    }
    let key = rcgen::generate_simple_self_signed(sans).map_err(|e| e.to_string())?;
    std::fs::write(&cert_path, key.cert.pem()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&key_path)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| std::fs::Permissions::from_mode(0o600));
        perm.set_mode(0o600);
        std::fs::write(&key_path, key.key_pair.serialize_pem()).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&key_path, perm).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&key_path, key.key_pair.serialize_pem()).map_err(|e| e.to_string())?;
    }
    Ok((
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    ))
}

pub fn read_cert_pem(id: &str) -> Result<String, String> {
    let (cert_path, _) = ensure_cert(id)?;
    std::fs::read_to_string(cert_path).map_err(|e| e.to_string())
}

/// This dev box's LAN address for building laptop-facing connect strings.
/// None when offline or loopback-only.
#[tauri::command]
pub fn lan_addr() -> Option<String> {
    lan_ip()
}

/// Server cert PEM for an instance (strict verify-ca mode: clients install
/// this one file). Verifies the instance exists first.
#[tauri::command]
pub fn server_cert(id: String) -> Result<String, String> {
    crate::state::load_instance(&id)?;
    read_cert_pem(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_password_falls_back_to_engine_default() {
        assert_eq!(check_password("", "postgres").unwrap(), "portside");
        assert_eq!(check_password("", "redis").unwrap(), "");
    }

    #[test]
    fn password_policy_rejects_url_breakers() {
        assert!(check_password("s3cret!", "postgres").is_ok());
        assert!(check_password("a-b_c~d.e$f", "postgres").is_ok());
        assert!(check_password("has space", "postgres").is_err());
        assert!(check_password("with@at", "postgres").is_err());
        assert!(check_password("with:colon", "postgres").is_err());
        assert!(check_password("with#hash", "postgres").is_err());
        assert!(check_password("back\\slash", "postgres").is_err());
        assert!(check_password("\"quoted\"", "postgres").is_err());
    }
}
