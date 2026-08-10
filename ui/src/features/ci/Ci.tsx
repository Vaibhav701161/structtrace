import { useMutation } from "@tanstack/react-query";
import { Braces, Check, Copy, FileCode2, Github, ShieldAlert, Terminal } from "lucide-react";
import { useState } from "react";
import { generateCi } from "../../api/client";
import { Button, Card, InlineNotice, PageHeader, Status } from "../../design-system/components";

export function Ci() {
  const [mode, setMode] = useState<"regression" | "release">("regression");
  const [copied, setCopied] = useState("");
  const generated = useMutation({ mutationFn: () => generateCi(mode) });
  const copy = async (label: string, value: string) => { await navigator.clipboard.writeText(value); setCopied(label); window.setTimeout(() => setCopied(""), 1500); };
  return (
    <div className="page page-wide">
      <PageHeader eyebrow="Automation" title="Prepare a reproducible CI check" description="Generate a reviewable starter template. Tagged binary distribution is still a private-alpha release gate." />
      <div className="integration-grid">
        <button className="integration-card selected"><Github /><div><strong>GitHub Actions</strong><p>Preview a pull-request check and safe gate command.</p><Status tone="info" label="Template preview" /></div></button>
        <button className="integration-card"><Terminal /><div><strong>Generic shell CI</strong><p>Use the same deterministic command after installing the binary.</p><Status tone="info" label="Template preview" /></div></button>
      </div>
      <Card className="ci-config">
        <div className="panel-heading"><div><h2>Check authority</h2><p>The generated command must match the meaning of the result.</p></div></div>
        <div className="gate-grid compact"><button className={mode === "regression" ? "selected" : ""} onClick={() => setMode("regression")}><span className="radio-dot">{mode === "regression" && <span />}</span><div><h2>Regression check</h2><p>Fails configured relative-quality regressions. Never claims release authorization.</p></div></button><button className={mode === "release" ? "selected" : ""} onClick={() => setMode("release")}><span className="radio-dot">{mode === "release" && <span />}</span><div><h2>Release authorization</h2><p>Replays evidence and succeeds only for an authorizing release-mode decision.</p></div></button></div>
        {mode === "release" && <InlineNotice tone="warning" title="Authorization-safe command required">The workflow will use <code>structtrace release-check latest</code>, never a generic gate command.</InlineNotice>}
        <InlineNotice tone="warning" title="Review and complete the generated project">The configuration is a safe authority fragment, not a runnable replacement for the comparison-specific sources and correctness rules saved by a completed run.</InlineNotice>
        <Button icon={Braces} onClick={() => generated.mutate()} disabled={generated.isPending}>{generated.isPending ? "Generating…" : "Generate starter files"}</Button>
      </Card>
      {generated.data && <div className="generated-grid"><GeneratedFile title="structtrace.yaml" icon={FileCode2} content={generated.data.config} copied={copied === "config"} copy={() => void copy("config", generated.data!.config)} /><GeneratedFile title=".github/workflows/structtrace.yml" icon={Github} content={generated.data.workflow} copied={copied === "workflow"} copy={() => void copy("workflow", generated.data!.workflow)} /><Card className="safe-command"><ShieldAlert /><div><small>CHECK COMMAND</small><code>{generated.data.command}</code><p>This command is generated from the selected authority and validated by the local server.</p></div></Card></div>}
      {generated.error && <InlineNotice tone="danger" title="CI files could not be generated">{generated.error.message}</InlineNotice>}
    </div>
  );
}

function GeneratedFile({ title, icon: Icon, content, copied, copy }: { title: string; icon: typeof FileCode2; content: string; copied: boolean; copy: () => void }) {
  return <Card className="generated-file"><header><span><Icon size={17} />{title}</span><Button variant="ghost" icon={copied ? Check : Copy} onClick={copy}>{copied ? "Copied" : "Copy"}</Button></header><pre>{content}</pre></Card>;
}
