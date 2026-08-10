import { z } from "zod";

export const gateModeSchema = z.enum(["advisory", "regression", "release"]);
export type GateMode = z.infer<typeof gateModeSchema>;

export const sourceKindSchema = z.enum(["dataset", "baseline", "candidate", "schema"]);
export type SourceKind = z.infer<typeof sourceKindSchema>;

export interface SourceArtifact {
  sourceId: string;
  hash: string;
  kind: SourceKind;
  name: string;
  format: "jsonl" | "json" | "csv";
  content: string;
  bytes: number;
  rows: number;
  status: "ready" | "staging" | "error";
  message?: string;
}

export interface FieldRule {
  pointer: string;
  kind:
    | "exact"
    | "normalized_string"
    | "canonical_date"
    | "exact_integer"
    | "decimal_exact"
    | "decimal_tolerance"
    | "keyed_array"
    | "required_fields";
  enabled: boolean;
  tolerance?: string;
  keys?: string;
  fields?: string;
  formats?: string;
  caseInsensitive?: boolean;
  expectedCoverage: number;
  baselineCoverage: number;
  candidateCoverage: number;
  observedType: string;
}

export interface Mapping {
  datasetId: string;
  datasetInput: string;
  datasetExpected: string;
  baselineId: string;
  baselineOutput: string;
  candidateId: string;
  candidateOutput: string;
  baselineStatus?: string; baselineError?: string; baselineLatency?: string; baselineUsage?: string; baselineCost?: string; baselineMetadata?: string;
  candidateStatus?: string; candidateError?: string; candidateLatency?: string; candidateUsage?: string; candidateCost?: string; candidateMetadata?: string;
}

export interface ComparisonDraft {
  projectId: string;
  name: string;
  baselineName: string;
  candidateName: string;
  sources: Partial<Record<SourceKind, SourceArtifact>>;
  mapping: Mapping;
  rules: FieldRule[];
  gateMode: GateMode;
  minCases: number;
  financialInvariants: boolean;
}

export const systemResponseSchema = z.object({
  product: z.literal("StructTrace"),
  version: z.string(),
  localOnly: z.literal(true),
  telemetry: z.literal(false),
  maxUploadBytes: z.number(),
  apiVersion: z.literal("v1"),
});
export type SystemResponse = z.infer<typeof systemResponseSchema>;

const variantSummarySchema = z.object({
  total: z.number(),
  parse_valid: z.number(),
  schema_valid: z.number(),
  semantic_success: z.number(),
  deployment_success: z.number(),
  valid_but_wrong: z.number(),
  errors: z.number(),
});

const pairedSchema = z.object({
  both_pass: z.number(),
  baseline_only_pass: z.number(),
  candidate_only_pass: z.number(),
  both_fail: z.number(),
  difference_pp: z.number(),
  mcnemar_exact_p: z.number(),
});

export const runResultSchema = z.object({
  runId: z.string(),
  projectId: z.string().nullable().optional(),
  projectName: z.string(),
  createdAt: z.number(),
  runDir: z.string().optional(),
  summary: z.object({
    baseline: variantSummarySchema,
    candidate: variantSummarySchema,
    paired: pairedSchema,
    bootstrap: z.object({ lower_pp: z.number(), upper_pp: z.number() }),
    evidence: z.object({
      total_rows: z.number(),
      effective_inference_units: z.number(),
      exact_duplicate_groups: z.number(),
      repeated_trial_groups: z.number(),
      label_conflict_groups: z.number(),
    }),
    gate: z.object({
      gate_mode: gateModeSchema,
      status: z.enum(["passed", "failed", "not_configured", "insufficient_evidence", "error"]),
      deployment_authorized: z.boolean(),
      quality_failures: z.array(z.string()),
      evidence_failures: z.array(z.string()),
      runtime_errors: z.array(z.string()),
    }),
    primary_field_hotspots: z.array(z.object({
      evaluator_id: z.string(),
      pointer: z.string(),
      regressions: z.number(),
      improvements: z.number(),
      candidate_failures: z.number(),
    })),
  }),
  cases: z.array(z.unknown()).default([]),
  schemaProvenance: z.enum(["caller_supplied", "inferred_from_expected_values"]).optional(),
});
export type RunResult = z.infer<typeof runResultSchema>;

