import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import AgentPanel from "./AgentPanel";

describe("AgentPanel", () => {
  it("renders the PATH toggle and every harness button", () => {
    render(<AgentPanel />);
    expect(screen.getByRole("heading", { name: "Agents" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /CLI to PATH/ }),
    ).toBeTruthy();
    for (const host of [
      "opencode",
      "claude",
      "codex",
      "cursor",
      "windsurf",
      "pi",
      "omp",
      "other",
    ]) {
      expect(screen.getByRole("button", { name: host })).toBeTruthy();
    }
  });
});
