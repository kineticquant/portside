import { useEffect, useState } from "react";
import InfoTip from "./InfoTip";
import { api, type Adoptable, type PgServer } from "../lib/tauri";

export type ImportDialogProps = {
  open: boolean;
  onClose: () => void;
  onDone: () => void;
};

type Tab = "connection" | "docker" | "pgadmin";

const ENGINES = ["postgres", "mysql", "mariadb", "redis", "valkey"];

function defaultPort(engine: string): string {
  switch (engine) {
    case "postgres":
      return "5432";
    case "mysql":
    case "mariadb":
      return "3306";
    default:
      return "6379";
  }
}

export default function ImportDialog({ open, onClose, onDone }: ImportDialogProps) {
  const [tab, setTab] = useState<Tab>("connection");
  const [engine, setEngine] = useState("postgres");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("5432");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [ssl, setSsl] = useState("off");
  const [busy, setBusy] = useState(false);
  const [probe, setProbe] = useState<{
    sig: string;
    ok: boolean;
    version?: string | null;
    error?: string | null;
  } | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [adoptable, setAdoptable] = useState<Adoptable[] | null>(null);
  const [adoptError, setAdoptError] = useState<string | null>(null);
  const [pgServers, setPgServers] = useState<PgServer[] | null>(null);
  const [pgError, setPgError] = useState<string | null>(null);
  const [pgPasswords, setPgPasswords] = useState<Record<number, string>>({});
  const [pgBusy, setPgBusy] = useState<number | null>(null);

  useEffect(() => {
    if (!open || tab !== "docker" || adoptable !== null) return;
    let cancelled = false;
    api
      .adoptable()
      .then((list) => {
        if (!cancelled) setAdoptable(list);
      })
      .catch((e) => {
        if (!cancelled) setAdoptError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [open, tab, adoptable]);

  useEffect(() => {
    if (!open || tab !== "pgadmin" || pgServers !== null) return;
    let cancelled = false;
    api
      .pgadmin()
      .then((list) => {
        if (!cancelled) setPgServers(list);
      })
      .catch((e) => {
        if (!cancelled) setPgError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [open, tab, pgServers]);

  if (!open) return null;

  const sig = JSON.stringify({ engine, host, port, user, password, ssl });
  const probeOk = probe !== null && probe.ok && probe.sig === sig;

  const handleProbe = async () => {
    setBusy(true);
    setSaveError(null);
    try {
      const portNum = Number(port) || 0;
      const r = await api.probe(engine, host.trim(), portNum, user.trim(), password, ssl);
      setProbe({ sig, ok: r.ok, version: r.version, error: r.error });
    } catch (e) {
      setProbe({ sig, ok: false, error: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleSave = async () => {
    if (!probeOk) return;
    setBusy(true);
    setSaveError(null);
    try {
      const portNum = Number(port) || 0;
      await api.importExternal(engine, host.trim(), portNum, user.trim(), password, ssl);
      onDone();
      onClose();
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleAdopt = async (container: string) => {
    setBusy(true);
    setAdoptError(null);
    try {
      await api.adopt(container);
      onDone();
      onClose();
    } catch (e) {
      setAdoptError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handlePgImport = async (srv: PgServer, idx: number) => {
    setPgBusy(idx);
    setPgError(null);
    try {
      await api.importExternal(
        "postgres",
        srv.host,
        srv.port,
        srv.username,
        pgPasswords[idx] ?? "",
        "off",
      );
      onDone();
      onClose();
    } catch (e) {
      setPgError(String(e));
    } finally {
      setPgBusy(null);
    }
  };

  return (
    <div className="ps-dialog-wrap" role="dialog" aria-label="Import database">
      <div className="ps-dialog">
        <h2>Import database</h2>
        <div className="ps-tabs" role="tablist">
          {(["connection", "docker", "pgadmin"] as Tab[]).map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              className={tab === t ? "ps-tab ps-tab-active" : "ps-tab"}
              onClick={() => setTab(t)}
            >
              {t === "connection" ? "Connection" : t === "docker" ? "Docker" : "pgAdmin"}
            </button>
          ))}
        </div>

        {tab === "connection" ? (
          <div>
            <p className="ps-hint">
              Point Portside at a server it did not create. It stays where it
              is. Portside only reads it.
              <InfoTip
                label="About imported servers"
                text="The server keeps running wherever it lives. You get health, browsing, and connect strings, but no start, stop, or delete buttons for it."
              />
            </p>
            {saveError ? (
              <p role="alert" className="ps-alert">
                {saveError}
              </p>
            ) : null}
            <div className="ps-dialog-grid">
              <label className="ps-field">
                <span className="ps-field-name">Engine</span>
                <select
                  aria-label="engine"
                  value={engine}
                  onChange={(e) => {
                    setEngine(e.target.value);
                    setPort(defaultPort(e.target.value));
                  }}
                >
                  {ENGINES.map((e) => (
                    <option key={e} value={e}>
                      {e}
                    </option>
                  ))}
                </select>
              </label>
              <label className="ps-field">
                <span className="ps-field-name">TLS mode</span>
                <select aria-label="TLS mode" value={ssl} onChange={(e) => setSsl(e.target.value)}>
                  <option value="off">Off</option>
                  <option value="require">Require (encrypted, unverified)</option>
                </select>
              </label>
              <label className="ps-field">
                <span className="ps-field-name">Host</span>
                <input
                  aria-label="host"
                  placeholder="db.lan or 192.168.1.20"
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                />
              </label>
              <label className="ps-field">
                <span className="ps-field-name">Port</span>
                <input
                  aria-label="port"
                  placeholder={defaultPort(engine)}
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                />
              </label>
              <label className="ps-field">
                <span className="ps-field-name">Username</span>
                <input
                  aria-label="username"
                  placeholder={engine === "postgres" ? "postgres" : "root"}
                  value={user}
                  onChange={(e) => setUser(e.target.value)}
                />
              </label>
              <label className="ps-field">
                <span className="ps-field-name">Password</span>
                <input
                  aria-label="password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
              </label>
            </div>
            {probe && probe.sig === sig ? (
              probe.ok ? (
                <p className="ps-hint">
                  Connected{probe.version ? ` (server ${probe.version})` : ""}. Ready to save.
                </p>
              ) : (
                <p role="alert" className="ps-alert">
                  {probe.error ?? "Connection failed."}
                </p>
              )
            ) : null}
            <footer className="ps-dialog-actions">
              <button type="button" className="ps-btn-ghost" onClick={onClose}>
                Cancel
              </button>
              <button
                type="button"
                className="ps-btn-ghost"
                disabled={busy || !host.trim()}
                onClick={() => void handleProbe()}
              >
                {busy ? "Testing…" : "Test connection"}
              </button>
              <button
                type="button"
                className="ps-btn"
                disabled={!probeOk || busy}
                onClick={() => void handleSave()}
              >
                Save
              </button>
            </footer>
          </div>
        ) : null}

        {tab === "docker" ? (
          <div>
            <p className="ps-hint">
              Containers on this machine that Portside did not create. Adopting
              keeps the container and brings it under management.
            </p>
            {adoptError ? (
              <p role="alert" className="ps-alert">
                {adoptError}
              </p>
            ) : null}
            {adoptable === null ? (
              <p className="ps-hint">Scanning containers…</p>
            ) : adoptable.length === 0 ? (
              <p className="ps-hint">No adoptable database containers found.</p>
            ) : (
              <ul className="ps-list">
                {adoptable.map((a) => (
                  <li key={a.container}>
                    {a.container}{" "}
                    <span className="ps-cell-sub">
                      {a.engine}:{a.tag} · {a.status}
                    </span>{" "}
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      disabled={busy}
                      onClick={() => void handleAdopt(a.container)}
                    >
                      Adopt
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <footer className="ps-dialog-actions">
              <button type="button" className="ps-btn-ghost" onClick={onClose}>
                Close
              </button>
            </footer>
          </div>
        ) : null}

        {tab === "pgadmin" ? (
          <div>
            <p className="ps-hint">
              Servers from your pgAdmin config. Passwords cannot be read from
              pgAdmin, so type each one once.
            </p>
            {pgError ? (
              <p role="alert" className="ps-alert">
                {pgError}
              </p>
            ) : null}
            {pgServers === null && !pgError ? (
              <p className="ps-hint">Reading pgAdmin config…</p>
            ) : pgServers !== null && pgServers.length === 0 ? (
              <p className="ps-hint">No servers in your pgAdmin config.</p>
            ) : (
              <ul className="ps-list">
                {(pgServers ?? []).map((srv, idx) => (
                  <li key={`${srv.name}-${idx}`}>
                    <span>
                      {srv.name}{" "}
                      <span className="ps-cell-sub">
                        {srv.username}@{srv.host}:{srv.port}/{srv.database}
                      </span>
                    </span>{" "}
                    <input
                      className="ps-inline-input"
                      aria-label={`password for ${srv.name}`}
                      type="password"
                      placeholder="password"
                      value={pgPasswords[idx] ?? ""}
                      onChange={(e) =>
                        setPgPasswords((p) => ({ ...p, [idx]: e.target.value }))
                      }
                    />{" "}
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      disabled={pgBusy === idx}
                      onClick={() => void handlePgImport(srv, idx)}
                    >
                      {pgBusy === idx ? "Importing…" : "Import"}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <footer className="ps-dialog-actions">
              <button type="button" className="ps-btn-ghost" onClick={onClose}>
                Close
              </button>
            </footer>
          </div>
        ) : null}
      </div>
    </div>
  );
}
