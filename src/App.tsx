import { useEffect, useState } from "react";
import InstanceList from "./components/InstanceList";
import SchemaPanel from "./components/SchemaPanel";
import { api, type CatalogEntry, type Instance } from "./lib/tauri";

const ENGINES = ["mysql", "mariadb", "postgres", "redis", "valkey"];

export default function App() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [engine, setEngine] = useState("postgres");
  const [tag, setTag] = useState("");
  const [port, setPort] = useState("");

  const refreshInstances = async () => {
    try {
      const list = await api.list();
      setInstances(list);
      setActionError(null);
    } catch (e) {
      setActionError(String(e));
    }
  };

  useEffect(() => {
    let cancelled = false;
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

  const handleCreate = async () => {
    setActionError(null);
    try {
      const taken = instances.map((i) => i.port);
      const portNum = port.trim()
        ? Number(port)
        : await api.suggestPort(engine, taken);
      const chosenTag = tag || tagsForEngine[0] || "latest";
      await api.create(engine, chosenTag, portNum);
      setPort("");
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
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

  const handleRemove = async (id: string) => {
    try {
      await api.remove(id, false);
      if (selectedId === id) setSelectedId(null);
      await refreshInstances();
    } catch (e) {
      setActionError(String(e));
    }
  };

  const selected = instances.find((i) => i.id === selectedId) ?? null;

  return (
    <div>
      <h1>Portside</h1>
      {runtimeError ? (
        <p role="alert">
          Docker runtime not detected. Is Docker Desktop running? ({runtimeError})
        </p>
      ) : null}
      <section>
        <h2>Create instance</h2>
        <label>
          Engine
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
        <label>
          Tag
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
        <label>
          Port
          <input
            aria-label="port"
            placeholder="auto"
            value={port}
            onChange={(e) => setPort(e.target.value)}
          />
        </label>
        <button type="button" onClick={() => void handleCreate()}>
          Create
        </button>
      </section>
      <section>
        <h2>Instances</h2>
        <InstanceList
          instances={instances}
          onStart={handleStart}
          onStop={handleStop}
          onRemove={handleRemove}
          onSelect={setSelectedId}
          selectedId={selectedId}
          actionError={actionError}
        />
      </section>
      <section>
        <SchemaPanel instanceId={selected?.id ?? null} engine={selected?.engine} />
      </section>
    </div>
  );
}
