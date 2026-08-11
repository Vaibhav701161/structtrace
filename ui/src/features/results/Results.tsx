import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "@tanstack/react-router";
import {
  ArrowDownRight,
  ArrowRight,
  ArrowUpRight,
  CheckCircle2,
  CircleAlert,
  Download,
  FileCheck2,
  GitPullRequest,
  ListFilter,
  Pin,
  ShieldAlert,
  ShieldCheck,
  TriangleAlert,
  XCircle,
} from "lucide-react";
import { acceptRun, getRun } from "../../api/client";
import type { RunResult } from "../../api/types";
import { Button, Card, InlineNotice, PageHeader, Skeleton, Status } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";

export function Results() {
  const { runId } = useParams({ strict: false });
  const { result: cached } = useWorkspace();
  const query = useQuery({ queryKey: ["run", runId], queryFn: () => getRun(runId ?? ""), enabled: !cached || cached.runId !== runId });
  const result = cached?.runId === runId ? cached : query.data;
  if (!result) return <ResultsLoading error={query.error} />;
  return <ResultContent result={result} />;
}

function ResultsLoading({ error }: { error: Error | null }) {
  if (error) return <div className="page"><InlineNotice tone="danger" title="Comparison unavailable">{error.message}</InlineNotice></div>;
  return <div className="page"><Skeleton width="36%" /><Card><Skeleton /><Skeleton width="72%" /><Skeleton width="52%" /></Card></div>;
}

export function decision(result: RunResult) {
  if (result.integrity.status !== "verified") return {
    title: result.integrity.status === "modified" ? "EVIDENCE MODIFIED" : result.integrity.status === "replay_failed" ? "REPLAY FAILED" : "EVIDENCE NOT VERIFIED",
    text: result.integrity.detail,
    tone: "fail" as const,
    icon: ShieldAlert,
  };
  if (result.regressionSuite.blocking) return {
    title: "PINNED REGRESSION SUITE FAILED",
    text: `${result.regressionSuite.stillBroken} still broken, ${result.regressionSuite.reintroduced} reintroduced, and ${result.regressionSuite.missing} missing pinned cases block release authority.`,
    tone: "fail" as const,
    icon: ShieldAlert,
  };
  const gate = result.summary.gate;
  if (gate.gate_mode === "release" && gate.deployment_authorized) return { title: "RELEASE AUTHORIZED", text: "Candidate passed every configured release and evidence rule.", tone: "pass" as const, icon: ShieldCheck };
  if (gate.status === "error") return { title: "RUN ERROR", text: gate.runtime_errors[0] ?? "A required rule could not be evaluated safely.", tone: "fail" as const, icon: CircleAlert };
  if (gate.quality_failures.length) return { title: "DO NOT DEPLOY", text: gate.evidence_failures.length ? `Quality thresholds failed: ${gate.quality_failures[0]} Evidence requirements are also insufficient: ${gate.evidence_failures[0]}` : gate.quality_failures[0], tone: "fail" as const, icon: ShieldAlert };
  if (gate.status === "insufficient_evidence") return { title: "NOT ENOUGH EVIDENCE", text: gate.evidence_failures[0] ?? "The configured evidence requirement was not met.", tone: "warning" as const, icon: TriangleAlert };
  if (gate.status === "failed") return { title: "DO NOT DEPLOY", text: "Candidate failed a configured quality threshold.", tone: "fail" as const, icon: ShieldAlert };
  if (gate.gate_mode === "regression" && gate.status === "passed") return { title: "REGRESSION CHECK PASSED", text: "No configured regression threshold failed. This is not release authorization.", tone: "info" as const, icon: CheckCircle2 };
  return { title: "ANALYSIS COMPLETE", text: "No deployment authorization was configured for this comparison.", tone: "neutral" as const, icon: FileCheck2 };
}

