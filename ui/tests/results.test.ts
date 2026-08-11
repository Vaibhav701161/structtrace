import { describe, expect, it } from "vitest";
import { aggregateHotspots } from "../src/features/results/Results";

describe("result visualization", () => {
  it("combines repeated evaluator findings for the same field", () => {
    expect(aggregateHotspots([
      { pointer: "/total", regressions: 2 },
      { pointer: "/subtotal", regressions: 1 },
      { pointer: "/total", regressions: 1 },
    ])).toEqual([
      { pointer: "/total", regressions: 3 },
      { pointer: "/subtotal", regressions: 1 },
    ]);
  });
});
