import { useEffect, useState } from "react";
import { api } from "../lib/tauri";

export type LogsPanelProps = {
  instanceId: string | null;
};

export default function LogsPanel({ instanceId }: LogsPanelProps) {
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
    return <p>Select an instance to view logs.</p>;
  }

  return (
    <div>
      <h2>Logs</h2>
      {error ? <p role="alert">{error}</p> : null}
      <pre>{logs || "No log output yet."}</pre>
    </div>
  );
}
