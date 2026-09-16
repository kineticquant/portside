import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import InfoTip from "./InfoTip";

describe("InfoTip", () => {
  it("renders an accessible button with its explainer", () => {
    render(<InfoTip label="About engine" text="Each instance runs in its own container." />);
    expect(screen.getByRole("button", { name: "About engine" })).toBeTruthy();
    expect(screen.getByRole("tooltip").textContent).toMatch(/own container/);
  });

  it("right-aligns the tooltip near viewport edges", () => {
    render(<InfoTip label="About strict TLS" text="LAN only." align="right" />);
    expect(screen.getByText("LAN only.").className).toMatch(/ps-tip-right/);
  });

  it("opens downward when asked (triggers near the top edge)", () => {
    render(<InfoTip label="About strict TLS" text="Opens below." direction="down" />);
    expect(screen.getByText("Opens below.").className).toMatch(/ps-tip-down/);
  });
});
