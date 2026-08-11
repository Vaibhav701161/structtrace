import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
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
function freshDraft(): ComparisonDraft { return { ...defaultDraft, projectId: crypto.randomUUID(), sources: {}, rules: [], activeJobId: undefined }; }

interface WorkspaceValue {
  draft: ComparisonDraft;
  updateDraft: (next: Partial<ComparisonDraft>) => void;
  updateDraftAndPersist: (next: Partial<ComparisonDraft>) => Promise<void>;
  setSource: (kind: SourceKind, source: SourceArtifact) => void;
  setRules: (rules: FieldRule[]) => void;
  result: RunResult | null;
  setResult: (result: RunResult | null) => void;
  reset: () => void;
  draftStatus: "loading" | "saved" | "saving" | "error";
  draftError: string | null;
  clearSensitiveDraft: () => Promise<void>;
  startNextIteration: (baseline: SourceArtifact) => Promise<void>;
  loadProject: (draft: ComparisonDraft) => void;
}

const WorkspaceContext = createContext<WorkspaceValue | null>(null);
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [draft, setDraft] = useState<ComparisonDraft>(defaultDraft);
  const [result, setResult] = useState<RunResult | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [draftStatus, setDraftStatus] = useState<WorkspaceValue["draftStatus"]>("loading");
  const [draftError, setDraftError] = useState<string | null>(null);
  const draftRef = useRef(draft);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());
  const saveSequence = useRef(0);

  const persistDraft = useCallback(async (next: ComparisonDraft) => {
    const sequence = ++saveSequence.current;
    setDraftStatus("saving");
    const operation = saveQueue.current.catch(() => undefined).then(() => saveDraft(next));
    saveQueue.current = operation;
    try {
      await operation;
      if (sequence === saveSequence.current) {
        setDraftStatus("saved");
        setDraftError(null);
      }
    } catch (error) {
      if (sequence === saveSequence.current) {
        setDraftStatus("error");
        setDraftError(error instanceof Error ? error.message : "Draft could not be saved.");
      }
      throw error;
    }
  }, []);

  useEffect(() => { draftRef.current = draft; }, [draft]);

  const updateDraftAndPersist = useCallback(async (patch: Partial<ComparisonDraft>) => {
    const next = { ...draftRef.current, ...patch };
    await persistDraft(next);
    draftRef.current = next;
    setDraft(next);
  }, [persistDraft]);

  useEffect(() => {
    void getDraft()
      .then((saved) => { if (saved) setDraft(saved); })
      .catch((error: Error) => { setDraftError(error.message); setDraftStatus("error"); })
      .finally(() => { setHydrated(true); setDraftStatus((current) => current === "error" ? current : "saved"); });
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    const timer = window.setTimeout(() => void persistDraft(draft).catch(() => undefined), 500);
    return () => window.clearTimeout(timer);
  }, [draft, hydrated, persistDraft]);

  const value = useMemo<WorkspaceValue>(() => ({
    draft,
    updateDraft: (next) => setDraft((current) => ({ ...current, ...next })),
    updateDraftAndPersist,
    setSource: (kind, source) => setDraft((current) => ({
      ...current,
      sources: { ...current.sources, [kind]: source },
      rules: [],
      activeJobId: undefined,
    })),
    setRules: (rules) => setDraft((current) => ({ ...current, rules })),
    result,
    setResult,
    reset: () => {
      const next = freshDraft();
      setDraft(next);
      setResult(null);
      void persistDraft(next).catch((error: Error) => { setDraftStatus("error"); setDraftError(error.message); });
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
    startNextIteration: async (baseline) => {
      const next = { ...draft, activeJobId: undefined, baselineName: draft.candidateName, candidateName: "Candidate change", sources: { ...draft.sources, baseline, candidate: undefined }, mapping: { ...draft.mapping, baselineId: "/id", baselineOutput: "/output", baselineStatus: "/status", baselineError: "/error", baselineLatency: "/latency_ms", baselineUsage: "/usage", baselineCost: "/cost", baselineMetadata: "/metadata" } };
      setDraft(next);
      setResult(null);
      await persistDraft(next);
    },
    loadProject: (next) => { setDraft(next); setResult(null); setDraftStatus("saved"); setDraftError(null); },
  }), [draft, result, draftStatus, draftError, persistDraft, updateDraftAndPersist]);

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace() {
  const value = useContext(WorkspaceContext);
  if (!value) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return value;
}
