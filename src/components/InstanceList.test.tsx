import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import InstanceList from "./InstanceList";

describe("InstanceList", () => {
  it("shows empty state", () => {
    render(<InstanceList instances={[]} />);
    expect(screen.getByText(/no instances/i)).toBeTruthy();
  });
});