function ResultContent({ result }: { result: RunResult }) {
  const navigate = useNavigate();
  const { startNextIteration } = useWorkspace();
  const accepted = useMutation({ mutationFn: () => acceptRun(result.runId) });
  const state = decision(result);
  const summary = result.summary;
  if (result.integrity.status !== "verified") {
    return <div className="page page-wide result-page">
      <PageHeader eyebrow={`${result.projectName} · run ${result.runId}`} title="Evidence inspection only" actions={<Button variant="secondary" icon={Download} onClick={() => exportSummary(result)}>Download raw inspection record</Button>} />
      <section className="decision-banner decision-fail"><ShieldAlert size={28} aria-hidden="true" /><div><span>AUTHORITY DISABLED</span><h2>{state.title}</h2><p>No decision metric is shown as authoritative while this run is unverified.</p></div><Status tone="fail" label="Raw evidence only" /></section>
      <InlineNotice tone="danger" title="Last trustworthy state unavailable">{result.integrity.detail} Open the original manifest-bound artifacts or restore them from the last immutable receipt. Release export, case claims, and baseline promotion remain disabled.</InlineNotice>
      <Card><h2>Why the dashboard is withheld</h2><p>Displaying percentages from modified or replay-failed artifacts would make untrusted evidence look conclusive. The downloadable inspection record retains the raw summary together with its integrity status for forensic use.</p></Card>
    </div>;
  }
  const independentTotal = summary.baseline.total;
  const capturedTotal = summary.descriptive_baseline.total;
  const semanticTotal = summary.jointly_scored_semantic.jointly_scored_cases;
  const metricRows = [
    ["Deployment success", summary.baseline.deployment_success, summary.candidate.deployment_success],
    ["Strict JSON", summary.baseline.parse_valid, summary.candidate.parse_valid],
    [result.schemaProvenance === "inferred_from_expected_values" ? "Schema valid (inferred shape)" : "Schema valid", summary.baseline.schema_valid, summary.candidate.schema_valid],
    ["Semantically correct", summary.baseline.semantic_success, summary.candidate.semantic_success],
    ["Valid but wrong", summary.baseline.valid_but_wrong, summary.candidate.valid_but_wrong],
  ] as const;
  const regressions = summary.paired.baseline_only_pass;
  const improvements = summary.paired.candidate_only_pass;
  const regressionHotspots = aggregateHotspots(summary.primary_field_hotspots);
  const description = regressions > improvements
    ? `Candidate created ${regressions} regressions and ${improvements} improvements. Structural success does not override those semantic losses.`
    : `Candidate created ${improvements} improvements and ${regressions} regressions. Review every discordant case before changing production behavior.`;
  return (
    <div className="page page-wide result-page">
      <PageHeader eyebrow={`${result.projectName} · run ${result.runId}`} title="Should I ship?" actions={<><Button variant="secondary" icon={Download} onClick={() => exportSummary(result)}>Export summary</Button><Button icon={GitPullRequest} disabled={result.integrity.status !== "verified" || !result.projectId} onClick={() => void navigate({ to: "/ci", search: { project: result.projectId ?? "", run: result.runId } })}>Export CI project</Button></>} />
      <section className={`decision-banner decision-${state.tone}`}>
        <state.icon size={28} aria-hidden="true" />
        <div><span>RELEASE DECISION</span><h2>{state.title}</h2><p>{state.text}</p></div>
        <Status tone={state.tone === "pass" ? "pass" : state.tone === "fail" ? "fail" : state.tone === "warning" ? "warning" : "info"} label={summary.gate.gate_mode === "release" ? "Release gate" : summary.gate.gate_mode === "regression" ? "Regression gate" : "Advisory"} />
      </section>
      <InlineNotice tone={result.integrity.status === "verified" ? "success" : "danger"} title={result.integrity.status === "verified" ? "Evidence integrity verified" : "Authority disabled"}>{result.integrity.detail}{result.integrity.status !== "verified" && " Case claims, release export, and baseline promotion are disabled until the original artifacts verify."}</InlineNotice>
      {result.regressionSuite.total > 0 && <InlineNotice tone={result.regressionSuite.blocking ? "danger" : "success"} title={result.regressionSuite.blocking ? "Pinned regression suite blocks release" : "Pinned regression suite passed"}>{result.regressionSuite.fixed + result.regressionSuite.passing} of {result.regressionSuite.total} required cases pass in this candidate. Missing, still-broken, and reintroduced cases are enforced again in generated CI.</InlineNotice>}
      {(summary.gate.quality_failures.length > 0 || summary.gate.evidence_failures.length > 0 || summary.gate.runtime_errors.length > 0) && <Card className="gate-audit"><div><h2>Why this decision was reached</h2><p>Quality, evidence, and runtime findings remain separate. None are hidden by another category.</p></div>{summary.gate.quality_failures.length > 0 && <GateFailures title="Quality failures" items={summary.gate.quality_failures} tone="fail" />}{summary.gate.evidence_failures.length > 0 && <GateFailures title="Evidence failures" items={summary.gate.evidence_failures} tone="warning" />}{summary.gate.runtime_errors.length > 0 && <GateFailures title="Runtime errors" items={summary.gate.runtime_errors} tone="fail" />}</Card>}
      <Card className="explanation"><CircleAlert size={20} /><p>{description}</p></Card>
      <Card className="outcome-visualization">
        <div className="panel-heading"><div><h2>Deployment outcome distribution</h2><p>Pass and failure counts over {independentTotal} independent evidence units. The scale is shared.</p></div><Status tone={summary.paired.difference_pp >= 0 ? "pass" : "fail"} label={`${summary.paired.difference_pp >= 0 ? "+" : ""}${summary.paired.difference_pp.toFixed(1)} pp`} /></div>
        <DistributionBar label="Baseline" pass={summary.baseline.deployment_success} total={independentTotal} />
        <DistributionBar label="Candidate" pass={summary.candidate.deployment_success} total={independentTotal} />
        <div className="chart-legend"><span><i className="legend-pass" /> Deployment success</span><span><i className="legend-fail" /> Parse, schema, semantic, or evaluator failure</span></div>
      </Card>
      <Card className="metrics-card">
        <div className="panel-heading"><div><h2>Independent deployment comparison</h2><p>Each percentage names the effective independent evidence-unit denominator.</p></div><Status tone="neutral" label={`${independentTotal} independent units`} /></div>
        <table className="metric-table"><caption>Metrics over independent, non-conflicting evidence units</caption><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th><th>Change</th></tr></thead><tbody>{metricRows.map(([label, baseline, candidate]) => { const delta = independentTotal ? ((candidate - baseline) / independentTotal) * 100 : 0; const reverse = label === "Valid but wrong"; const good = reverse ? delta <= 0 : delta >= 0; return <tr key={label}><th scope="row">{label}</th><td>{countPercent(baseline, independentTotal)}</td><td>{countPercent(candidate, independentTotal)}</td><td><span className={`delta ${good ? "delta-good" : "delta-bad"}`}>{delta >= 0 ? <ArrowUpRight size={15} /> : <ArrowDownRight size={15} />}{delta >= 0 ? "+" : ""}{delta.toFixed(1)} pp</span></td></tr>; })}</tbody></table>
      </Card>
      <Card className="metrics-card">
        <div className="panel-heading"><div><h2>Captured execution and semantic comparison</h2><p>Descriptive rows, independent deployment units, and fully evaluated semantic pairs are not interchangeable.</p></div></div>
        <div className="evidence-metrics denominator-metrics"><div><strong>{capturedTotal}</strong><span>Rows captured</span></div><div><strong>{independentTotal}</strong><span>Independent deployment units</span></div><div><strong>{semanticTotal}</strong><span>Semantic pairs fully evaluated</span></div><div><strong>{summary.jointly_scored_semantic.excluded_pairs}</strong><span>Semantic pairs excluded</span></div><div><strong>{summary.evidence.total_rows - independentTotal}</strong><span>Rows not multiplying inference</span></div></div>
        <div className="stat-line"><span>Semantic-only candidate − baseline</span><strong>{summary.jointly_scored_semantic.paired.difference_pp >= 0 ? "+" : ""}{summary.jointly_scored_semantic.paired.difference_pp.toFixed(1)} pp</strong><small>{semanticTotal} pairs had explicit binary semantic outcomes for both variants. Operational failures are not relabelled as wrong answers.</small></div>
      </Card>
      <div className="result-grid">
        <Card className="transition-card">
          <div className="panel-heading"><div><h2>Independent deployment outcomes</h2><p>{independentTotal} non-conflicting evidence units contribute once each.</p></div><Button variant="ghost" icon={ListFilter} onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: "" } })}>Inspect cases</Button></div>
          <div className="transition-matrix" role="img" aria-label={`${summary.paired.both_pass} both pass, ${regressions} regressions, ${improvements} improvements, ${summary.paired.both_fail} both fail`}>
            <div className="both-pass"><strong>{summary.paired.both_pass}</strong><span>Both pass</span></div><div className="regression"><strong>{regressions}</strong><span>Regressions</span></div><div className="improvement"><strong>{improvements}</strong><span>Improvements</span></div><div className="both-fail"><strong>{summary.paired.both_fail}</strong><span>Both fail</span></div>
          </div>
          <div className="stat-line"><span>Candidate − baseline</span><strong>{summary.paired.difference_pp >= 0 ? "+" : ""}{summary.paired.difference_pp.toFixed(1)} pp</strong><small>95% paired interval [{summary.bootstrap.lower_pp.toFixed(1)}, {summary.bootstrap.upper_pp.toFixed(1)}] pp</small></div>
          <EffectInterval estimate={summary.paired.difference_pp} lower={summary.bootstrap.lower_pp} upper={summary.bootstrap.upper_pp} />
        </Card>
        <Card className="hotspots-card">
          <div className="panel-heading"><div><h2>Top regression fields</h2><p>Primary correctness rules only.</p></div></div>
          {regressionHotspots.length ? <div className="hotspot-list">{regressionHotspots.slice(0, 6).map((hotspot) => <button key={hotspot.pointer} onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: hotspot.pointer } })}><code>{hotspot.pointer}</code><span><i style={{ width: `${Math.max(4, hotspot.regressions / Math.max(1, regressions) * 100)}%` }} /></span><strong>{hotspot.regressions}</strong></button>)}</div> : <div className="no-hotspots"><CheckCircle2 size={20} /><strong>No field-level regressions recorded</strong><p>Open case evidence for complete transition details.</p></div>}
        </Card>
      </div>
      <Card className="evidence-card">
        <div className="panel-heading"><div><h2>Evidence independence audit</h2><p>Captured execution remains visible even when rows cannot multiply inferential evidence.</p></div><Status tone={result.integrity.status === "verified" ? "pass" : "fail"} label={result.integrity.status === "verified" ? "Artifacts verified" : "Authority disabled"} /></div>
        <div className="evidence-funnel" role="img" aria-label={`${capturedTotal} rows captured, ${independentTotal} independent evidence units, ${semanticTotal} jointly scored semantic pairs`}><FunnelStep label="Captured rows" value={capturedTotal} max={capturedTotal} /><FunnelStep label="Independent units" value={independentTotal} max={capturedTotal} /><FunnelStep label="Semantic pairs" value={semanticTotal} max={capturedTotal} /></div>
        <div className="evidence-metrics"><div><strong>{summary.evidence.total_rows}</strong><span>Rows captured</span></div><div><strong>{summary.evidence.effective_inference_units}</strong><span>Effective independent units</span></div><div><strong>{summary.evidence.exact_duplicate_groups}</strong><span>Exact-duplicate groups</span></div><div className={summary.evidence.repeated_trial_groups ? "bad" : ""}><strong>{summary.evidence.repeated_trial_groups}</strong><span>Repeated-trial conflicts</span></div><div className={summary.evidence.label_conflict_groups ? "bad" : ""}><strong>{summary.evidence.label_conflict_groups}</strong><span>Label conflicts</span></div></div>
      </Card>
      <div className="result-actions"><Button icon={XCircle} variant="secondary" disabled={result.integrity.status !== "verified"} onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: "" } })}>Inspect regressions</Button><Button icon={Pin} variant="secondary" onClick={() => void navigate({ to: "/regressions" })}>Saved cases</Button>{result.integrity.status === "verified" && result.projectId && summary.gate.deployment_authorized && !accepted.isSuccess && <Button icon={ArrowRight} onClick={() => accepted.mutate()} disabled={accepted.isPending}>{accepted.isPending ? "Promoting…" : "Accept as next baseline"}</Button>}{accepted.data && <Button icon={ArrowRight} onClick={() => { void startNextIteration({ ...accepted.data.source, kind: "baseline", status: "ready" }).then(() => navigate({ to: "/new/source" })); }}>Open next comparison</Button>}</div>
      {accepted.data && <InlineNotice tone="success" title="Verified baseline revision committed">Replay-verified candidate bytes from run <code>{accepted.data.accepted.runId}</code> now define project revision <code>{accepted.data.accepted.projectRevisionId}</code>. Reopening the project and CI export resolve this same candidate hash.</InlineNotice>}
      {accepted.error && <InlineNotice tone="danger" title="Baseline promotion failed">{accepted.error.message}</InlineNotice>}
    </div>
  );
}

