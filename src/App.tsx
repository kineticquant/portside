import { useEffect, useRef, useState } from "react";
import ImportDialog from "./components/ImportDialog";
import InfoTip from "./components/InfoTip";
import InstanceList from "./components/InstanceList";
import LogsPanel from "./components/LogsPanel";
import PrereqPanel from "./components/PrereqPanel";
import SchemaPanel from "./components/SchemaPanel";
import { api, type CatalogEntry, type Health, type Instance, type Prereqs } from "./lib/tauri";

const ENGINES = ["mysql", "mariadb", "postgres", "redis", "valkey"];

export function parsePortInput(raw: string): number | "auto" | null {
  const trimmed = raw.trim();
  if (!trimmed) return "auto";
  const n = Number(trimmed);
  if (!Number.isInteger(n) || n < 1 || n > 65535) return null;
  return n;
}

export default function App() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [prereqs, setPrereqs] = useState<Prereqs | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [health, setHealth] = useState<Health | null>(null);
  const [engine, setEngine] = useState("postgres");
  const [tag, setTag] = useState("");
  const [port, setPort] = useState("");
  const [lan, setLan] = useState(false);
  const [password, setPassword] = useState("");
  const [lanAddr, setLanAddr] = useState<string | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const [strict, setStrict] = useState(
    () => localStorage.getItem("ps-strict-tls") === "1",
  );
  const createDialogRef = useRef<HTMLDialogElement>(null);

  const openCreate = () => {
    createDialogRef.current?.showModal();
  };

  const closeCreate = () => {
    createDialogRef.current?.close();
  };

  const toggleStrict = () => {
    setStrict((s) => {
      localStorage.setItem("ps-strict-tls", s ? "0" : "1");
      return !s;
    });
  };

  const refreshInstances = async () => {
    try {
      const list = await api.list();
      setInstances(list);
      setActionError(null);
    } catch (e) {
      setActionError(String(e));
    }
  };

  const refreshPrereqs = async () => {
    try {
      setPrereqs(await api.prereqs());
    } catch {
      setPrereqs(null);
    }
  };

  const refreshRuntime = async () => {
    try {
      await api.detectRuntime();
      setRuntimeError(null);
    } catch (e) {
      setRuntimeError(String(e));
    }
  };

  useEffect(() => {
    let cancelled = false;
    void refreshPrereqs();
    api
      .lanAddr()
      .then((a) => {
        if (!cancelled) setLanAddr(a);
      })
      .catch(() => {
        if (!cancelled) setLanAddr(null);
      });
    api
      .detectRuntime()
      .then(() => {
        if (!cancelled) setRuntimeError(null);
      })
      .catch((e) => {
        if (!cancelled) setRuntimeError(String(e));
      });
    api
      .catalog()
      .then((c) => {
        if (!cancelled) setCatalog(c.entries);
      })
      .catch(() => {
        if (!cancelled) setCatalog([]);
      });
    api
      .list()
      .then((list) => {
        if (!cancelled) setInstances(list);
      })
      .catch((e) => {
        if (!cancelled) setActionError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const tagsForEngine = catalog.filter((e) => e.engine === engine).map((e) => e.tag);

  // The daemon can take minutes to come up after a (re)start, and a single
  // re-check leaves a stale "starting…" notice behind. Keep polling until
  // Docker is up, then clear the notice.
  useEffect(() => {
    if (prereqs?.docker_daemon && !runtimeError) {
      setNotice(null);
      return;
    }
    const timer = setInterval(() => {
      void refreshPrereqs();
      void refreshRuntime();
    }, 5000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prereqs?.docker_daemon, runtimeError]);

  const handleCreate = async (): Promise<boolean> => {
    setActionError(null);
    try {
      const taken = instances.map((i) => i.port);
      const parsed = parsePortInput(port);
      if (parsed === null) {
        setActionError("Port must be a number 1-65535");
        return false;
      }
      const portNum =
        parsed === "auto" ? await api.suggestPort(engine, taken) : parsed;
      const chosenTag = tag || tagsForEngine[0] || "latest";
      await api.create(
        engine,
        chosenTag,
        portNum,
        lan ? "0.0.0.0" : "127.0.0.1",
        password,
      );
      setPort("");
      setPassword("");
      await refreshInstances();
      return true;
    } catch (e) {
      setActionError(String(e));
      return false;
    }
  };

  const handleStart = async (id: string) => {
    try {
      await api.start(id);
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
    }
  };

  const handleStop = async (id: string) => {
    try {
      await api.stop(id);
      await refreshInstances();
    } catch (e) {
      // Stopping an already-stopped container returns Docker 304 as an
      // error; surface it in the UI instead of crashing.
      setActionError(String(e));
    }
  };

  // Remove = drop container, KEEP data volume (restorable/extractable).
  const handleRemove = async (id: string) => {
    try {
      await api.remove(id, false);
      if (selectedId === id) setSelectedId(null);
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
    }
  };

  // Wipe = drop container AND data volume. Destructive, no restore.
  const handleWipe = async (id: string) => {
    try {
      await api.remove(id, true);
      if (selectedId === id) setSelectedId(null);
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
    }
  };

  // Forget = delete Portside's record for an imported row. The server
  // itself is never touched (enforced backend-side too).
  const handleForget = async (id: string) => {
    try {
      await api.forget(id);
      if (selectedId === id) setSelectedId(null);
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
    }
  };

  const selected = instances.find((i) => i.id === selectedId) ?? null;

  useEffect(() => {
    if (!selected) {
      setHealth(null);
      return;
    }
    let cancelled = false;
    const fetchHealth = async () => {
      try {
        const h = await api.health(selected.id);
        if (!cancelled) setHealth(h);
      } catch {
        if (!cancelled) setHealth(null);
      }
    };
    void fetchHealth();
    const timer = setInterval(() => void fetchHealth(), 5000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [selected?.id]);

  return (
    <div className="ps-root">
      <header className="ps-topbar">
        <div className="ps-brand">
          <h1>Portside</h1>
        </div>
        <div className="ps-toolbar">
          <label className="ps-check">
            <input
              aria-label="strict TLS verify"
              type="checkbox"
              checked={strict}
              onChange={toggleStrict}
            />
            Strict TLS
            <InfoTip
              align="right"
              direction="down"
              label="About strict TLS"
              text="Extra identity check for LAN-shared instances. Only turn it on if you also download the instance's certificate (the row's Cert button) and add it to your database app — without that file, connections fail. Leave it off for normal use: LAN traffic is still encrypted either way."
            />
          </label>
        </div>
      </header>
      {notice ? <p className="ps-banner">{notice}</p> : null}
      <PrereqPanel prereqs={prereqs} onRefresh={() => void refreshPrereqs()} onNotice={setNotice} />
      {runtimeError && prereqs?.docker_daemon !== true ? (
        <p role="alert" className="ps-banner">
          Docker services not detected ({runtimeError}). Start Docker services and retry.
        </p>
      ) : null}
      <section className="ps-card ps-instances">
        <div className="ps-card-head">
          <h2>Instances</h2>
          <div className="ps-form-actions">
            <button type="button" className="ps-btn-ghost" onClick={() => setImportOpen(true)}>
              Import
            </button>
            <button type="button" className="ps-btn" onClick={openCreate}>
              Create instance
            </button>
          </div>
        </div>
        <InstanceList
          instances={instances}
          onStart={handleStart}
          onStop={handleStop}
          onRemove={handleRemove}
          onWipe={handleWipe}
          onForget={handleForget}
          onSelect={setSelectedId}
          selectedId={selectedId}
          actionError={actionError}
          onActionError={setActionError}
          lanIp={lanAddr}
          strict={strict}
        />
      </section>
      <div className="ps-inspector">
        <section className="ps-card">
          <SchemaPanel instanceId={selected?.id ?? null} engine={selected?.engine} />
        </section>
        <section className="ps-card">
          {selected && health ? (
            <div
              className={`ps-health-strip ${
                health.container_running && health.tcp_open && health.query_ok
                  ? "ps-health-healthy"
                  : "ps-health-degraded"
              }`}
            >
              <span
                className={`ps-dot ${
                  health.container_running && health.tcp_open && health.query_ok
                    ? "ps-dot-running"
                    : "ps-dot-stopped"
                }`}
              />
              <span>
                {health.container_running && health.tcp_open && health.query_ok
                  ? "healthy"
                  : "degraded"}
              </span>
              <span className="ps-health-detail">
                container {health.container_running ? "running" : "down"} · tcp{" "}
                {health.tcp_open ? "open" : "closed"} · query{" "}
                {health.query_ok ? "ok" : "failing"}
              </span>
            </div>
          ) : null}
          <LogsPanel instanceId={selected?.id ?? null} origin={selected?.origin} />
        </section>
      </div>
      <ImportDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onDone={() => void refreshInstances()}
      />
      <dialog
        ref={createDialogRef}
        className="ps-dialog"
        aria-label="Create instance"
        onClick={(e) => {
          if (e.target === e.currentTarget) closeCreate();
        }}
      >
        <h2>Create instance</h2>
        {actionError ? (
          <p role="alert" className="ps-alert">
            {actionError}
          </p>
        ) : null}
        <div className="ps-dialog-grid">
          <label className="ps-field">
            <span className="ps-field-name">
              Engine
              <InfoTip
                label="About engine"
                text="Which database to run. Each instance gets its own container and data volume."
              />
            </span>
            <select
              aria-label="engine"
              value={engine}
              onChange={(e) => {
                setEngine(e.target.value);
                setTag("");
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
            <span className="ps-field-name">
              Tag
              <InfoTip
                label="About tag"
                text="Which image version to run. Different versions run side by side on different ports."
              />
            </span>
            <select
              aria-label="tag"
              value={tag || tagsForEngine[0] || ""}
              onChange={(e) => setTag(e.target.value)}
            >
              {tagsForEngine.length === 0 ? (
                <option value="">latest</option>
              ) : (
                tagsForEngine.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))
              )}
            </select>
          </label>
          <label className="ps-field">
            <span className="ps-field-name">
              Port
              <InfoTip
                label="About port"
                text="Host port to listen on. Leave blank for auto: the engine's standard port, skipping ports already taken."
              />
            </span>
            <input
              aria-label="port"
              placeholder="auto"
              value={port}
              onChange={(e) => setPort(e.target.value)}
            />
          </label>
          <label className="ps-field">
            <span className="ps-field-name">
              Password
              <InfoTip
                label="About password"
                text="Set once at creation; it can't be changed later. Blank keeps the default (portside, or empty for redis/valkey). Letters, numbers and _~-!$&'()*+,;=. only."
              />
            </span>
            <input
              aria-label="password"
              type="password"
              placeholder={engine === "redis" || engine === "valkey" ? "none" : "portside"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </label>
        </div>
        <label className="ps-check ps-dialog-lan">
          <input
            aria-label="expose on LAN"
            type="checkbox"
            checked={lan}
            onChange={(e) => setLan(e.target.checked)}
          />
          LAN (TLS)
          <InfoTip
            label="About LAN mode"
            text="Let other machines on your network connect. Binds all interfaces and encrypts with a self-signed cert. Clients verify it only when Strict TLS is on."
          />
        </label>
        {lan ? (
          <p className="ps-hint">
            LAN binds 0.0.0.0 with TLS (self-signed cert, encryption without
            verification). Laptops connect via {lanAddr ?? "this box's LAN IP"}.
          </p>
        ) : null}
        <footer className="ps-dialog-actions">
          <button type="button" className="ps-btn-ghost" onClick={closeCreate}>
            Cancel
          </button>
          <button
            type="button"
            className="ps-btn"
            onClick={() => {
              void handleCreate().then((ok) => {
                if (ok) closeCreate();
              });
            }}
          >
            Create
          </button>
        </footer>
      </dialog>
    </div>
  );
}
