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
import { parseArtifact } from "../import/inspect";

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
  const gate = result.summary.gate;
  if (gate.gate_mode === "release" && gate.deployment_authorized) return { title: "RELEASE AUTHORIZED", text: "Candidate passed every configured release and evidence rule.", tone: "pass" as const, icon: ShieldCheck };
  if (gate.status === "insufficient_evidence") return { title: "NOT ENOUGH EVIDENCE", text: gate.evidence_failures[0] ?? "The configured evidence requirement was not met.", tone: "warning" as const, icon: TriangleAlert };
  if (gate.status === "error") return { title: "RUN ERROR", text: gate.runtime_errors[0] ?? "A required rule could not be evaluated safely.", tone: "fail" as const, icon: CircleAlert };
  if (gate.status === "failed") return { title: "DO NOT DEPLOY", text: gate.quality_failures[0] ?? "Candidate failed a configured quality threshold.", tone: "fail" as const, icon: ShieldAlert };
  if (gate.gate_mode === "regression" && gate.status === "passed") return { title: "REGRESSION CHECK PASSED", text: "No configured regression threshold failed. This is not release authorization.", tone: "info" as const, icon: CheckCircle2 };
  return { title: "ANALYSIS COMPLETE", text: "No deployment authorization was configured for this comparison.", tone: "neutral" as const, icon: FileCheck2 };
}

