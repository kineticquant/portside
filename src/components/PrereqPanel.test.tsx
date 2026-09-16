import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import PrereqPanel from "./PrereqPanel";
import type { Prereqs } from "../lib/tauri";

const ok: Prereqs = {
  wsl: true,
  docker_cli: true,
  docker_daemon: true,
  docker_server_version: "27.0",
  vc_redist: true,
  webview2: true,
};

describe("PrereqPanel", () => {
  it("renders nothing when everything is ready", () => {
    const { container } = render(
      <PrereqPanel prereqs={ok} onRefresh={() => {}} onNotice={() => {}} />,
    );
    expect(container.textContent).toBe("");
  });

  it("offers WSL install when missing", () => {
    render(
      <PrereqPanel
        prereqs={{ ...ok, wsl: false }}
        onRefresh={() => {}}
        onNotice={() => {}}
      />,
    );
    expect(screen.getByText(/install wsl2/i)).toBeTruthy();
    expect(screen.getByText(/10\+ minutes/i)).toBeTruthy();
  });

  it("shows docker download url when cli is missing", () => {
    render(
      <PrereqPanel
        prereqs={{ ...ok, docker_cli: false, docker_daemon: false }}
        onRefresh={() => {}}
        onNotice={() => {}}
      />,
    );
    expect(screen.getByText(/docker\.com\/products\/docker-desktop/)).toBeTruthy();
  });

  it("nudges to start docker when daemon is down", () => {
    render(
      <PrereqPanel
        prereqs={{ ...ok, docker_daemon: false }}
        onRefresh={vi.fn()}
        onNotice={() => {}}
      />,
    );
    // Stable copy (the Start button label flips to "Starting…" while the
    // one automatic start attempt runs on mount).
    expect(screen.getByText(/daemon is not running/i)).toBeTruthy();
  });
});
