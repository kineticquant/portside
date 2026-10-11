import { useEffect, useState } from "react";
import { api, AGENT_HOSTS, type PathStatus } from "../lib/tauri";

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    document.body.removeChild(area);
  }
}

/** One-click agent wiring: user PATH toggle + per-harness MCP registration. */
export default function AgentPanel() {
  const [status, setStatus] = useState<PathStatus | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [genericBlock, setGenericBlock] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refreshStatus = async () => {
    try {
      setStatus(await api.pathStatus());
    } catch {
      setStatus(null);
    }
  };

  useEffect(() => {
    void refreshStatus();
  }, []);

  const runHost = async (host: string) => {
    setBusy(host);
    setMessage(null);
    try {
      if (host === "generic") {
        const r = await api.setupAgent("generic");
        setGenericBlock(r.note);
        setMessage("Paste this block into your harness config.");
      } else {
        const r = await api.setupAgent(host);
        setMessage(`${r.note}${r.path_written ? ` (${r.path_written})` : ""}`);
      }
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(null);
    }
  };

  const togglePath = async () => {
    setBusy("path");
    setMessage(null);
    try {
      const note = await api.setupPath(status?.on_path ?? false);
      setMessage(note);
      await refreshStatus();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="ps-card" aria-label="Agents">
      <div className="ps-card-head">
        <h2>Agents</h2>
      </div>
      <p className="ps-hint">
        {status == null
          ? "Register this machine's databases with a coding-agent harness."
          : status.on_path
            ? `CLI on PATH (${status.dir}). Harness configs can call portside-cli by name.`
            : `CLI not on PATH (${status.dir}). Add it so harnesses can call portside-cli by name.`}
      </p>
      <div className="ps-form-actions">
        <button
          type="button"
          className="ps-btn-ghost"
          disabled={busy === "path"}
          onClick={() => void togglePath()}
        >
          {status?.on_path ? "Remove CLI from PATH" : "Add CLI to PATH"}
        </button>
        {AGENT_HOSTS.map((h) => (
          <button
            key={h}
            type="button"
            className="ps-btn-ghost"
            disabled={busy === h}
            onClick={() => void runHost(h)}
          >
            {h}
          </button>
        ))}
        <button
          type="button"
          className="ps-btn-ghost"
          disabled={busy === "generic"}
          onClick={() => void runHost("generic")}
        >
          other
        </button>
      </div>
      {message ? <p className="ps-hint">{message}</p> : null}
      {genericBlock ? (
        <div className="ps-form-actions">
          <pre className="ps-hint">{genericBlock}</pre>
          <button
            type="button"
            className="ps-btn-ghost"
            onClick={() => void copyText(genericBlock)}
          >
            Copy
          </button>
        </div>
      ) : null}
    </section>
  );
}
