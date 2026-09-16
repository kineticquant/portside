import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import InstanceList from "./InstanceList";
import type { Instance } from "../lib/tauri";

const pg: Instance = {
  id: "a1",
  engine: "postgres",
  tag: "17",
  port: 5432,
  container: "portside-a1",
  volume: "portside-a1-data",
  status: "running",
  bind_ip: "127.0.0.1",
  password: "portside",
};

describe("InstanceList", () => {
  it("shows empty state", () => {
    render(<InstanceList instances={[]} />);
    expect(screen.getByText(/no instances/i)).toBeTruthy();
  });

  it("surfaces copy failures via onActionError", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("denied")) },
      configurable: true,
    });
    const origExec = document.execCommand;
    document.execCommand = () => {
      throw new Error("no fallback");
    };
    const errors: string[] = [];
    try {
      render(<InstanceList instances={[pg]} onActionError={(m) => errors.push(m)} />);
      fireEvent.click(screen.getByText(/copy connect/i));
      await waitFor(() => expect(errors.length).toBeGreaterThan(0));
      expect(errors[0]).toMatch(/copy failed/i);
    } finally {
      delete (navigator as unknown as Record<string, unknown>)["clipboard"];
      document.execCommand = origExec;
    }
  });

  it("keeps runtime state out of the instance table header", () => {
    render(<InstanceList instances={[pg]} />);
    expect(screen.queryByText(/docker (ready|down)/i)).toBeNull();
  });
});
