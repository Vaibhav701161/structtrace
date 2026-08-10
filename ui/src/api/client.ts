import { casePageSchema, comparisonDraftSchema, pinnedCaseSchema, runResultSchema, systemResponseSchema, type ComparisonRequest } from "./types";

function capabilityBase(): string {
  const first = window.location.pathname.split("/").filter(Boolean)[0];
  if (!first || first === "new" || first === "runs" || first === "regressions" || first === "ci" || first === "settings") {
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
    throw new Error(message);
  }
  return payload;
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

export async function getRun(runId: string) {
  return runResultSchema.parse(await request(`/runs/${encodeURIComponent(runId)}`));
}

export async function getRunCases(runId: string, offset: number, filter: string, search: string) {
  const parameters = new URLSearchParams({ offset: String(offset), limit: "200", filter, search });
  return casePageSchema.parse(await request(`/runs/${encodeURIComponent(runId)}/cases?${parameters}`));
}

export async function getRuns() {
  return runResultSchema.array().parse(await request("/runs"));
}

export async function acceptRun(runId: string) {
  return request(`/runs/${encodeURIComponent(runId)}/accept`, { method: "POST", body: "{}" });
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

export async function saveDraft(draft: unknown) {
  await request("/comparisons/draft", { method: "PUT", body: JSON.stringify(draft) });
}

export async function getDraft() {
  const payload = await request("/comparisons/draft") as { draft?: unknown };
  return payload.draft == null ? null : comparisonDraftSchema.parse(payload.draft);
}

export async function generateCi(mode: "regression" | "release") {
  return request("/ci/generate", { method: "POST", body: JSON.stringify({ mode }) }) as Promise<{
    config: string;
    workflow: string;
    command: string;
  }>;
}
