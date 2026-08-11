import { acceptedBaselineSchema, casePageSchema, comparisonDraftSchema, fieldInventorySchema, jobResponseSchema, pinnedCaseSchema, projectSummarySchema, runListItemSchema, runResultSchema, systemResponseSchema, type ComparisonRequest, type SourceArtifact, type SourceKind } from "./types";
import { strictJsonParse } from "../lib/lossless-json";

function capabilityBase(): string {
  const first = window.location.pathname.split("/").filter(Boolean)[0];
  if (!first || first === "new" || first === "runs" || first === "projects" || first === "regressions" || first === "ci" || first === "settings") {
    return "";
  }
  return `/${first}`;
}

export const appBase = capabilityBase();

async function request(path: string, init?: RequestInit): Promise<unknown> {
  const response = await fetch(`${appBase}/api/v1${path}`, {
    ...init,
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
  });
  const payload = await response.json().catch(() => ({ message: response.statusText }));
  if (!response.ok) {
    const message = typeof payload === "object" && payload && "message" in payload
      ? String(payload.message)
      : `Local API returned ${response.status}`;
    const code = typeof payload === "object" && payload && "code" in payload
      ? String(payload.code)
      : "http_error";
    throw new ApiError(code, message, response.status);
  }
  return payload;
}

export class ApiError extends Error {
  constructor(public readonly code: string, message: string, public readonly status: number) {
    super(message);
    this.name = "ApiError";
  }
}

export async function getSystem() {
  return systemResponseSchema.parse(await request("/system"));
}

export async function runDemo() {
  return runResultSchema.parse(await request("/demo", { method: "POST", body: "{}" }));
}

export async function runComparison(comparison: ComparisonRequest) {
  return runResultSchema.parse(await request("/comparisons/run", {
    method: "POST",
    body: JSON.stringify(comparison),
  }));
}

export async function createComparisonJob(comparison: ComparisonRequest) {
  return jobResponseSchema.parse(await request("/jobs", { method: "POST", body: JSON.stringify(comparison) }));
}

export async function getComparisonJob(jobId: string) {
  return jobResponseSchema.parse(await request(`/jobs/${encodeURIComponent(jobId)}`));
}

export async function cancelComparisonJob(jobId: string) {
  return jobResponseSchema.parse(await request(`/jobs/${encodeURIComponent(jobId)}/cancel`, { method: "POST", body: "{}" }));
}

export async function retryComparisonJob(jobId: string) {
  return jobResponseSchema.parse(await request(`/jobs/${encodeURIComponent(jobId)}/retry`, { method: "POST", body: "{}" }));
}

