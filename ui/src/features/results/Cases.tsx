import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, ArrowRight, Braces, Check, ChevronLeft, ChevronRight, Pin, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { getRun, getRunCases, pinCase } from "../../api/client";
import type { RunResult } from "../../api/types";
import { Button, Card, InlineNotice, PageHeader, Status } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";

type CaseRecord = Record<string, any>;
const filters = ["All cases", "Regressions", "Improvements", "Both wrong", "Valid but wrong", "Parse failures", "Schema failures", "Evaluator errors", "Pinned"];

export function Cases() {
  const { runId, caseId } = useParams({ strict: false });
  const { result: cached } = useWorkspace();
  const query = useQuery({ queryKey: ["run", runId], queryFn: () => getRun(runId ?? ""), enabled: !cached || cached.runId !== runId });
  const result = cached?.runId === runId ? cached : query.data;
  if (!result) return <div className="page">{query.error ? <InlineNotice tone="danger" title="Cases unavailable">{query.error.message}</InlineNotice> : <p>Loading case evidence…</p>}</div>;
  return <CaseExplorer result={result} selectedId={caseId} />;
}

function CaseExplorer({ result, selectedId }: { result: RunResult; selectedId?: string }) {
  const navigate = useNavigate();
  const [active, setActive] = useState(selectedId ? "All cases" : "Regressions");
  const [search, setSearch] = useState(selectedId ?? "");
  const parent = useRef<HTMLDivElement>(null);
  const serverFilter = filterValue(active);
  const pages = useInfiniteQuery({
    queryKey: ["run-cases", result.runId, serverFilter, search],
    initialPageParam: 0,
    queryFn: ({ pageParam }) => getRunCases(result.runId, pageParam, serverFilter, search),
    getNextPageParam: (page) => page.offset + page.items.length < page.total
      ? page.offset + page.items.length
      : undefined,
  });
  const records = useMemo(
    () => (pages.data?.pages.flatMap((page) => page.items) ?? []) as CaseRecord[],
    [pages.data],
  );
  const total = pages.data?.pages[0]?.total ?? 0;
  const virtual = useVirtualizer({ count: records.length, getScrollElement: () => parent.current, estimateSize: () => 48, overscan: 12 });
  const virtualItems = virtual.getVirtualItems();
  const lastIndex = virtualItems.at(-1)?.index ?? 0;
  useEffect(() => {
    if (lastIndex >= records.length - 15 && pages.hasNextPage && !pages.isFetchingNextPage) {
      void pages.fetchNextPage();
    }
  }, [lastIndex, pages.hasNextPage, pages.isFetchingNextPage, pages.fetchNextPage, records.length]);
  const selected = records.find((record) => String(record.case?.id) === selectedId);
  const selectedIndex = records.findIndex((record) => String(record.case?.id) === selectedId);
  const open = (record: CaseRecord) => void navigate({ to: "/runs/$runId/cases/$caseId", params: { runId: result.runId, caseId: String(record.case?.id) } });
  return (
    <div className="page page-full cases-page">
      <PageHeader eyebrow={`${result.projectName} · ${result.runId}`} title="Case evidence" description="Inspect each matched pair. Filters never change the stored denominator." actions={<Button variant="secondary" icon={ArrowLeft} onClick={() => void navigate({ to: "/runs/$runId", params: { runId: result.runId } })}>Overview</Button>} />
      <div className="case-toolbar"><div className="search-field"><Search size={16} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search case ID or evidence" aria-label="Search cases" /></div><span>{records.length < total ? `${records.length} loaded · ${total} matching` : `${total} cases`}</span></div>
      <div className="filter-pills" role="group" aria-label="Case filters">{filters.map((filter) => <button key={filter} className={active === filter ? "active" : ""} onClick={() => setActive(filter)}>{filter}</button>)}</div>
      <Card className="case-table-card">
        <div className="case-table-head"><span>Case ID</span><span>Transition</span><span>Top failing field</span><span>Baseline</span><span>Candidate</span><span>Structure</span></div>
        <div ref={parent} className="case-scroll"><div style={{ height: virtual.getTotalSize(), position: "relative" }}>{virtualItems.map((row) => { const record = records[row.index]; const transition = transitionLabel(record.transition); return <button className="case-row" key={String(record.case?.id)} style={{ transform: `translateY(${row.start}px)` }} onClick={() => open(record)}><code>{String(record.case?.id)}</code><Status tone={transition.tone} label={transition.label} /><code>{firstFailure(record)}</code><OutcomeCell pass={Boolean(record.baseline_evaluation?.primary_pass)} /><OutcomeCell pass={Boolean(record.candidate_evaluation?.primary_pass)} /><Status tone={record.candidate_evaluation?.schema_valid ? "pass" : "fail"} label={record.candidate_evaluation?.schema_valid ? "Valid" : "Invalid"} /></button>; })}</div></div>
      </Card>
      {pages.isLoading && <p className="muted">Loading bounded case evidence…</p>}
      {pages.error && <InlineNotice tone="danger" title="Case evidence could not be loaded">{pages.error.message}</InlineNotice>}
      {selected && <CaseDrawer result={result} record={selected} close={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId } })} previous={selectedIndex > 0 ? () => open(records[selectedIndex - 1]) : undefined} next={selectedIndex >= 0 && selectedIndex + 1 < records.length ? () => open(records[selectedIndex + 1]) : undefined} />}
    </div>
  );
}