function ResultContent({ result }: { result: RunResult }) {
  const navigate = useNavigate();
  const { startNextIteration } = useWorkspace();
  const accepted = useMutation({ mutationFn: () => acceptRun(result.runId) });
  const state = decision(result);
  const summary = result.summary;
  const total = summary.baseline.total || 1;
  const metricRows = [
    ["Deployment success", summary.baseline.deployment_success, summary.candidate.deployment_success],
    ["Strict JSON", summary.baseline.parse_valid, summary.candidate.parse_valid],
    [result.schemaProvenance === "inferred_from_expected_values" ? "Schema valid (inferred shape)" : "Schema valid", summary.baseline.schema_valid, summary.candidate.schema_valid],
    ["Semantically correct", summary.baseline.semantic_success, summary.candidate.semantic_success],
    ["Valid but wrong", summary.baseline.valid_but_wrong, summary.candidate.valid_but_wrong],
  ] as const;
  const regressions = summary.paired.baseline_only_pass;
  const improvements = summary.paired.candidate_only_pass;
  const description = regressions > improvements
    ? `Candidate created ${regressions} regressions and ${improvements} improvements. Structural success does not override those semantic losses.`
    : `Candidate created ${improvements} improvements and ${regressions} regressions. Review every discordant case before changing production behavior.`;
  return (
    <div className="page page-wide result-page">
      <PageHeader eyebrow={`${result.projectName} · immutable run ${result.runId}`} title="Should I ship?" actions={<><Button variant="secondary" icon={Download} onClick={() => exportSummary(result)}>Export summary</Button><Button icon={GitPullRequest} onClick={() => void navigate({ to: "/ci" })}>Review CI starter</Button></>} />
      <section className={`decision-banner decision-${state.tone}`}>
        <state.icon size={28} aria-hidden="true" />
        <div><span>RELEASE DECISION</span><h2>{state.title}</h2><p>{state.text}</p></div>
        <Status tone={state.tone === "pass" ? "pass" : state.tone === "fail" ? "fail" : state.tone === "warning" ? "warning" : "info"} label={summary.gate.gate_mode === "release" ? "Release gate" : summary.gate.gate_mode === "regression" ? "Regression gate" : "Advisory"} />
      </section>
      <Card className="explanation"><CircleAlert size={20} /><p>{description}</p></Card>
      <Card className="metrics-card">
        <div className="panel-heading"><div><h2>Baseline vs candidate</h2><p>All percentages use the complete matched denominator.</p></div><Status tone="neutral" label={`${summary.evidence.total_rows} rows`} /></div>
        <table className="metric-table"><caption>Primary structural and semantic metrics</caption><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th><th>Change</th></tr></thead><tbody>{metricRows.map(([label, baseline, candidate]) => { const delta = ((candidate - baseline) / total) * 100; const reverse = label === "Valid but wrong"; const good = reverse ? delta <= 0 : delta >= 0; return <tr key={label}><th scope="row">{label}</th><td>{percent(baseline, total)}</td><td>{percent(candidate, total)}</td><td><span className={`delta ${good ? "delta-good" : "delta-bad"}`}>{delta >= 0 ? <ArrowUpRight size={15} /> : <ArrowDownRight size={15} />}{delta >= 0 ? "+" : ""}{delta.toFixed(1)} pp</span></td></tr>; })}</tbody></table>
      </Card>
      <div className="result-grid">
        <Card className="transition-card">
          <div className="panel-heading"><div><h2>Paired outcomes</h2><p>Every case stays paired across the change.</p></div><Button variant="ghost" icon={ListFilter} onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: "" } })}>Inspect cases</Button></div>
          <div className="transition-matrix" role="img" aria-label={`${summary.paired.both_pass} both pass, ${regressions} regressions, ${improvements} improvements, ${summary.paired.both_fail} both fail`}>
            <div className="both-pass"><strong>{summary.paired.both_pass}</strong><span>Both pass</span></div><div className="regression"><strong>{regressions}</strong><span>Regressions</span></div><div className="improvement"><strong>{improvements}</strong><span>Improvements</span></div><div className="both-fail"><strong>{summary.paired.both_fail}</strong><span>Both fail</span></div>
          </div>
          <div className="stat-line"><span>Candidate − baseline</span><strong>{summary.paired.difference_pp >= 0 ? "+" : ""}{summary.paired.difference_pp.toFixed(1)} pp</strong><small>95% paired interval [{summary.bootstrap.lower_pp.toFixed(1)}, {summary.bootstrap.upper_pp.toFixed(1)}] pp</small></div>
        </Card>
        <Card className="hotspots-card">
          <div className="panel-heading"><div><h2>Top regression fields</h2><p>Primary correctness rules only.</p></div></div>
          {summary.primary_field_hotspots.length ? <div className="hotspot-list">{summary.primary_field_hotspots.slice(0, 6).map((hotspot) => <button key={`${hotspot.evaluator_id}:${hotspot.pointer}`} onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: hotspot.pointer } })}><code>{hotspot.pointer}</code><span><i style={{ width: `${Math.max(4, hotspot.regressions / Math.max(1, regressions) * 100)}%` }} /></span><strong>{hotspot.regressions}</strong></button>)}</div> : <div className="no-hotspots"><CheckCircle2 size={20} /><strong>No field-level regressions recorded</strong><p>Open case evidence for complete transition details.</p></div>}
        </Card>
      </div>
      <Card className="evidence-card">
        <div className="panel-heading"><div><h2>Evidence quality</h2><p>Inference stays separate from descriptive row counts.</p></div><Status tone="pass" label="Hash-bound artifacts" /></div>
        <div className="evidence-metrics"><div><strong>{summary.evidence.total_rows}</strong><span>Total rows</span></div><div><strong>{summary.evidence.effective_inference_units}</strong><span>Independent cases</span></div><div><strong>{summary.evidence.exact_duplicate_groups}</strong><span>Duplicate groups</span></div><div className={summary.evidence.repeated_trial_groups ? "bad" : ""}><strong>{summary.evidence.repeated_trial_groups}</strong><span>Repeated-trial conflicts</span></div><div className={summary.evidence.label_conflict_groups ? "bad" : ""}><strong>{summary.evidence.label_conflict_groups}</strong><span>Label conflicts</span></div></div>
      </Card>
      <div className="result-actions"><Button icon={XCircle} variant="secondary" onClick={() => void navigate({ to: "/runs/$runId/cases", params: { runId: result.runId }, search: { search: "" } })}>Inspect regressions</Button><Button icon={Pin} variant="secondary" onClick={() => void navigate({ to: "/regressions" })}>Saved cases</Button>{result.projectId && summary.gate.deployment_authorized && !accepted.isSuccess && <Button icon={ArrowRight} onClick={() => accepted.mutate()} disabled={accepted.isPending}>{accepted.isPending ? "Promoting…" : "Accept as next baseline"}</Button>}{accepted.data && <Button icon={ArrowRight} onClick={() => { const parsed = parseArtifact("baseline", accepted.data.source.name, accepted.data.source.content); startNextIteration({ ...parsed, sourceId: accepted.data.source.sourceId, hash: accepted.data.source.hash }); void navigate({ to: "/new/source" }); }}>Start next comparison</Button>}</div>
      {accepted.data && <InlineNotice tone="success" title="Authorized baseline recorded">Candidate bytes from run <code>{accepted.data.accepted.runId}</code> are hash-bound and will be the default baseline for the next comparison in this project.</InlineNotice>}
      {accepted.error && <InlineNotice tone="danger" title="Baseline promotion failed">{accepted.error.message}</InlineNotice>}
    </div>
  );
}

function percent(value: number, total: number) { return `${((value / total) * 100).toFixed(1)}%`; }

function exportSummary(result: RunResult) {
  const blob = new Blob([JSON.stringify({ runId: result.runId, projectName: result.projectName, summary: result.summary }, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `structtrace-${result.runId}-summary.json`;
  link.click();
  URL.revokeObjectURL(url);
}
