import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import ImportDialog from "./ImportDialog";
import { api } from "../lib/tauri";

vi.mock("../lib/tauri", () => ({
  api: {
    probe: vi.fn(),
    importExternal: vi.fn(),
    adoptable: vi.fn(),
    adopt: vi.fn(),
    pgadmin: vi.fn(),
  },
}));

const probe = api.probe as unknown as ReturnType<typeof vi.fn>;

describe("ImportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders nothing when closed", () => {
    const { container } = render(
      <ImportDialog open={false} onClose={() => {}} onDone={() => {}} />,
    );
    expect(container.textContent).toBe("");
  });

  it("shows connection, docker, and pgAdmin tabs", () => {
    render(<ImportDialog open onClose={() => {}} onDone={() => {}} />);
    expect(screen.getByRole("tab", { name: /connection/i })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /docker/i })).toBeTruthy();
    expect(screen.getByRole("tab", { name: /pgadmin/i })).toBeTruthy();
  });

  it("probes before saving a manual connection", async () => {
    probe.mockResolvedValue({ ok: true, version: "16.4", error: null });
    const onDone = vi.fn();
    render(<ImportDialog open onClose={() => {}} onDone={onDone} />);
    fireEvent.change(screen.getByLabelText(/host/i), { target: { value: "db.lan" } });
    fireEvent.click(screen.getByRole("button", { name: /test connection/i }));
    await waitFor(() => expect(screen.getByText(/16\.4/)).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("adopts a listed container", async () => {
    const adopt = api.adopt as unknown as ReturnType<typeof vi.fn>;
    (api.adoptable as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      { container: "my-pg", engine: "postgres", tag: "15", port: 5433, volume: "", status: "running" },
    ]);
    adopt.mockResolvedValue({});
    const onDone = vi.fn();
    render(<ImportDialog open onClose={() => {}} onDone={onDone} />);
    fireEvent.click(screen.getByRole("tab", { name: /docker/i }));
    await waitFor(() => expect(screen.getByText("my-pg")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /adopt/i }));
    await waitFor(() => {
      expect(adopt).toHaveBeenCalledWith("my-pg");
      expect(onDone).toHaveBeenCalled();
    });
  });

  it("explains when pgAdmin has nothing to import", async () => {
    (api.pgadmin as unknown as ReturnType<typeof vi.fn>).mockRejectedValue(
      new Error("pgAdmin server list not found on this machine"),
    );
    render(<ImportDialog open onClose={() => {}} onDone={() => {}} />);
    fireEvent.click(screen.getByRole("tab", { name: /pgadmin/i }));
    await waitFor(() =>
      expect(screen.getByText(/pgadmin server list not found/i)).toBeTruthy(),
    );
  });
});
