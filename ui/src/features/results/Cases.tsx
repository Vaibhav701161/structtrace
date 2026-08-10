import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, ArrowRight, Braces, Check, ChevronLeft, ChevronRight, Copy, Pin, Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { getRun, getRunCases, pinCase } from "../../api/client";
import type { RunResult } from "../../api/types";
import { Button, Card, InlineNotice, PageHeader, Status } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";

type CaseRecord = Record<string, any>;
const filters = ["All cases", "Regressions", "Improvements", "Both wrong", "Valid but wrong", "Parse failures", "Schema failures", "Evaluator errors", "Saved"];

export function Cases() {
  const { runId, caseId } = useParams({ strict: false });
  const routeSearch = useSearch({ strict: false }) as { search?: string };
  const { result: cached } = useWorkspace();
  const query = useQuery({ queryKey: ["run", runId], queryFn: () => getRun(runId ?? ""), enabled: !cached || cached.runId !== runId });
  const result = cached?.runId === runId ? cached : query.data;
  if (!result) return <div className="page">{query.error ? <InlineNotice tone="danger" title="Cases unavailable">{query.error.message}</InlineNotice> : <p>Loading case evidence…</p>}</div>;
  return <CaseExplorer result={result} selectedId={caseId} initialSearch={routeSearch.search ?? ""} />;
}

function CaseExplorer({ result, selectedId, initialSearch }: { result: RunResult; selectedId?: string; initialSearch: string }) {
  const navigate = useNavigate();
  const [active, setActive] = useState(selectedId ? "All cases" : "Regressions");
  const [search, setSearch] = useState(selectedId ?? initialSearch);
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
        <div ref={parent} className="case-scroll"><div style={{ height: virtual.getTotalSize(), position: "relative" }}>{virtualItems.map((row) => { const record = records[row.index]; const transition = transitionLabel(record.transition); return <button className="case-row" key={String(record.case?.id)} style={{ transform: `translateY(${row.start}px)` }} onClick={() => open(record)}><code>{String(record.case?.id)}</code><Status tone={transition.tone} label={transition.label} /><code>{firstFailure(record)}</code><OutcomeCell evaluation={record.baseline_evaluation} /><OutcomeCell evaluation={record.candidate_evaluation} /><Status tone={record.candidate_evaluation?.schema_valid ? "pass" : "fail"} label={record.candidate_evaluation?.schema_valid ? "Valid" : "Invalid"} /></button>; })}</div></div>
      </Card>
      {pages.isLoading && <p className="muted">Loading bounded case evidence…</p>}
      {pages.error && <InlineNotice tone="danger" title="Case evidence could not be loaded">{pages.error.message}</InlineNotice>}
      {selected && <CaseDrawer result={result} record={selected} close={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search } })} previous={selectedIndex > 0 ? () => open(records[selectedIndex - 1]) : undefined} next={selectedIndex >= 0 && selectedIndex + 1 < records.length ? () => open(records[selectedIndex + 1]) : undefined} />}
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
    "Saved": "pinned",
  };
  return values[label] ?? "all";
}

function OutcomeCell({ evaluation }: { evaluation: CaseRecord | undefined }) {
  const results = Object.values(evaluation?.evaluators ?? {}) as CaseRecord[];
  const statuses = results.map((item) => item.status);
  if (evaluation?.adapter_status === "error" || statuses.includes("error")) return <span className="outcome-unknown">Error</span>;
  if (statuses.includes("not_applicable")) return <span className="outcome-unknown">N/A</span>;
  if (statuses.includes("unscored") || evaluation?.primary_pass == null) return <span className="outcome-unknown">Unscored</span>;
  const pass = evaluation.primary_pass === true;
  return <span className={pass ? "outcome-pass" : "outcome-fail"}>{pass ? <Check size={14} /> : <X size={14} />}{pass ? "Correct" : "Wrong"}</span>;
}
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
  const [failedOnly, setFailedOnly] = useState(false);
  const drawer = useRef<HTMLElement>(null);
  const queryClient = useQueryClient();
  const pin = useMutation({
    mutationFn: () => pinCase(result.runId, String(record.case?.id)),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["regressions"] }),
  });
  const transition = transitionLabel(record.transition);
  const output = (side: "baseline" | "candidate") => record[`${side}_output`]?.parsed_output ?? record[`${side}_output`]?.raw_output ?? null;
  const failingPointers = useMemo(() => {
    const evaluators = Object.values(record.candidate_evaluation?.evaluators ?? {}) as CaseRecord[];
    return new Set(evaluators.flatMap((item) => (item.fields ?? []) as CaseRecord[]).filter((field) => field.status !== "passed").map((field) => String(field.pointer ?? itemPointer(field))).filter(Boolean));
  }, [record]);
  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    drawer.current?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
      if (event.key === "ArrowLeft" && previous) previous();
      if (event.key === "ArrowRight" && next) next();
      if (event.key === "Tab" && drawer.current) {
        const focusable = [...drawer.current.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])')];
        if (!focusable.length) return;
        const first = focusable[0]; const last = focusable[focusable.length - 1];
        if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
        if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => { window.removeEventListener("keydown", keyboard); previousFocus?.focus(); };
  }, [close, previous, next]);
  return (
    <div className="drawer-backdrop" role="presentation" onMouseDown={close}>
      <aside ref={drawer} tabIndex={-1} className="case-drawer" role="dialog" aria-modal="true" aria-label={`Case ${record.case?.id}`} onMouseDown={(event) => event.stopPropagation()}>
        <header><div><small>CASE</small><h2>{String(record.case?.id)}</h2><Status tone={transition.tone} label={transition.label} /></div><div><Button variant="secondary" icon={Pin} onClick={() => pin.mutate()} disabled={pin.isPending}>{pin.isSuccess ? "Saved" : pin.isPending ? "Saving…" : "Save case"}</Button><button className="icon-button" onClick={close} aria-label="Close case"><X /></button></div></header>
        <nav className="drawer-tabs" aria-label="Case detail tabs">{["Comparison", "Rules", "Raw evidence", "Metadata"].map((item) => <button className={tab === item ? "active" : ""} onClick={() => setTab(item)} key={item}>{item}</button>)}</nav>
        <div className="drawer-content">
          {tab === "Comparison" && <><div className="diff-toolbar"><span>{failingPointers.size} failing {failingPointers.size === 1 ? "field" : "fields"}</span><label><input type="checkbox" checked={failedOnly} onChange={(event) => setFailedOnly(event.target.checked)} /> Focus failed fields</label></div><div className="comparison-columns"><JsonPanel title="Expected" value={record.case?.expected} highlights={failingPointers} failedOnly={failedOnly} /><JsonPanel title="Baseline" value={output("baseline")} state={record.baseline_evaluation?.primary_pass} highlights={failingPointers} failedOnly={failedOnly} /><JsonPanel title="Candidate" value={output("candidate")} state={record.candidate_evaluation?.primary_pass} highlights={failingPointers} failedOnly={failedOnly} /></div></>}
          {tab === "Rules" && <RuleTable record={record} />}
          {tab === "Raw evidence" && <div className="raw-stack"><JsonPanel title="Baseline output envelope" value={record.baseline_output} /><JsonPanel title="Candidate output envelope" value={record.candidate_output} /></div>}
          {tab === "Metadata" && <JsonPanel title="Evaluation-only metadata" value={record.case?.metadata ?? { message: "No metadata retained" }} />}
        </div>
        <footer><button onClick={previous} disabled={!previous}><ChevronLeft size={16} /> Previous case</button><span>Immutable evidence · {result.runId}</span><button onClick={next} disabled={!next}>Next case <ChevronRight size={16} /></button></footer>
      </aside>
    </div>
  );
}

