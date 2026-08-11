import { describe, expect, it } from "vitest";
import { structuralDiff } from "../src/features/results/Cases";

describe("structured case diff", () => {
  it("distinguishes additions, removals, value changes, and type changes", () => {
    const rows = structuralDiff({ kept: 1, removed: "x", changed: 2, typed: 3 }, { kept: 1, added: true, changed: 4, typed: "3" });
    expect(Object.fromEntries(rows.map((row) => [row.path, row.state]))).toEqual({
      "/added": "added", "/changed": "changed", "/kept": "unchanged", "/removed": "removed", "/typed": "type_changed",
    });
  });

  it("matches common keyed-array items before comparing their fields", () => {
    const rows = structuralDiff({ items: [{ sku: "A", quantity: 1 }, { sku: "B", quantity: 2 }] }, { items: [{ sku: "B", quantity: 2 }, { sku: "A", quantity: 1 }] });
    expect(rows.every((row) => row.state === "unchanged")).toBe(true);
  });
});
