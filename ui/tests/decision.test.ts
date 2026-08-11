import { describe, expect, it } from "vitest";
import { decision, effectPosition } from "../src/features/results/Results";
import type { RunResult } from "../src/api/types";

function result(gate: RunResult["summary"]["gate"]): RunResult {
  return {
    runId: "run",
    projectName: "project",
    createdAt: 0,
    integrity: { status: "verified", detail: "Replay verified." },
    regressionSuite: { total: 0, passing: 0, fixed: 0, stillBroken: 0, reintroduced: 0, missing: 0, blocking: false },
    cases: [],
    summary: {
      baseline: { total: 1, parse_valid: 1, schema_valid: 1, semantic_success: 1, deployment_success: 1, valid_but_wrong: 0, errors: 0 },
      candidate: { total: 1, parse_valid: 1, schema_valid: 1, semantic_success: 1, deployment_success: 1, valid_but_wrong: 0, errors: 0 },
      descriptive_baseline: { total: 1, parse_valid: 1, schema_valid: 1, semantic_success: 1, deployment_success: 1, valid_but_wrong: 0, errors: 0 },
      descriptive_candidate: { total: 1, parse_valid: 1, schema_valid: 1, semantic_success: 1, deployment_success: 1, valid_but_wrong: 0, errors: 0 },
      primary_jointly_scored: 1,
      paired: { both_pass: 1, baseline_only_pass: 0, candidate_only_pass: 0, both_fail: 0, difference_pp: 0, mcnemar_exact_p: 1 },
      jointly_scored_semantic: { jointly_scored_cases: 1, excluded_pairs: 0, paired: { both_pass: 1, baseline_only_pass: 0, candidate_only_pass: 0, both_fail: 0, difference_pp: 0, mcnemar_exact_p: 1 } },
      bootstrap: { lower_pp: 0, upper_pp: 0 },
      evidence: { total_rows: 1, effective_inference_units: 1, exact_duplicate_groups: 0, repeated_trial_groups: 0, label_conflict_groups: 0 },
      gate,
      primary_field_hotspots: [],
    },
  };
}

describe("decision language", () => {
  it("never presents a passed regression check as release authorization", () => {
    const value = decision(result({ gate_mode: "regression", status: "passed", deployment_authorized: false, quality_failures: [], evidence_failures: [], runtime_errors: [] }));
    expect(value.title).toBe("REGRESSION CHECK PASSED");
    expect(value.text).toContain("not release authorization");
  });

  it("reserves release authorization for an authorizing release gate", () => {
    const value = decision(result({ gate_mode: "release", status: "passed", deployment_authorized: true, quality_failures: [], evidence_failures: [], runtime_errors: [] }));
    expect(value.title).toBe("RELEASE AUTHORIZED");
  });

  it("surfaces a quality failure even when evidence is also insufficient", () => {
    const value = decision(result({ gate_mode: "release", status: "insufficient_evidence", deployment_authorized: false, quality_failures: ["deployment regression exceeded"], evidence_failures: ["too few cases"], runtime_errors: [] }));
    expect(value.title).toBe("DO NOT DEPLOY");
    expect(value.text).toContain("Quality thresholds failed");
    expect(value.text).toContain("Evidence requirements are also insufficient");
  });
});

describe("full-range effect scale", () => {
  it.each([[-100, 0], [-75, 12.5], [0, 50], [75, 87.5], [100, 100]])("maps %s pp to %s%%", (effect, expected) => {
    expect(effectPosition(effect)).toBe(expected);
  });
});
