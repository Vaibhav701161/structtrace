import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { deleteDraft, getDraft, saveDraft } from "../api/client";
import type { ComparisonDraft, FieldRule, RunResult, SourceArtifact, SourceKind } from "../api/types";

const defaultDraft: ComparisonDraft = {
  projectId: crypto.randomUUID(),
  name: "Structured output comparison",
  baselineName: "Current production",
  candidateName: "Candidate change",
  sources: {},
  mapping: {
    datasetId: "/id",
    datasetInput: "/input",
    datasetExpected: "/expected",
    baselineId: "/id",
    baselineOutput: "/output",
    candidateId: "/id",
    candidateOutput: "/output",
    baselineStatus: "/status", baselineError: "/error", baselineLatency: "/latency_ms", baselineUsage: "/usage", baselineCost: "/cost", baselineMetadata: "/metadata",
    candidateStatus: "/status", candidateError: "/error", candidateLatency: "/latency_ms", candidateUsage: "/usage", candidateCost: "/cost", candidateMetadata: "/metadata",
  },
  rules: [],
  gateMode: "regression",
  minCases: 100,
  financialInvariants: false,
};
function freshDraft(): ComparisonDraft { return { ...defaultDraft, projectId: crypto.randomUUID(), sources: {}, rules: [] }; }

interface WorkspaceValue {
  draft: ComparisonDraft;
  updateDraft: (next: Partial<ComparisonDraft>) => void;
  setSource: (kind: SourceKind, source: SourceArtifact) => void;
  setRules: (rules: FieldRule[]) => void;
  result: RunResult | null;
  setResult: (result: RunResult | null) => void;
  reset: () => void;
  draftStatus: "loading" | "saved" | "saving" | "error";
  draftError: string | null;
  clearSensitiveDraft: () => Promise<void>;
  startNextIteration: (baseline: SourceArtifact) => void;
  loadProject: (draft: ComparisonDraft) => void;
}

const WorkspaceContext = createContext<WorkspaceValue | null>(null);
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [draft, setDraft] = useState<ComparisonDraft>(defaultDraft);
  const [result, setResult] = useState<RunResult | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [draftStatus, setDraftStatus] = useState<WorkspaceValue["draftStatus"]>("loading");
  const [draftError, setDraftError] = useState<string | null>(null);

  useEffect(() => {
    void getDraft()
      .then((saved) => { if (saved) setDraft(saved); })
      .catch((error: Error) => { setDraftError(error.message); setDraftStatus("error"); })
      .finally(() => { setHydrated(true); setDraftStatus((current) => current === "error" ? current : "saved"); });
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    setDraftStatus("saving");
    const timer = window.setTimeout(() => void saveDraft(draft)
      .then(() => { setDraftStatus("saved"); setDraftError(null); })
      .catch((error: Error) => { setDraftStatus("error"); setDraftError(error.message); }), 500);
    return () => window.clearTimeout(timer);
  }, [draft, hydrated]);

  const value = useMemo<WorkspaceValue>(() => ({
    draft,
    updateDraft: (next) => setDraft((current) => ({ ...current, ...next })),
    setSource: (kind, source) => setDraft((current) => ({
      ...current,
      sources: { ...current.sources, [kind]: source },
    })),
    setRules: (rules) => setDraft((current) => ({ ...current, rules })),
    result,
    setResult,
    reset: () => {
      const next = freshDraft();
      setDraft(next);
      setResult(null);
      void saveDraft(next).catch((error: Error) => { setDraftStatus("error"); setDraftError(error.message); });
    },
    draftStatus,
    draftError,
    clearSensitiveDraft: async () => {
      await deleteDraft();
      setDraft(freshDraft());
      setResult(null);
      setDraftStatus("saved");
      setDraftError(null);
    },
    startNextIteration: (baseline) => {
      setDraft((current) => {
        const next = { ...current, baselineName: current.candidateName, candidateName: "Candidate change", sources: { ...current.sources, baseline, candidate: undefined }, mapping: { ...current.mapping, baselineId: "/id", baselineOutput: "/output", baselineStatus: "/status", baselineError: "/error", baselineLatency: "/latency_ms", baselineUsage: "/usage", baselineCost: "/cost", baselineMetadata: "/metadata" } };
        setDraftStatus("saving");
        void saveDraft(next).then(() => { setDraftStatus("saved"); setDraftError(null); }).catch((error: Error) => { setDraftStatus("error"); setDraftError(error.message); });
        return next;
      });
      setResult(null);
    },
    loadProject: (next) => { setDraft(next); setResult(null); setDraftStatus("saved"); setDraftError(null); },
  }), [draft, result, draftStatus, draftError]);

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace() {
  const value = useContext(WorkspaceContext);
  if (!value) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return value;
}
