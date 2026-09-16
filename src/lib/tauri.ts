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
};

export function isLan(i: Pick<Instance, "bind_ip">): boolean {
  return i.bind_ip !== "127.0.0.1" && i.bind_ip !== "localhost";
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
 */
export function connectString(
  i: Pick<Instance, "engine" | "port"> & {
    password?: string;
    bind_ip?: string;
  },
  opts: ConnectOpts = {},
): string {
  const host = opts.host ?? "127.0.0.1";
  const pw = i.password || defaultPassword(i.engine);
  const userinfo = pw ? `${userFor(i.engine)}:${pw}@` : `${userFor(i.engine)}@`;
  const tls = "bind_ip" in i ? isLan(i as Pick<Instance, "bind_ip">) : false;
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
  prereqs: () => invoke<Prereqs>("check_prereqs"),
  installWsl: () => invoke<string>("install_wsl"),
  startDocker: () => invoke<string>("start_docker"),
  connectString,
};
