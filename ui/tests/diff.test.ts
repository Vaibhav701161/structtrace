import { describe, expect, it } from "vitest";
import { structuralDiff } from "../src/features/results/Cases";
import { strictJsonParse } from "../src/lib/lossless-json";

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

  it("keeps adjacent unsafe integers distinct in the case diff", () => {
    const expected = strictJsonParse('{"answer":9007199254740992}');
    const actual = strictJsonParse('{"answer":9007199254740993}');
    expect(structuralDiff(expected, actual)).toContainEqual(expect.objectContaining({ path: "/answer", value: "9007199254740993", state: "changed" }));
  });
});