export const acceptedBaselineSchema = z.object({
  accepted: z.object({ runId: z.string(), projectId: z.string(), acceptedAt: z.number(), candidateArtifactHash: z.string(), sourceId: z.string() }),
  source: z.object({ sourceId: z.string(), hash: z.string(), name: z.string(), format: z.enum(["jsonl", "json", "csv"]), content: z.string(), bytes: z.number() }),
});
export type AcceptedBaselineResponse = z.infer<typeof acceptedBaselineSchema>;

export const projectSummarySchema = z.object({ projectId: z.string(), name: z.string(), runCount: z.number(), updatedAt: z.number() });
export type ProjectSummary = z.infer<typeof projectSummarySchema>;

export const casePageSchema = z.object({
  items: z.array(z.unknown()),
  total: z.number(),
  offset: z.number(),
  limit: z.number(),
});

export const pinnedCaseSchema = z.object({
  id: z.string(),
  runId: z.string(),
  caseId: z.string(),
  projectName: z.string(),
  pinnedAt: z.number(),
  note: z.string().default(""),
  status: z.enum(["open", "fixed"]).default("open"),
});
export type PinnedCase = z.infer<typeof pinnedCaseSchema>;

export interface ComparisonRequest {
  projectId: string;
  name: string;
  baselineName: string;
  candidateName: string;
  files: {
    dataset: Pick<SourceArtifact, "sourceId">;
    baseline: Pick<SourceArtifact, "sourceId">;
    candidate: Pick<SourceArtifact, "sourceId">;
    schema?: Pick<SourceArtifact, "sourceId">;
  };
  mapping: Mapping;
    rules: Array<Pick<FieldRule, "pointer" | "kind" | "tolerance" | "keys" | "fields" | "formats" | "caseInsensitive">>;
  gateMode: GateMode;
  minCases: number;
  financialInvariants: boolean;
}

const sourceArtifactSchema = z.object({
  sourceId: z.string(),
  hash: z.string(),
  kind: sourceKindSchema,
  name: z.string(),
  format: z.enum(["jsonl", "json", "csv"]),
  content: z.string(),
  bytes: z.number(),
  rows: z.number(),
  status: z.enum(["ready", "staging", "error"]),
  message: z.string().optional(),
});

const fieldRuleSchema = z.object({
  pointer: z.string(),
  kind: z.enum(["exact", "normalized_string", "canonical_date", "exact_integer", "decimal_exact", "decimal_tolerance", "keyed_array", "required_fields"]),
  enabled: z.boolean(),
  tolerance: z.string().optional(),
  keys: z.string().optional(),
  fields: z.string().optional(),
  formats: z.string().optional(),
  caseInsensitive: z.boolean().optional(),
  expectedCoverage: z.number(),
  baselineCoverage: z.number(),
  candidateCoverage: z.number(),
  observedType: z.string(),
});

export const comparisonDraftSchema: z.ZodType<ComparisonDraft> = z.object({
  projectId: z.string(),
  name: z.string(),
  baselineName: z.string(),
  candidateName: z.string(),
  sources: z.object({
    dataset: sourceArtifactSchema.optional(),
    baseline: sourceArtifactSchema.optional(),
    candidate: sourceArtifactSchema.optional(),
    schema: sourceArtifactSchema.optional(),
  }),
  mapping: z.object({
    datasetId: z.string(), datasetInput: z.string(), datasetExpected: z.string(),
    baselineId: z.string(), baselineOutput: z.string(), candidateId: z.string(), candidateOutput: z.string(),
    baselineStatus: z.string().optional(), baselineError: z.string().optional(), baselineLatency: z.string().optional(), baselineUsage: z.string().optional(), baselineCost: z.string().optional(), baselineMetadata: z.string().optional(),
    candidateStatus: z.string().optional(), candidateError: z.string().optional(), candidateLatency: z.string().optional(), candidateUsage: z.string().optional(), candidateCost: z.string().optional(), candidateMetadata: z.string().optional(),
  }),
  rules: z.array(fieldRuleSchema),
  gateMode: gateModeSchema,
  minCases: z.number(),
  financialInvariants: z.boolean(),
});
