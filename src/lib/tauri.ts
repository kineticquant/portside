import { invoke } from "@tauri-apps/api/core";

export type Instance = {
  id: string;
  engine: string;
  tag: string;
  port: number;
  container: string;
  volume: string;
  status: string;
};

export type CatalogEntry = {
  engine: string;
  tag: string;
  image: string;
};

export type Catalog = {
  updated?: string;
  entries: CatalogEntry[];
};

export function connectString(i: Pick<Instance, "engine" | "port">): string {
  if (i.engine === "postgres") {
    return `postgres://postgres:portside@127.0.0.1:${i.port}/postgres`;
  }
  if (i.engine === "redis" || i.engine === "valkey") {
    return `redis://127.0.0.1:${i.port}/0`;
  }
  return `mysql://root:portside@127.0.0.1:${i.port}/mysql`;
}

export const api = {
  detectRuntime: () => invoke<string>("detect_runtime"),
  list: () => invoke<Instance[]>("list_instances"),
  create: (engine: string, tag: string, port: number) =>
    invoke<Instance>("create_instance", { engine, tag, port }),
  start: (id: string) => invoke<void>("start_instance", { id }),
  stop: (id: string) => invoke<void>("stop_instance", { id }),
  remove: (id: string, wipe_data: boolean) =>
    invoke<void>("remove_instance", { id, wipe_data }),
  catalog: () => invoke<Catalog>("get_catalog"),
  suggestPort: (engine: string, taken: number[]) =>
    invoke<number>("suggest_port", { engine, taken }),
  databases: (id: string) => invoke<string[]>("list_databases", { id }),
  createDb: (id: string, name: string) =>
    invoke<void>("create_database", { id, name }),
  dropDb: (id: string, name: string) =>
    invoke<void>("drop_database", { id, name }),
  schemas: (id: string, db: string) =>
    invoke<string[]>("list_schemas", { id, db }),
  redisInfo: (id: string) => invoke<string>("redis_info", { id }),
  connectString,
};
