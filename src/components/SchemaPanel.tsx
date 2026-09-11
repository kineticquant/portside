import { useEffect, useState } from "react";
import { api } from "../lib/tauri";

export type SchemaPanelProps = {
  instanceId: string | null;
  engine?: string;
};

export default function SchemaPanel({ instanceId, engine }: SchemaPanelProps) {
  const [databases, setDatabases] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [selectedDb, setSelectedDb] = useState<string | null>(null);
  const [schemas, setSchemas] = useState<string[]>([]);

  useEffect(() => {
    if (!instanceId) return;
    let cancelled = false;
    setError(null);
    setInfo(null);
    setSelectedDb(null);
    setSchemas([]);
    if (engine === "redis" || engine === "valkey") {
      api
        .redisInfo(instanceId)
        .then((r) => {
          if (!cancelled) setInfo(r);
        })
        .catch((e) => {
          if (!cancelled) setError(String(e));
        });
      setDatabases([]);
      return () => {
        cancelled = true;
      };
    }
    api
      .databases(instanceId)
      .then((dbs) => {
        if (!cancelled) setDatabases(dbs);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [instanceId, engine]);

  useEffect(() => {
    if (!instanceId || engine !== "postgres" || !selectedDb) return;
    let cancelled = false;
    api
      .schemas(instanceId, selectedDb)
      .then((s) => {
        if (!cancelled) setSchemas(s);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [instanceId, engine, selectedDb]);

  if (!instanceId) {
    return <p>Select an instance to manage schemas.</p>;
  }

  if (engine === "redis" || engine === "valkey") {
    return (
      <div>
        <h2>Schemas</h2>
        <p>Redis has no schemas; showing keyspace info instead.</p>
        {info ? <pre>{info}</pre> : null}
        {error ? <p role="alert">{error}</p> : null}
      </div>
    );
  }

  const refresh = async () => {
    try {
      setDatabases(await api.databases(instanceId));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleCreate = async () => {
    if (!name.trim()) return;
    try {
      await api.createDb(instanceId, name.trim());
      setName("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDrop = async (db: string) => {
    try {
      await api.dropDb(instanceId, db);
      if (db === selectedDb) {
        setSelectedDb(null);
        setSchemas([]);
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div>
      <h2>Schemas</h2>
      {error ? <p role="alert">{error}</p> : null}
      {databases.length === 0 ? (
        <p>No databases found.</p>
      ) : (
        <ul>
          {databases.map((db) => (
            <li key={db}>
              {db}{" "}
              {engine === "postgres" ? (
                <button
                  type="button"
                  onClick={() => {
                    setSelectedDb(db);
                    setSchemas([]);
                  }}
                >
                  View schemas
                </button>
              ) : null}{" "}
              <button type="button" onClick={() => void handleDrop(db)}>
                Drop
              </button>
            </li>
          ))}
        </ul>
      )}
      {engine === "postgres" && selectedDb ? (
        <div>
          <h3>Schemas in {selectedDb}</h3>
          <p>Schema list is read-only.</p>
          {schemas.length === 0 ? (
            <p>No schemas found.</p>
          ) : (
            <ul>
              {schemas.map((s) => (
                <li key={s}>{s}</li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
      <div>
        <input
          aria-label="database name"
          placeholder="new database name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="button" onClick={() => void handleCreate()}>
          Create
        </button>
      </div>
    </div>
  );
}
