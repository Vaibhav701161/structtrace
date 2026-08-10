import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { getDraft, saveDraft } from "../api/client";
import type { ComparisonDraft, FieldRule, RunResult, SourceArtifact, SourceKind } from "../api/types";

const defaultDraft: ComparisonDraft = {
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
  },
  rules: [],
  gateMode: "regression",
  minCases: 100,
  financialInvariants: false,
};

interface WorkspaceValue {
  draft: ComparisonDraft;
  updateDraft: (next: Partial<ComparisonDraft>) => void;
  setSource: (kind: SourceKind, source: SourceArtifact) => void;
  setRules: (rules: FieldRule[]) => void;
  result: RunResult | null;
  setResult: (result: RunResult | null) => void;
  reset: () => void;
}

const WorkspaceContext = createContext<WorkspaceValue | null>(null);
export function WorkspaceProvider({ children }: { children: ReactNode }) {
  const [draft, setDraft] = useState<ComparisonDraft>(defaultDraft);
  const [result, setResult] = useState<RunResult | null>(null);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    void getDraft()
      .then((saved) => { if (saved) setDraft(saved); })
      .catch(() => undefined)
      .finally(() => setHydrated(true));
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    const timer = window.setTimeout(() => void saveDraft(draft).catch(() => undefined), 350);
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
      setDraft(defaultDraft);
      setResult(null);
      void saveDraft(defaultDraft).catch(() => undefined);
    },
  }), [draft, result]);

  return <WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>;
}

export function useWorkspace() {
  const value = useContext(WorkspaceContext);
  if (!value) throw new Error("useWorkspace must be used inside WorkspaceProvider");
  return value;
}