function DistributionBar({ label, pass, total }: { label: string; pass: number; total: number }) {
  const percent = total ? pass / total * 100 : 0;
  return <div className="distribution-row"><strong>{label}</strong><div className="distribution-track" role="img" aria-label={`${label}: ${pass} of ${total} deployment successes`}><span className="distribution-pass" style={{ width: `${percent}%` }} /><span className="distribution-fail" style={{ width: `${100 - percent}%` }} /></div><code>{pass}/{total}</code><b>{percent.toFixed(1)}%</b></div>;
}

function EffectInterval({ estimate, lower, upper }: { estimate: number; lower: number; upper: number }) {
  const position = effectPosition;
  return <div className="effect-interval" role="img" aria-label={`Paired effect ${estimate.toFixed(1)} percentage points with 95 percent interval ${lower.toFixed(1)} to ${upper.toFixed(1)}`}><div className="effect-axis"><span className="effect-zero" /><span className="effect-range" style={{ left: `${position(lower)}%`, width: `${Math.max(1, position(upper) - position(lower))}%` }} /><span className="effect-point" style={{ left: `${position(estimate)}%` }} /></div><div className="effect-labels"><span>−100 pp harm</span><span>0</span><span>+100 pp benefit</span></div></div>;
}

export function effectPosition(value: number) {
  return Math.max(0, Math.min(100, (value + 100) / 2));
}

