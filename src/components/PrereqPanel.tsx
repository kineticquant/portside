import { useEffect, useRef, useState } from "react";
import { api, DOCKER_DOWNLOAD_URL, type Prereqs } from "../lib/tauri";

export type PrereqPanelProps = {
  prereqs: Prereqs | null;
  onRefresh: () => void;
  onNotice: (message: string) => void;
};

/** Actionable first-run panel: the installed app guides its own setup. */
export default function PrereqPanel({ prereqs, onRefresh, onNotice }: PrereqPanelProps) {
  const [busy, setBusy] = useState(false);
  const autoTried = useRef(false);

  const handleStartDocker = async () => {
    setBusy(true);
    try {
      const msg = await api.startDocker();
      onNotice(msg);
      // App polls check_prereqs every 5s until the daemon is up and clears
      // this notice then; refresh once now so the panel updates promptly.
      onRefresh();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setBusy(false);
    }
  };

  // One automatic start attempt per session: Docker runs as part of
  // Portside, minimized to tray - no window in your face.
  const daemonDown = !!prereqs && prereqs.docker_cli && !prereqs.docker_daemon;
  useEffect(() => {
    if (daemonDown && !autoTried.current) {
      autoTried.current = true;
      void handleStartDocker();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [daemonDown]);

  if (!prereqs) return null;
  const issues: string[] = [];
  if (!prereqs.wsl) issues.push("wsl");
  if (!prereqs.docker_cli) issues.push("docker");
  if (!prereqs.vc_redist) issues.push("vcredist");
  if (!prereqs.webview2) issues.push("webview2");
  if (issues.length === 0 && prereqs.docker_daemon) return null;

  const handleInstallWsl = async () => {
    setBusy(true);
    try {
      const msg = await api.installWsl();
      onNotice(msg);
      onRefresh();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ps-card ps-onboard" role="region" aria-label="setup">
      <h2>Setup needed</h2>
      {!prereqs.wsl ? (
        <p>
          <strong>WSL2 is not installed</strong> — Docker services cannot run
          without it. Installing downloads the kernel + Ubuntu and can sit
          quiet for 10+ minutes: that is normal, leave the window alone
          until it finishes, then reboot whenever suits you.
          <br />
          <button
            type="button"
            className="ps-btn"
            disabled={busy}
            onClick={() => void handleInstallWsl()}
          >
            {busy ? "Installing…" : "Install WSL2"}
          </button>
        </p>
      ) : null}
      {!prereqs.docker_cli ? (
        <p>
          <strong>Docker services are not installed.</strong> Grab them here and
          re-run this check after installing:
          <br />
          <code>{DOCKER_DOWNLOAD_URL}</code>
        </p>
      ) : !prereqs.docker_daemon ? (
        <p>
          Docker is installed but the daemon is not running — Portside
          starts it minimized to tray by itself.
          <br />
          <button
            type="button"
            className="ps-btn"
            disabled={busy}
            onClick={() => void handleStartDocker()}
          >
            {busy ? "Starting…" : "Start Docker Services"}
          </button>
        </p>
      ) : null}
      {!prereqs.vc_redist ? (
        <p className="ps-hint">
          VC++ redistributable not detected. The installer normally handles
          this; repair via the installer if the app misbehaves.
        </p>
      ) : null}
      {!prereqs.webview2 ? (
        <p className="ps-hint">
          WebView2 runtime not detected. The installer normally handles this.
        </p>
      ) : null}
      <button type="button" className="ps-btn-ghost" onClick={onRefresh}>
        Re-check
      </button>
    </div>
  );
}
