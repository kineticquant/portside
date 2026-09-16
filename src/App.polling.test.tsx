import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, cleanup } from "@testing-library/react";
import App from "./App";
import type { Prereqs } from "./lib/tauri";

const base: Prereqs = {
  wsl: true,
  docker_cli: true,
  docker_daemon: false,
  docker_server_version: null,
  vc_redist: true,
  webview2: true,
};

let prereqCalls = 0;
let runtimeCalls = 0;

// Daemon comes up on the 3rd check: down, down, then up.
vi.mock("./lib/tauri", () => ({
  DOCKER_DOWNLOAD_URL: "https://www.docker.com/products/docker-desktop/",
  api: {
    detectRuntime: () => {
      runtimeCalls += 1;
      return runtimeCalls >= 3
        ? Promise.resolve("ok")
        : Promise.reject(new Error("daemon down"));
    },
    list: () => Promise.resolve([]),
    catalog: () => Promise.resolve({ entries: [] }),
    lanAddr: () => Promise.resolve(null),
    startDocker: () => Promise.reject(new Error("auto-start denied")),
    installWsl: () => Promise.resolve("wsl ok"),
    suggestPort: () => Promise.resolve(5432),
    create: () => Promise.resolve({}),
    start: () => Promise.resolve(),
    stop: () => Promise.resolve(),
    remove: () => Promise.resolve(),
    databases: () => Promise.resolve([]),
    serverCert: () => Promise.resolve(""),
    health: () =>
      Promise.resolve({
        container_running: false,
        tcp_open: false,
        query_ok: false,
      }),
    connectString: () => "",
  },
}));

describe("App docker polling", () => {
  beforeEach(() => {
    prereqCalls = 0;
    runtimeCalls = 0;
    vi.useFakeTimers();
    // api.prereqs is defined here so the counter resets with the module mock.
    void base;
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("keeps checking until the daemon is up, then clears the notice", async () => {
    const { api } = await import("./lib/tauri");
    // Swap in the counting prereqs checker (daemon up on 3rd call).
    (api as unknown as Record<string, unknown>)["prereqs"] = () => {
      prereqCalls += 1;
      return Promise.resolve({
        ...base,
        docker_daemon: prereqCalls >= 3,
      });
    };

    render(<App />);
    // Flush mount effects: prereqs check, runtime detect, auto-start attempt.
    await act(async () => {});

    // Daemon down: setup panel nags, auto-start failure notice shows.
    expect(screen.getByText(/daemon is not running/i)).toBeTruthy();
    expect(screen.getByText(/auto-start denied/i)).toBeTruthy();

    // Two poll ticks: still down after the first, up after the second.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(11000);
    });
    await act(async () => {});

    // Daemon up: setup panel gone, stale notice cleared.
    expect(screen.queryByText(/daemon is not running/i)).toBeNull();
    expect(screen.queryByText(/auto-start denied/i)).toBeNull();
  });
});
