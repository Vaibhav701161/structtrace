import { useMutation, useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowRight, FileCheck2, FolderOpen, GitPullRequest, Play, Plus, ShieldCheck } from "lucide-react";
import { Button, Card, EmptyState, PageHeader, Status } from "../../design-system/components";
import { getPinnedCases, getRuns, runDemo } from "../../api/client";
import { useWorkspace } from "../../state/workspace";

export function Home() {
  const navigate = useNavigate();
  const { setResult } = useWorkspace();
  const runs = useQuery({ queryKey: ["runs"], queryFn: getRuns });
  const pins = useQuery({ queryKey: ["regressions"], queryFn: getPinnedCases });
  const demo = useMutation({ mutationFn: runDemo, onSuccess: (result) => { setResult(result); void navigate({ to: "/runs/$runId", params: { runId: result.runId } }); } });
  return (
    <div className="page page-wide">
      <PageHeader title="What do you want to test?" description="Start with recorded outputs. StructTrace will generate the reproducible project and CI check." />
      <div className="action-grid">
        <button className="action-card primary-action" onClick={() => void navigate({ to: "/new/source" })}><span className="action-icon"><Plus /></span><strong>Compare a change</strong><p>Test a new model, prompt, provider, decoder, or implementation.</p><span className="card-link">Start comparison <ArrowRight size={15} /></span></button>
        <button className="action-card" onClick={() => demo.mutate()} disabled={demo.isPending}><span className="action-icon"><Play /></span><strong>{demo.isPending ? "Running local demo…" : "Try invoice extraction demo"}</strong><p>See why schema validity and semantic correctness are separate.</p><span className="card-link">Open workflow <ArrowRight size={15} /></span></button>
        <button className="action-card" disabled title="Folder-based setup import is not part of the current private alpha"><span className="action-icon"><FolderOpen /></span><strong>Open an existing setup</strong><p>Folder import is the next private-alpha workflow. Existing UI drafts resume automatically.</p><span className="card-link">Not yet available</span></button>
      </div>
      <div className="home-grid">
        <Card className="home-panel">
          <div className="panel-heading"><div><h2>Recent comparisons</h2><p>Completed runs are immutable.</p></div><Button variant="ghost" onClick={() => void navigate({ to: "/runs" })}>View all</Button></div>
          {runs.data?.length ? <div className="recent-list">{runs.data.slice(0, 3).map((run) => <button key={run.runId} onClick={() => void navigate({ to: "/runs/$runId", params: { runId: run.runId } })}><span><strong>{run.projectName}</strong><small>{run.runId}</small></span><span><code>{run.summary.paired.difference_pp >= 0 ? "+" : ""}{run.summary.paired.difference_pp.toFixed(1)} pp</code><ArrowRight size={15} /></span></button>)}</div> : <EmptyState icon={FileCheck2} title={runs.isLoading ? "Loading comparisons…" : "No production comparisons yet"} description="Your first completed comparison will appear here with its decision and evidence count." />}
        </Card>
        <div className="side-stack">
          <Card><div className="compact-heading"><ShieldCheck size={18} /><h2>Regression suite</h2></div><p className="muted">Pin important failures from a completed run so future changes cannot silently reintroduce them.</p><Status tone={pins.data?.length ? "info" : "neutral"} label={pins.isLoading ? "Loading…" : `${pins.data?.length ?? 0} pinned ${pins.data?.length === 1 ? "case" : "cases"}`} /></Card>
          <Card><div className="compact-heading"><GitPullRequest size={18} /><h2>CI status</h2></div><p className="muted">Generate a deterministic check after your first real comparison.</p><Status tone="neutral" label="Not configured" /></Card>
        </div>
      </div>
    </div>
  );
}
