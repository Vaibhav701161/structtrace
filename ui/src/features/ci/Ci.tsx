import { useMutation } from "@tanstack/react-query";
import { Braces, Check, Copy, FileCode2, Github, ShieldAlert, Terminal } from "lucide-react";
import { useEffect, useState } from "react";
import { generateCi, getProjects, getRun } from "../../api/client";
import { useQuery } from "@tanstack/react-query";
import { Button, Card, InlineNotice, PageHeader, Status } from "../../design-system/components";
import { useSearch } from "@tanstack/react-router";

export function Ci() {
  const search = useSearch({ from: "/app/ci" });
  const [mode, setMode] = useState<"regression" | "release">("regression");
  const [copied, setCopied] = useState("");
  const projects = useQuery({ queryKey: ["projects"], queryFn: getProjects });
  const selectedRun = useQuery({ queryKey: ["run", search.run], queryFn: () => getRun(search.run), enabled: Boolean(search.run) });
  const project = projects.data?.find((item) => item.projectId === search.project);
  const explicitTarget = Boolean(search.project && search.run);
  const verified = explicitTarget
    && project?.integrity.status === "verified"
    && selectedRun.data?.integrity.status === "verified"
    && selectedRun.data.projectId === search.project;
  useEffect(() => {
    if (selectedRun.data) {
      setMode(selectedRun.data.summary.gate.gate_mode === "release" ? "release" : "regression");
    }
  }, [selectedRun.data, search.run]);
  const generated = useMutation({ mutationFn: () => generateCi(mode, search.project, search.run) });
  const copy = async (label: string, value: string) => { await navigator.clipboard.writeText(value); setCopied(label); window.setTimeout(() => setCopied(""), 1500); };
  return (
    <div className="page page-wide">
      <PageHeader eyebrow="Automation" title="Export a reproducible CI project" description="Materialize the saved configuration, data contract, paired sources, pinned toolchain, safe gate, and immutable evidence upload." />
      <InlineNotice tone={verified ? "success" : "danger"} title={verified ? "Explicit project and run verified" : "CI authority unavailable"}>{project?.integrity.detail ?? "Open a verified comparison and choose Export CI project so the route binds an explicit project and run."}{explicitTarget && <> Run <code>{search.run}</code>.</>}{project?.revisionId && <> Revision <code>{project.revisionId}</code>.</>}</InlineNotice>
      <div className="integration-grid">
        <button className="integration-card selected"><Github /><div><strong>GitHub Actions</strong><p>Complete project snapshot with a commit-pinned StructTrace install.</p><Status tone="pass" label="Runnable export" /></div></button>
        <button className="integration-card"><Terminal /><div><strong>Generic shell CI</strong><p>The generated config and authority-safe command are portable to any shell runner.</p><Status tone="info" label="Included" /></div></button>
      </div>
      <Card className="ci-config">
        <div className="panel-heading"><div><h2>Check authority</h2><p>The generated command must match the meaning of the result.</p></div></div>
        <div className="gate-grid compact"><button className={mode === "regression" ? "selected" : ""} onClick={() => setMode("regression")}><span className="radio-dot">{mode === "regression" && <span />}</span><div><h2>Regression check</h2><p>Fails configured relative-quality regressions. Never claims release authorization.</p></div></button><button className={mode === "release" ? "selected" : ""} onClick={() => setMode("release")}><span className="radio-dot">{mode === "release" && <span />}</span><div><h2>Release authorization</h2><p>Replays evidence and succeeds only for an authorizing release-mode decision.</p></div></button></div>
        {mode === "release" && <InlineNotice tone="warning" title="Authorization-safe command required">The workflow will use <code>structtrace release-check latest</code>, never a generic gate command.</InlineNotice>}
        <InlineNotice title="Candidate acquisition remains yours">The export contains a runnable evidence snapshot. Your existing model pipeline should replace <code>outputs/candidate.jsonl</code> before the check; StructTrace never guesses how production outputs are generated.</InlineNotice>
        <Button icon={Braces} onClick={() => generated.mutate()} disabled={generated.isPending || !verified}>{generated.isPending ? "Exporting…" : "Export complete CI project"}</Button>
      </Card>
      {generated.data && <><InlineNotice tone="success" title="Complete CI project exported"><code>{generated.data.export_path}</code><p>{generated.data.files.length} files were materialized from the saved project, including the full evaluator configuration and source snapshot.</p></InlineNotice><div className="generated-grid"><GeneratedFile title="structtrace.yaml" icon={FileCode2} content={generated.data.config} copied={copied === "config"} copy={() => void copy("config", generated.data!.config)} /><GeneratedFile title=".github/workflows/structtrace.yml" icon={Github} content={generated.data.workflow} copied={copied === "workflow"} copy={() => void copy("workflow", generated.data!.workflow)} /><Card className="safe-command"><ShieldAlert /><div><small>CHECK COMMAND</small><code>{generated.data.command}</code><p>Generated from the selected authority and validated against the complete saved project.</p></div></Card></div></>}
      {generated.error && <InlineNotice tone="danger" title="CI files could not be generated">{generated.error.message}</InlineNotice>}
    </div>
  );
}

function GeneratedFile({ title, icon: Icon, content, copied, copy }: { title: string; icon: typeof FileCode2; content: string; copied: boolean; copy: () => void }) {
  return <Card className="generated-file"><header><span><Icon size={17} />{title}</span><Button variant="ghost" icon={copied ? Check : Copy} onClick={copy}>{copied ? "Copied" : "Copy"}</Button></header><pre>{content}</pre></Card>;
}
