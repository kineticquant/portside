# Portside toolchain

Everything needed to build, test, and run Portside from a fresh clone.
No secrets, `.env` files, or data dumps are required — all state
(instance metadata, DB data) lives outside the repo.

## Version floors

| Tool | Minimum | Tested with |
|------|---------|-------------|
| Node.js | 20 LTS | 24.13.0 |
| npm | 10 | 11.6.2 |
| Rust + Cargo | 1.77 | 1.97.1 |
| Docker Desktop (or Podman with Docker-compat socket) | current | — |

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

## Setup from a fresh clone

```bash
npm install          # frontend deps
npm run build        # verify frontend builds to dist/
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
npx vitest run       # frontend tests
npm run tauri dev    # run the desktop app
```

Release builds for all three OSes are produced by the GitHub workflow
(`.github/workflows/release.yml`) via `tauri-action`.
