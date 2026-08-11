import { describe, expect, it } from "vitest";
import { aggregateHotspots, formatEffect } from "../src/features/results/Results";
import { primaryOutcomeState } from "../src/features/results/Cases";

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

  it("renders every primary-outcome truth state without collapsing it to wrong", () => {
    const evaluation = (truth: string, fullyEvaluated = false) => ({
      primary_outcome: {
        truth,
        fully_evaluated: fullyEvaluated,
        component_errors: truth === "error" ? 1 : 0,
        component_not_applicable: truth === "not_applicable" ? 1 : 0,
        component_unscored: truth === "unscored" ? 1 : 0,
        evaluator_ids: ["primary"],
      },
    }) as Parameters<typeof primaryOutcomeState>[0];
    expect(primaryOutcomeState(evaluation("true", true))).toEqual({ label: "Correct", kind: "pass" });
    expect(primaryOutcomeState(evaluation("false", true))).toEqual({ label: "Wrong", kind: "fail" });
    expect(primaryOutcomeState(evaluation("false", false))).toEqual({ label: "Incomplete", kind: "incomplete" });
    expect(primaryOutcomeState(evaluation("error"))).toEqual({ label: "Evaluation error", kind: "error" });
    expect(primaryOutcomeState(evaluation("not_applicable"))).toEqual({ label: "N/A", kind: "na" });
    expect(primaryOutcomeState(evaluation("unscored"))).toEqual({ label: "Unscored", kind: "unscored" });
  });

  it("never formats missing evidence as an exact zero effect", () => {
    expect(formatEffect(null)).toBe("Not estimable");
    expect(formatEffect(null)).not.toContain("0.0");
    expect(formatEffect(0)).toBe("+0.0 pp");
  });
});
