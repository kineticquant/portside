import { invoke } from "@tauri-apps/api/core";

export type Instance = {
  id: string;
  engine: string;
  tag: string;
  port: number;
  container: string;
  volume: string;
  status: string;
  bind_ip: string;
  password: string;
  /** "managed" (Portside-created) | "imported" (external) | "adopted" (container). */
  origin: string;
  /** Connection host: loopback for managed rows, the real host otherwise. */
  host: string;
  /** Connection user: "" means the engine default (root / postgres). */
  db_user: string;
  /** TLS posture for non-managed rows: "" | "off" | "require". */
  ssl: string;
};

export function isLan(i: Pick<Instance, "bind_ip">): boolean {
  return i.bind_ip !== "127.0.0.1" && i.bind_ip !== "localhost";
}

export function isExternal(i: Pick<Instance, "origin">): boolean {
  return i.origin === "imported" || i.origin === "adopted";
}

function defaultUser(engine: string): string {
  return engine === "postgres" ? "postgres" : "root";
}

function defaultPassword(engine: string): string {
  return engine === "redis" || engine === "valkey" ? "" : "portside";
}

export type CatalogEntry = {
  engine: string;
  tag: string;
  image: string;
};

export type Catalog = {
  updated?: string;
  entries: CatalogEntry[];
};

export type ProbeResult = {
  ok: boolean;
  version: string | null;
  error: string | null;
};

export type Adoptable = {
  container: string;
  engine: string;
  tag: string;
  port: number | null;
  volume: string;
  status: string;
};

export type PgServer = {
  name: string;
  host: string;
  port: number;
  username: string;
  database: string;
};

export type Health = {
  container_running: boolean;
  tcp_open: boolean;
  query_ok: boolean;
};

export type Prereqs = {
  wsl: boolean;
  docker_cli: boolean;
  docker_daemon: boolean;
  docker_server_version: string | null;
  vc_redist: boolean;
  webview2: boolean;
};

export const DOCKER_DOWNLOAD_URL =
  "https://www.docker.com/products/docker-desktop/";

export type ConnectOpts = {
  /** Host to advertise. Defaults to loopback; pass the LAN IP for laptops. */
  host?: string;
  /** Strict verify-ca mode (needs the downloaded server cert client-side). */
  strict?: boolean;
};

/**
 * pgAdmin posture by default: TLS when the instance serves it, verification
 * off. Strict mode appends verify-ca parameters (pair with Cert download).
 * Imported rows point at their own host/user and use their ssl flag.
 */
export function connectString(
  i: Pick<Instance, "engine" | "port"> & {
    password?: string;
    bind_ip?: string;
    host?: string;
    db_user?: string;
    ssl?: string;
    origin?: string;
  },
  opts: ConnectOpts = {},
): string {
  const host = opts.host ?? i.host ?? "127.0.0.1";
  const pw = i.password || defaultPassword(i.engine);
  const user = i.db_user || userFor(i.engine);
  const userinfo = pw ? `${user}:${pw}@` : `${user}@`;
  const tls =
    i.origin === "imported" || i.origin === "adopted"
      ? i.ssl === "require"
      : "bind_ip" in i
        ? isLan(i as Pick<Instance, "bind_ip">)
        : false;
  if (i.engine === "postgres") {
    const ssl = tls
      ? opts.strict
        ? "?sslmode=verify-ca"
        : "?sslmode=require"
      : "";
    return `postgres://${userinfo}${host}:${i.port}/postgres${ssl}`;
  }
  if (i.engine === "redis" || i.engine === "valkey") {
    const scheme = tls ? "rediss" : "redis";
    const auth = pw ? `:${pw}@` : "";
    return `${scheme}://${auth}${host}:${i.port}/0`;
  }
  const ssl = tls
    ? opts.strict
      ? "?ssl-mode=VERIFY_CA"
      : "?ssl-mode=REQUIRED"
    : "";
  return `mysql://${userinfo}${host}:${i.port}/mysql${ssl}`;
}

function userFor(engine: string): string {
  return engine === "postgres" ? "postgres" : "root";
}

export const api = {
  detectRuntime: () => invoke<string>("detect_runtime"),
  list: () => invoke<Instance[]>("list_instances"),
  create: (
    engine: string,
    tag: string,
    port: number,
    bind_ip = "127.0.0.1",
    password = "",
  ) =>
    invoke<Instance>("create_instance", {
      engine,
      tag,
      port,
      bindIp: bind_ip,
      password,
    }),
  lanAddr: () => invoke<string | null>("lan_addr"),
  serverCert: (id: string) => invoke<string>("server_cert", { id }),
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
  logs: (id: string, tail = 200) =>
    invoke<string>("container_logs", { id, tail }),
  health: (id: string) => invoke<Health>("health", { id }),
  probe: (engine: string, host: string, port: number, user: string, password: string, ssl: string) =>
    invoke<ProbeResult>("probe_connection", { engine, host, port, user, password, ssl }),
  importExternal: (engine: string, host: string, port: number, user: string, password: string, ssl: string) =>
    invoke<Instance>("import_external", { engine, host, port, user, password, ssl }),
  adoptable: () => invoke<Adoptable[]>("list_adoptable"),
  adopt: (container: string) => invoke<Instance>("adopt_container", { container }),
  pgadmin: () => invoke<PgServer[]>("list_pgadmin_servers"),
  forget: (id: string) => invoke<void>("forget_instance", { id }),
  prereqs: () => invoke<Prereqs>("check_prereqs"),
  installWsl: () => invoke<string>("install_wsl"),
  startDocker: () => invoke<string>("start_docker"),
  connectString,
};