export async function stageSource(kind: SourceKind, file: File, format: SourceArtifact["format"]): Promise<Pick<SourceArtifact, "sourceId" | "hash" | "bytes" | "rows" | "preview">> {
  const parameters = new URLSearchParams({ kind, name: file.name, format });
  const response = await fetch(`${appBase}/api/v1/sources?${parameters}`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
  const staged = await response.json().catch(() => ({ message: response.statusText })) as { sourceId: string; hash: string; bytes: number; rows: number; previewJson?: string[]; message?: string; code?: string };
  if (!response.ok) throw new ApiError(staged.code ?? "source_staging_failed", staged.message ?? `Local API returned ${response.status}`, response.status);
  return { ...staged, preview: staged.previewJson?.map(strictJsonParse) };
}

export async function getFieldInventory(requestBody: {
  dataset: { sourceId: string }; baseline: { sourceId: string }; candidate: { sourceId: string };
  schema?: { sourceId: string }; datasetOutput: string; baselineOutput: string; candidateOutput: string;
  datasetId: string; baselineId: string; candidateId: string;
}) {
  return fieldInventorySchema.parse(await request("/sources/inventory", { method: "POST", body: JSON.stringify(requestBody) }));
}

export async function getRun(runId: string) {
  return runResultSchema.parse(await request(`/runs/${encodeURIComponent(runId)}`));
}

export async function acceptRun(runId: string) {
  return acceptedBaselineSchema.parse(await request(`/runs/${encodeURIComponent(runId)}/accept`, { method: "POST", body: "{}" }));
}

export async function getAcceptedBaseline(projectId: string) {
  return acceptedBaselineSchema.parse(await request(`/projects/${encodeURIComponent(projectId)}/accepted-baseline`));
}

export async function getRunCases(runId: string, offset: number, filter: string, search: string) {
  const parameters = new URLSearchParams({ offset: String(offset), limit: "200", filter, search });
  const page = casePageSchema.parse(await request(`/runs/${encodeURIComponent(runId)}/cases?${parameters}`));
  return { ...page, items: page.itemsJson.map(strictJsonParse) };
}

export async function getRuns() {
  return runListItemSchema.array().parse(await request("/runs"));
}

export async function getProjects() {
  return projectSummarySchema.array().parse(await request("/projects"));
}

export async function getProject(projectId: string) {
  const payload = await request(`/projects/${encodeURIComponent(projectId)}`) as { draft?: unknown };
  return comparisonDraftSchema.parse(hydrateDraftPreviews(payload.draft));
}

export async function duplicateProject(projectId: string) {
  const payload = await request(`/projects/${encodeURIComponent(projectId)}/duplicate`, { method: "POST", body: "{}" }) as { draft?: unknown };
  return comparisonDraftSchema.parse(hydrateDraftPreviews(payload.draft));
}

export async function archiveProject(projectId: string) {
  await request(`/projects/${encodeURIComponent(projectId)}`, { method: "DELETE" });
}

export async function getPinnedCases() {
  return pinnedCaseSchema.array().parse(await request("/regressions"));
}

export async function pinCase(runId: string, caseId: string) {
  return pinnedCaseSchema.parse(await request("/regressions/pin", {
    method: "POST",
    body: JSON.stringify({ runId, caseId }),
  }));
}

export async function deletePinnedCase(pinId: string) {
  await request(`/regressions/${encodeURIComponent(pinId)}`, { method: "DELETE" });
}

export async function updatePinnedCase(pinId: string, note: string, status: "open" | "fixed") {
  return pinnedCaseSchema.parse(await request(`/regressions/${encodeURIComponent(pinId)}`, { method: "PUT", body: JSON.stringify({ note, status }) }));
}

export async function saveDraft(draft: unknown) {
  const value = draftForStorage(draft);
  await request("/comparisons/draft", { method: "PUT", body: JSON.stringify(value) });
}

export function draftForStorage(draft: unknown): Record<string, unknown> {
  const value = structuredClone(draft) as Record<string, unknown>;
  const sources = value.sources;
  if (sources && typeof sources === "object") {
    for (const source of Object.values(sources)) {
      if (source && typeof source === "object") {
        delete (source as Record<string, unknown>).content;
        delete (source as Record<string, unknown>).preview;
        delete (source as Record<string, unknown>).previewJson;
      }
    }
    for (const [kind, source] of Object.entries(sources)) {
      if (!source || typeof source !== "object" || !(source as Record<string, unknown>).sourceId) delete (sources as Record<string, unknown>)[kind];
    }
  }
  return value;
}

export async function getDraft() {
  const payload = await request("/comparisons/draft") as { draft?: unknown };
  return payload.draft == null ? null : comparisonDraftSchema.parse(hydrateDraftPreviews(payload.draft));
}

function hydrateDraftPreviews(value: unknown): unknown {
  if (!value || typeof value !== "object") return value;
  const sources = (value as Record<string, unknown>).sources;
  if (!sources || typeof sources !== "object") return value;
  for (const source of Object.values(sources)) {
    if (!source || typeof source !== "object") continue;
    const record = source as Record<string, unknown>;
    if (Array.isArray(record.previewJson)) record.preview = record.previewJson.map((item) => strictJsonParse(String(item)));
    delete record.previewJson;
  }
  return value;
}

export async function deleteDraft() {
  await request("/comparisons/draft", { method: "DELETE" });
}

export async function generateCi(mode: "regression" | "release", projectId: string, runId: string) {
  return request("/ci/generate", { method: "POST", body: JSON.stringify({ mode, projectId, runId }) }) as Promise<{
    config: string;
    workflow: string;
    command: string;
    export_path: string;
    files: string[];
  }>;
}
