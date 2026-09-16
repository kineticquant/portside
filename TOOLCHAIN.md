# Portside toolchain

Everything needed to build, test, and run Portside from a fresh clone.
No secrets, `.env` files, or data dumps are required — all state
(instance metadata, DB data) lives outside the repo.

## Version floors

| Tool | Minimum | Tested with | This machine (2026-09-13) |
|------|---------|-------------|---------------------------|
| Node.js | 20 LTS | 24.13.0 | 22.15.1 OK |
| npm | 10 | 11.6.2 | 10.9.2 OK |
| Rust + Cargo | 1.77 | 1.97.1 | MISSING - install via winget below |
| Docker Desktop (or Podman with Docker-compat socket) | current | — | MISSING - install via winget below |
| MSVC Build Tools (Windows only) | VS 2022 17.x + VCTools workload | — | MISSING - install via winget below |
| WebView2 (Windows only) | preinstalled Win10 1803+ | — | OK (Edge + `F3017226-...` client key present) |
| winget | any recent | — | 1.29.290 OK |

## OS prerequisites (Tauri v2)

- **Windows 10+**: WebView2 (preinstalled on Win10 1803+ / Win11) + MSVC
  Build Tools (`Desktop development with C++` workload).
- **macOS 13+**: Xcode Command Line Tools (`xcode-select --install`).
- **Linux**: WebKit2GTK + build tools, e.g. on Ubuntu/Debian:
  `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget
  file libssl-dev libayatana-appindicator3-dev librsvg2-dev`

## Runtime requirement

Docker Desktop (or Podman exposing a Docker-compatible socket) must be
running to create/boot database instances. The UI starts without it but
shows an onboarding banner until a runtime is detected.

## One-click run (Windows) — fully automatic, never reboots you

Double-click `run.bat` and do nothing else. Prompts are limited to one
UAC "Yes" (enforced by Windows, can't be scripted away) plus one
WSL2 yes/no (recommended — Docker Desktop cannot run without it):

1. `scripts/check-env.ps1` (exit 0 ready / 1 missing / 2 reboot-would-help).
2. `scripts/setup-windows.ps1` for anything missing — Rust per-user
   first (so `cargo` lands in your profile), then self-elevates for
   WSL + Build Tools + Docker. If the VS installer exists but the C++
   workload is missing, it runs `vs_installer modify --add
   Microsoft.VisualStudio.Workload.NativeDesktop` itself — no manual
   clicks. Elevated output is captured to
   `C:\ProgramData\portside-setup.log` and its tail is printed, so a
   failure is visible instead of a silent loop.
3. It starts Docker Desktop itself and waits up to ~4 min for the
   daemon, runs `npm install` if `node_modules/` is absent, then
   `npm run tauri dev`.
4. NEVER force-reboots and NEVER registers anything at startup (no
   RunOnce, no services). If a reboot is needed (WSL / Docker), it only
   prints that — reboot whenever suits you, then double-click `run.bat`
   again. Autostart stays the app's own business: only Portside itself
   may offer a run-at-startup setting, never the installer.

`scripts/setup-windows.ps1` is the same installer extracted for
standalone use (also runnable as `npm run setup:win`).

## Windows setup via winget (new - verified 2026-09-13)

Run in an elevated PowerShell (only the Build Tools + Docker steps
need elevation; Rustup does not):

```powershell
# 1. Rust toolchain (provides cargo + rustc)
winget install --exact --id Rustlang.Rustup --source winget
# then in a FRESH terminal (so PATH picks up cargo):
rustup toolchain install stable
rustup default stable

# 2. C++ build tools for Tauri (large download, ~5GB with SDK).
#    Must include the "Desktop development with C++" workload
#    (Microsoft.VisualStudio.Workload.NativeDesktop - note: VCTools is a
#    component inside it, passing it as a workload id silently does
#    nothing). run.bat adds it via `vs_installer modify` automatically;
#    only if that fails, open VS Installer > Modify > check "Desktop
#    development with C++" > Modify by hand.
winget install --exact --id Microsoft.VisualStudio.2022.BuildTools --source winget `
  --override "--add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended --passive --norestart"
# NOTE: do NOT add --wait (vs_installer 4.10.x rejects it: "Option 'wait' is
# unknown", exit 87). Detection is cl.exe on disk, not just vswhere output.
# The C++ workload must include a Windows 11 SDK: Tauri's build script
# shells to rc.exe (cargo check/test never exercise it, so check-env
# verifies it explicitly). run.bat prepends the newest SDK bin to PATH and
# sets RC to its rc.exe - the registry auto-discovery fails on some
# machines even with PATH set; plain `cargo build` was verified to pass
# only with RC set.

# 3. Docker Desktop (requires WSL2 + reboot; start it after install)
winget install --exact --id Docker.DockerDesktop --source winget
```

Verify after install (fresh terminal):

```powershell
node --version   # want 20+
cargo --version  # want 1.77+
docker info --format '{{.ServerVersion}}'  # must print a version; if not, Docker Desktop isn't running
npx --yes @tauri-apps/cli --version  # want v2.x (repo used 2.11.4)
```

Notes:

- WebView2: do NOT install manually. It ships with Edge on Win10
  1803+/Win11. Confirm via registry key
  `HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`.
- If `cargo` is still unknown after Rustup install, reopen the
  terminal or run `$env:Path += ";$env:USERPROFILE\.cargo\bin"`.
- If `docker` is unknown but Docker Desktop is installed, start
  Docker Desktop once and enable "Expose daemon on tcp://localhost:2375
  without TLS" only if `docker.rs` socket fallback fails (default named
  pipe `//./pipe/docker_engine` is preferred).
- `node_modules/`, `dist/`, `src-tauri/target/` are git-ignored build
  outputs (verified in `.gitignore`).

## Setup from a fresh clone (manual / macOS / Linux)

```bash
npm install          # frontend deps
npm run build        # verify frontend builds to dist/
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
npx vitest run       # frontend tests
npm run tauri dev    # run the desktop app
```

Release builds for all three OSes are produced by the GitHub workflow
(`.github/workflows/release.yml`) via `tauri-action`. Push a `v*` tag
or use `workflow_dispatch` to get signed installers.
