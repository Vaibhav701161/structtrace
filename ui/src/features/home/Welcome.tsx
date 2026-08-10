import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowRight, FileSearch, FolderOpen, Play, ShieldCheck } from "lucide-react";
import { runDemo } from "../../api/client";
import { Brand, Button, InlineNotice } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";

export function Welcome() {
  const navigate = useNavigate();
  const { setResult, reset } = useWorkspace();
  const demo = useMutation({
    mutationFn: runDemo,
    onSuccess: (result) => { setResult(result); void navigate({ to: "/runs/$runId", params: { runId: result.runId } }); },
  });

  return (
    <main className="welcome">
      <header className="welcome-brand"><Brand /></header>
      <section className="welcome-hero">
        <div className="hero-copy">
          <div className="eyebrow"><span className="signal-dot" /> Local-first release evidence</div>
          <h1>Your schema passed.<br /><span>Did the answer?</span></h1>
          <p>Compare two structured-output systems and catch valid-but-wrong regressions before shipping a change.</p>
          <div className="hero-actions">
            <Button icon={ArrowRight} onClick={() => { reset(); void navigate({ to: "/new/source" }); }}>Compare a change</Button>
            <Button variant="secondary" icon={demo.isPending ? undefined : Play} onClick={() => demo.mutate()} disabled={demo.isPending}>{demo.isPending ? "Running local demo…" : "Try invoice demo"}</Button>
          </div>
          {demo.isError && <InlineNotice tone="danger" title="Demo could not run">{demo.error.message}</InlineNotice>}
          <div className="trust-row"><span><ShieldCheck size={16} /> No account</span><span>No telemetry</span><span>Your data stays on this machine</span></div>
          <div className="secondary-links"><button onClick={() => void navigate({ to: "/projects" })}><FolderOpen size={16} /> Open a saved project</button><a href="https://github.com/Vaibhav701161/structtrace/tree/main/docs"><FileSearch size={16} /> View documentation</a></div>
        </div>
        <div className="hero-product" aria-label="Example StructTrace release decision">
          <div className="product-window-bar"><span /><span /><span /><small>invoice-extraction / bundled verified demo</small></div>
          <div className="decision-preview">
            <div className="preview-label preview-warning"><span className="status-icon-fail">!</span><div><small>REGRESSION CHECK</small><strong>NOT ENOUGH EVIDENCE</strong></div></div>
            <p>Candidate fixed structure, but semantic correctness did not improve.</p>
          </div>
          <div className="preview-metrics">
            <div><span>Schema valid</span><strong>10/12 <em>→</em> 12/12</strong><small className="positive">+16.7 pp</small></div>
            <div><span>Semantically correct</span><strong>9/12 <em>→</em> 9/12</strong><small className="neutral-change">0.0 pp</small></div>
            <div><span>Valid but wrong</span><strong>1/12 <em>→</em> 3/12</strong><small className="negative">+16.7 pp</small></div>
          </div>
          <div className="preview-chart"><div className="both-pass-preview"><span>Both pass</span><i style={{ width: "100%" }} /><b>6</b></div><div><span>Regressions</span><i style={{ width: "50%" }} /><b>3</b></div><div><span>Improvements</span><i className="improvement-bar" style={{ width: "50%" }} /><b>3</b></div></div>
        </div>
      </section>
      <footer className="welcome-footer">Built on a deterministic Rust evidence engine. Works offline after installation.</footer>
    </main>
  );
}