function FunnelStep({ label, value, max }: { label: string; value: number; max: number }) { return <div><span>{label}</span><div><i style={{ width: `${max ? Math.max(2, value / max * 100) : 0}%` }} /></div><strong>{value}</strong></div>; }

function countPercent(value: number, total: number) { return total ? `${value}/${total} · ${((value / total) * 100).toFixed(1)}%` : `${value}/0 · N/A`; }
function GateFailures({ title, items, tone }: { title: string; items: string[]; tone: "fail" | "warning" }) { return <section className={`gate-failure gate-failure-${tone}`}><strong>{title}</strong><ul>{items.map((item) => <li key={item}>{item}</li>)}</ul></section>; }

export function aggregateHotspots(hotspots: Array<{ pointer: string; regressions: number }>) {
  const totals = new Map<string, number>();
  for (const hotspot of hotspots) totals.set(hotspot.pointer, (totals.get(hotspot.pointer) ?? 0) + hotspot.regressions);
  return [...totals.entries()]
    .map(([pointer, regressions]) => ({ pointer, regressions }))
    .sort((left, right) => right.regressions - left.regressions || left.pointer.localeCompare(right.pointer));
}

function exportSummary(result: RunResult) {
  const blob = new Blob([JSON.stringify({ runId: result.runId, projectName: result.projectName, integrity: result.integrity, authoritative: result.integrity.status === "verified", summary: result.summary }, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `structtrace-${result.runId}-summary.json`;
  link.click();
  URL.revokeObjectURL(url);
}
