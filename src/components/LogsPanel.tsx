import { useEffect, useState } from "react";
import { api } from "../lib/tauri";

export type LogsPanelProps = {
  instanceId: string | null;
  origin?: string;
};

export default function LogsPanel({ instanceId, origin }: LogsPanelProps) {
  const [logs, setLogs] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!instanceId) return;
    let cancelled = false;
    const fetchLogs = async () => {
      try {
        const text = await api.logs(instanceId, 200);
        if (!cancelled) {
          setLogs(text);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    };
    void fetchLogs();
    const timer = setInterval(() => void fetchLogs(), 2000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [instanceId]);

  if (!instanceId) {
    return (
      <div className="ps-empty">
        <strong>No logs yet</strong>
        Select a row above to tail its container output, refreshed every 2s.
      </div>
    );
  }

  if (origin === "imported") {
    return (
      <div className="ps-empty">
        <strong>No logs here</strong>
        Imported servers run elsewhere, so there is no container output to tail.
      </div>
    );
  }

  return (
    <div>
      <h2>Logs</h2>
      {error ? (
        <p role="alert" className="ps-alert">
          {error}
        </p>
      ) : null}
      <pre className="ps-logs">{logs || "No log output yet."}</pre>
    </div>
  );
}
