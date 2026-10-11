# Portside

Local database launcher for devs. Spin up versioned database instances as
Docker containers, manage databases, tail logs, and copy connect strings,
all from one Tauri desktop app.

A modern, open replacement for [StackBricks](https://stackbricks.app/)
(MariaDB / MySQL / PostgreSQL / Redis on your desktop), built with Tauri
(Rust) + Docker and per-instance TLS for LAN sharing.

## Built for agentic coding

Agents burn through databases: every experiment wants a fresh schema,
every attempt wants isolation, and a shared dev DB gets clobbered fast.
Portside gives each agent and each project its own isolated home:
one-click instance creation, dedicated databases or schemas per project,
multiple engine versions running side by side. Paste the connect string
into the agent's env, let it prototype freely, wipe the instance when
done. All local Docker, so it's fast, offline, and free. Agents can
drive all of this themselves via the CLI/MCP interface below — no
copy-paste required.

## Databases we support

| Engine | Versions (Docker tags, see `src/catalog.json`) |
|--------|-------------------------------------------------|
| MySQL | 8.4, 9.7, 26.7 |
| MariaDB | 11.4, 11.8, 12.3 |
| Postgres | 16, 17 |
| Redis | 7, 8 |
| Valkey | 8 |

Multiple versions of the same engine run side by side on different ports.

## Run it

Needs Docker running.

```bash
npm install
npm run tauri dev
```

Windows one-click dev flow: double-click `run.bat` (checks the toolchain,
starts Docker, launches the app). Details in `TOOLCHAIN.md`.

## How you use it

- **Create instance**: pick engine, version, port (blank = auto), and an
  optional password. Defaults: `portside` (SQL), empty (redis/valkey).
  The password is set once and can't be changed later.
- **Copy connect**: localhost string. **Copy LAN**: laptop-facing string
  using this machine's LAN IP (LAN instances only).
- **LAN (TLS)**: lets other machines on your network connect. The instance
  encrypts with a self-signed cert; clients don't verify it by default.
- **Lifecycle**: Inactivate stops the container but keeps data. Remove
  drops the container but keeps the data volume. Wipe drops both (no
  restore, confirms first).

Instance metadata lives in SQLite (`%LOCALAPPDATA%\portside\portside.db`);
actual data lives in Docker volumes (`portside-{id}-data`).

## Agents (CLI + MCP)

Headless control for agents and scripts — same instances, same Docker,
same SQLite as the desktop app.

```bash
cargo build --release --manifest-path src-tauri/Cargo.toml --bin portside-cli
./src-tauri/target/release/portside-cli create --engine postgres --tag 17 --wait 60 --json
./src-tauri/target/release/portside-cli conn <id> --json
./src-tauri/target/release/portside-cli db create <id> demo --json
./src-tauri/target/release/portside-cli rm <id> --wipe
```

### One-command harness setup (no repo needed)

`setup-agent` registers the MCP server in a harness config, merging
into the existing file without touching anything else. Global scope by
default; add `--project-dir <dir>` for project-scoped files. Installer
users get the same via the app's Agents panel.

```bash
portside-cli setup-agent --host opencode   # ~/.config/opencode/opencode.json
portside-cli setup-agent --host claude     # project .mcp.json (see below)
portside-cli setup-agent --host codex      # ~/.codex/config.toml (appends [mcp_servers.portside])
portside-cli setup-agent --host cursor     # ~/.cursor/mcp.json
portside-cli setup-agent --host windsurf   # ~/.codeium/windsurf/mcp_config.json
portside-cli setup-agent --host pi         # ~/.pi/agent/mcp.json (or `pi mcp add`)
portside-cli setup-agent --host omp        # ~/.omp/agent/mcp.json
portside-cli setup-agent --host generic    # prints a paste-block for anything else
```

Reruns are idempotent ("already configured"). When `portside-cli`
resolves on PATH the config uses the bare name (survives reinstalls);
otherwise it records the absolute binary path:

```bash
portside-cli setup-path          # add this binary's dir to the user PATH
portside-cli setup-path --remove # undo (same toggle lives in the app's Agents panel)
```

opencode (`opencode.json`):

```json
{ "mcp": { "portside": { "type": "local", "command": ["portside-cli", "mcp"] } } }
```

Claude Code (`.mcp.json` in your project):

```json
{ "mcpServers": { "portside": { "command": "portside-cli", "args": ["mcp"] } } }
```

Prompt your agent: "use portside to stand up postgres 17 with a
database called demo and give me the connect string."

## Tests

```bash
npx vitest run                                    # frontend
cargo test --manifest-path src-tauri/Cargo.toml   # Rust (needs cargo)
```

Releases are built by `.github/workflows/release.yml` for Windows, macOS
(Intel + ARM), and Linux.

## License

MIT. See `LICENSE`.
