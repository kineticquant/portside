import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import SchemaPanel from "./SchemaPanel";

afterEach(() => cleanup());

vi.mock("../lib/tauri", () => ({
  api: {
    databases: vi.fn(async () => ["appdb"]),
    schemas: vi.fn(async () => ["public", "app"]),
    createDb: vi.fn(async () => {}),
    dropDb: vi.fn(async () => {}),
  },
}));

describe("SchemaPanel postgres schemas", () => {
  it("lists schemas for the selected database", async () => {
    render(<SchemaPanel instanceId="abc" engine="postgres" />);
    await waitFor(() => expect(screen.getByText("appdb")).toBeTruthy());
    fireEvent.click(screen.getByText("View schemas"));
    await waitFor(() => expect(screen.getByText("public")).toBeTruthy());
    expect(screen.getByText("app")).toBeTruthy();
  });

  it("shows an empty state when a database has no schemas", async () => {
    const { api } = await import("../lib/tauri");
    vi.mocked(api.schemas).mockResolvedValueOnce([]);
    render(<SchemaPanel instanceId="abc" engine="postgres" />);
    await waitFor(() => expect(screen.getByText("appdb")).toBeTruthy());
    fireEvent.click(screen.getByText("View schemas"));
    await waitFor(() => expect(screen.getByText(/no schemas found/i)).toBeTruthy());
  });
});