function filterValue(label: string) {
  const values: Record<string, string> = {
    "Regressions": "regressions",
    "All cases": "all",
    "Improvements": "improvements",
    "Both wrong": "both_wrong",
    "Valid but wrong": "valid_but_wrong",
    "Parse failures": "parse_failures",
    "Schema failures": "schema_failures",
    "Evaluator errors": "evaluator_errors",
    "Pinned": "pinned",
  };
  return values[label] ?? "all";
}

function OutcomeCell({ pass }: { pass: boolean }) { return <span className={pass ? "outcome-pass" : "outcome-fail"}>{pass ? <Check size={14} /> : <X size={14} />}{pass ? "Correct" : "Wrong"}</span>; }
function transitionLabel(value: unknown): { label: string; tone: "pass" | "fail" | "neutral" | "info" } {
  if (value === "baseline_only_pass") return { label: "Regression", tone: "fail" };
  if (value === "candidate_only_pass") return { label: "Improvement", tone: "pass" };
  if (value === "both_pass") return { label: "Both pass", tone: "info" };
  return { label: "Both fail", tone: "neutral" };
}
function firstFailure(record: CaseRecord) {
  const results = Object.values(record.candidate_evaluation?.evaluators ?? {}) as CaseRecord[];
  const failed = results.find((item: CaseRecord) => item.status === "failed" || item.passed === false);
  return failed?.fields?.[0]?.pointer ?? failed?.details?.pointer ?? "—";
}

function CaseDrawer({ result, record, close, previous, next }: { result: RunResult; record: CaseRecord; close: () => void; previous?: () => void; next?: () => void }) {
  const [tab, setTab] = useState("Comparison");
  const queryClient = useQueryClient();
  const pin = useMutation({
    mutationFn: () => pinCase(result.runId, String(record.case?.id)),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["regressions"] }),
  });
  const transition = transitionLabel(record.transition);
  const output = (side: "baseline" | "candidate") => record[`${side}_output`]?.parsed_output ?? record[`${side}_output`]?.raw_output ?? null;
  return (
    <div className="drawer-backdrop" role="presentation" onMouseDown={close}>
      <aside className="case-drawer" role="dialog" aria-modal="true" aria-label={`Case ${record.case?.id}`} onMouseDown={(event) => event.stopPropagation()}>
        <header><div><small>CASE</small><h2>{String(record.case?.id)}</h2><Status tone={transition.tone} label={transition.label} /></div><div><Button variant="secondary" icon={Pin} onClick={() => pin.mutate()} disabled={pin.isPending}>{pin.isSuccess ? "Pinned" : pin.isPending ? "Pinning…" : "Pin"}</Button><button className="icon-button" onClick={close} aria-label="Close case"><X /></button></div></header>
        <nav className="drawer-tabs" aria-label="Case detail tabs">{["Comparison", "Rules", "Raw evidence", "Metadata"].map((item) => <button className={tab === item ? "active" : ""} onClick={() => setTab(item)} key={item}>{item}</button>)}</nav>
        <div className="drawer-content">
          {tab === "Comparison" && <div className="comparison-columns"><JsonPanel title="Expected" value={record.case?.expected} /><JsonPanel title="Baseline" value={output("baseline")} state={record.baseline_evaluation?.primary_pass} /><JsonPanel title="Candidate" value={output("candidate")} state={record.candidate_evaluation?.primary_pass} /></div>}
          {tab === "Rules" && <RuleTable record={record} />}
          {tab === "Raw evidence" && <div className="raw-stack"><JsonPanel title="Baseline output envelope" value={record.baseline_output} /><JsonPanel title="Candidate output envelope" value={record.candidate_output} /></div>}
          {tab === "Metadata" && <JsonPanel title="Evaluation-only metadata" value={record.case?.metadata ?? { message: "No metadata retained" }} />}
        </div>
        <footer><button onClick={previous} disabled={!previous}><ChevronLeft size={16} /> Previous case</button><span>Immutable evidence · {result.runId}</span><button onClick={next} disabled={!next}>Next case <ChevronRight size={16} /></button></footer>
      </aside>
    </div>
  );
}

function JsonPanel({ title, value, state }: { title: string; value: unknown; state?: boolean }) {
  return <section className="json-panel"><header><span><Braces size={15} />{title}</span>{state !== undefined && <Status tone={state ? "pass" : "fail"} label={state ? "Correct" : "Incorrect"} />}</header><pre>{typeof value === "string" ? value : JSON.stringify(value, null, 2)}</pre></section>;
}
function RuleTable({ record }: { record: CaseRecord }) {
  const baseline = Object.values(record.baseline_evaluation?.evaluators ?? {}) as CaseRecord[];
  const candidate = Object.values(record.candidate_evaluation?.evaluators ?? {}) as CaseRecord[];
  const ids = [...new Set([...baseline, ...candidate].map((item: CaseRecord) => item.evaluator_id))];
  return <table className="drawer-rule-table"><thead><tr><th>Rule</th><th>Baseline</th><th>Candidate</th><th>Reason</th></tr></thead><tbody>{ids.map((id) => { const left = baseline.find((item: CaseRecord) => (item.evaluator_id ?? item.id) === id); const right = candidate.find((item: CaseRecord) => (item.evaluator_id ?? item.id) === id); return <tr key={String(id)}><td><code>{String(id)}</code></td><td><Status tone={left?.passed ? "pass" : "fail"} label={left?.passed ? "Pass" : "Fail"} /></td><td><Status tone={right?.passed ? "pass" : "fail"} label={right?.passed ? "Pass" : "Fail"} /></td><td>{right?.message ?? right?.reason ?? "Deterministic evaluator result"}</td></tr>; })}</tbody></table>;
}
