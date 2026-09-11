import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import App, { parsePortInput } from "./App";

describe("App", () => {
  it("renders Portside heading", () => {
    render(<App />);
    expect(screen.getByText(/portside/i)).toBeTruthy();
  });
});

describe("parsePortInput", () => {
  it("treats blank input as auto", () => {
    expect(parsePortInput("")).toBe("auto");
    expect(parsePortInput("   ")).toBe("auto");
  });

  it("accepts integers 1-65535", () => {
    expect(parsePortInput("5432")).toBe(5432);
    expect(parsePortInput("1")).toBe(1);
    expect(parsePortInput("65535")).toBe(65535);
  });

  it("rejects non-numeric, fractional, and out-of-range input", () => {
    expect(parsePortInput("abc")).toBeNull();
    expect(parsePortInput("5432abc")).toBeNull();
    expect(parsePortInput("NaN")).toBeNull();
    expect(parsePortInput("54.5")).toBeNull();
    expect(parsePortInput("0")).toBeNull();
    expect(parsePortInput("-1")).toBeNull();
    expect(parsePortInput("65536")).toBeNull();
  });
});
