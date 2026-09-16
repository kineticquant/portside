//! Runtime prerequisite checks for the installed app.
//!
//! The dev-time `run.bat` flow handles these on the build machine, but an
//! end user installing `Portside-setup.exe` never sees that script. These
//! commands let the app itself detect and (where possible) fix the same
//! prerequisites at runtime: WSL2, Docker Desktop, the VC++ redistributable
//! and WebView2 on Windows. Everything else degrades to guidance.

use serde::Serialize;

// NOTE: the Docker download URL lives in src/lib/tauri.ts
// (DOCKER_DOWNLOAD_URL) - single source of truth on the frontend.

#[derive(Debug, Clone, Serialize)]
pub struct Prereqs {
    pub wsl: bool,
    pub docker_cli: bool,
    pub docker_daemon: bool,
    pub docker_server_version: Option<String>,
    pub vc_redist: bool,
    pub webview2: bool,
}

async fn cmd_ok(program: &str, args: &[&str]) -> bool {
    tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
async fn reg_dword_is_one(path: &str, value: &str) -> bool {
    let out = tokio::process::Command::new("reg")
        .args(["query", path, "/v", value])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines().any(|l| l.contains(value) && l.contains("0x1"))
        }
        _ => false,
    }
}

#[tauri::command]
pub async fn check_prereqs() -> Result<Prereqs, String> {
    // Docker daemon state reuses the same connect path as detect_runtime.
    let (docker_daemon, docker_server_version) = match crate::docker::connect() {
        Ok(docker) => match docker.version().await {
            Ok(v) => (true, v.version),
            Err(_) => (false, None),
        },
        Err(_) => (false, None),
    };

    #[cfg(windows)]
    let (wsl, vc_redist, webview2) = {
        let wsl = cmd_ok("wsl", &["--status"]).await;
        let vc_redist = reg_dword_is_one(
            r"HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64",
            "Installed",
        )
        .await;
        let webview2 = reg_dword_is_one(
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            "pv",
        )
        .await
            || cmd_ok("reg", &["query", r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"]).await;
        (wsl, vc_redist, webview2)
    };
    #[cfg(not(windows))]
    let (wsl, vc_redist, webview2) = (true, true, true);

    Ok(Prereqs {
        wsl,
        docker_cli: cmd_ok("docker", &["--version"]).await,
        docker_daemon,
        docker_server_version,
        vc_redist,
        webview2,
    })
}

/// Launch `wsl --install` elevated. Returns a human message; a reboot is
/// still needed afterwards to finish the WSL install - we never reboot.
#[tauri::command]
pub async fn install_wsl() -> Result<String, String> {
    #[cfg(not(windows))]
    {
        return Err("wsl --install is Windows-only".to_string());
    }
    #[cfg(windows)]
    {
        // wsl.exe needs admin: re-launch it elevated via a UAC prompt.
        let status = tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Start-Process wsl -ArgumentList '--install' -Verb RunAs -Wait",
            ])
            .status()
            .await
            .map_err(|e| format!("failed to launch elevated wsl --install: {e}"))?;
        if status.success() {
            Ok("WSL install launched. It downloads the kernel + Ubuntu (10+ minutes, long quiet stretches are normal - leave it alone), then reboot whenever suits you.".to_string())
        } else {
            Err("elevated wsl --install did not complete (UAC denied or installer error)".to_string())
        }
    }
}

/// Start Docker services minimized (tray, no window in your face) and return
/// immediately - the caller polls `check_prereqs` until the daemon is up.
/// Docker Desktop itself can't be bundled (separate ~500MB product, own
/// license/installer), so this is as "part of Portside" as it gets.
#[tauri::command]
pub async fn start_docker() -> Result<String, String> {
    #[cfg(windows)]
    {
        const DESKTOP: &str = r"C:\Program Files\Docker\Docker\Docker Desktop.exe";
        if !std::path::Path::new(DESKTOP).exists() {
            return Err("Docker services not found on this machine".to_string());
        }
        tokio::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Start-Process '{DESKTOP}' -WindowStyle Minimized"),
            ])
            .status()
            .await
            .map_err(|e| format!("failed to start Docker Desktop: {e}"))?;
        Ok("Docker services starting in the background - daemon should be up within a minute or two.".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        if cmd_ok("open", &["-a", "Docker"]).await {
            Ok("Docker services starting.".to_string())
        } else {
            Err("could not open Docker services".to_string())
        }
    }
    #[cfg(target_os = "linux")]
    {
        if cmd_ok("systemctl", &["--user", "start", "docker"]).await
            || cmd_ok("sudo", &["systemctl", "start", "docker"]).await
        {
            Ok("Docker service start requested.".to_string())
        } else {
            Err("could not start the Docker service".to_string())
        }
    }
}