function JsonPanel({ title, value, state, highlights = new Set<string>(), failedOnly = false }: { title: string; value: unknown; state?: boolean; highlights?: Set<string>; failedOnly?: boolean }) {
  return <section className="json-panel"><header><span><Braces size={15} />{title}</span>{state !== undefined && <Status tone={state ? "pass" : "fail"} label={state ? "Correct" : "Incorrect"} />}</header><div className="json-tree">{jsonRows(value).filter((row) => !failedOnly || rowRelevant(row.path, highlights)).map((row) => <div key={row.path || "/"} className={[...highlights].some((pointer) => pointerMatches(pointer, row.path)) ? "json-row json-row-failed" : "json-row"} style={{ paddingLeft: `${10 + row.depth * 14}px` }}><button title={`Copy JSON Pointer ${row.path || "/"}`} onClick={() => void navigator.clipboard.writeText(row.path || "/")}><Copy size={12} /><code>{row.path || "/"}</code></button><span>{row.value}</span></div>)}</div></section>;
}

function itemPointer(field: CaseRecord) { return String(field.expected_pointer ?? ""); }
function pointerMatches(pattern: string, path: string) { const expected = pattern.split("/"); const actual = path.split("/"); return expected.length === actual.length && expected.every((segment, index) => segment === "*" || segment === actual[index]); }
function rowRelevant(path: string, highlights: Set<string>) { return [...highlights].some((pointer) => pointerMatches(pointer, path) || pointer.startsWith(`${path}/`) || path.startsWith(`${pointer}/`) || pointer.split("/").slice(0, path.split("/").length).every((segment, index) => segment === "*" || segment === path.split("/")[index])); }
function jsonRows(value: unknown, path = "", depth = 0): Array<{ path: string; depth: number; value: string }> {
  if (value !== null && typeof value === "object") {
    const entries = Array.isArray(value) ? value.map((item, index) => [String(index), item] as const) : Object.entries(value as Record<string, unknown>);
    if (!entries.length) return [{ path, depth, value: Array.isArray(value) ? "[]" : "{}" }];
    return entries.flatMap(([key, child]) => jsonRows(child, `${path}/${key.replace(/~/g, "~0").replace(/\//g, "~1")}`, depth + 1));
  }
  return [{ path, depth, value: typeof value === "string" ? JSON.stringify(value) : String(value) }];
}
function RuleTable({ record }: { record: CaseRecord }) {
  const baseline = Object.values(record.baseline_evaluation?.evaluators ?? {}) as CaseRecord[];
  const candidate = Object.values(record.candidate_evaluation?.evaluators ?? {}) as CaseRecord[];
  const ids = [...new Set([...baseline, ...candidate].map((item: CaseRecord) => item.evaluator_id))];
  return <table className="drawer-rule-table"><thead><tr><th>Rule</th><th>Baseline</th><th>Candidate</th><th>Reason</th></tr></thead><tbody>{ids.map((id) => { const left = baseline.find((item: CaseRecord) => (item.evaluator_id ?? item.id) === id); const right = candidate.find((item: CaseRecord) => (item.evaluator_id ?? item.id) === id); return <tr key={String(id)}><td><code>{String(id)}</code></td><td><Status tone={left?.passed ? "pass" : "fail"} label={left?.passed ? "Pass" : "Fail"} /></td><td><Status tone={right?.passed ? "pass" : "fail"} label={right?.passed ? "Pass" : "Fail"} /></td><td>{right?.message ?? right?.reason ?? "Deterministic evaluator result"}</td></tr>; })}</tbody></table>;
}
