import { useMutation } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  AlertTriangle,
  ArrowRight,
  Braces,
  Check,
  CheckCircle2,
  ChevronDown,
  CircleDot,
  FileJson,
  FileSpreadsheet,
  FolderLock,
  Info,
  LoaderCircle,
  LockKeyhole,
  Play,
  ShieldCheck,
  Sparkles,
  Upload,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { runComparison } from "../../api/client";
import type { ComparisonRequest, FieldRule, GateMode, SourceArtifact, SourceKind } from "../../api/types";
import { Button, Card, InlineNotice, PageHeader, Status, Stepper, WizardActions } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";
import { discoverRules, inferPointer, parseArtifact, parseRows, pointerCandidates, valueAt } from "../import/inspect";

const paths = ["/new/source", "/new/map", "/new/correctness", "/new/evidence", "/new/review", "/new/run"] as const;

export function Wizard({ step }: { step: number }) {
  return (
    <div className="page wizard-page">
      <Stepper current={step} />
      <div className="wizard-content">
        {step === 0 && <SourcesStep />}
        {step === 1 && <MappingStep />}
        {step === 2 && <CorrectnessStep />}
        {step === 3 && <EvidenceStep />}
        {step === 4 && <ReviewStep />}
        {step === 5 && <RunStep />}
      </div>
    </div>
  );
}

function useStepNavigation(step: number) {
  const navigate = useNavigate();
  return {
    back: step > 0 ? () => void navigate({ to: paths[step - 1] }) : undefined,
    next: () => void navigate({ to: paths[Math.min(step + 1, paths.length - 1)] }),
  };
}

const sourceDefinitions: Array<{ kind: SourceKind; title: string; description: string; required: boolean }> = [
  { kind: "dataset", title: "Golden / expected data", description: "Case IDs, inputs, and expected outputs", required: true },
  { kind: "baseline", title: "Baseline results", description: "Current production or accepted outputs", required: true },
  { kind: "candidate", title: "Candidate results", description: "Outputs from the change you want to test", required: true },
  { kind: "schema", title: "Output schema", description: "Caller-facing JSON Schema", required: false },
];

function SourcesStep() {
  const { draft, setSource } = useWorkspace();
  const navigation = useStepNavigation(0);
  const ready = ["dataset", "baseline", "candidate"].every((kind) => draft.sources[kind as SourceKind]?.status === "ready");
  return (
    <>
      <PageHeader eyebrow="New comparison" title="What are you comparing?" description="Drop matched artifacts. Files are parsed and evaluated on this machine." />
      <div className="mode-switch" role="group" aria-label="Comparison source mode">
        <button className="selected"><FileJson size={18} /><span><strong>Recorded outputs</strong><small>Recommended</small></span><CheckCircle2 size={18} /></button>
        <button disabled title="Available after recorded-output private alpha"><Play size={18} /><span><strong>Run systems locally</strong><small>Beta · coming next</small></span></button>
      </div>
      <div className="source-grid">
        {sourceDefinitions.map((definition) => <SourceDrop key={definition.kind} {...definition} source={draft.sources[definition.kind]} onSource={(source) => setSource(definition.kind, source)} />)}
      </div>
      <InlineNotice title="Local by design"><span>StructTrace reads these files through its loopback-only server. No account, telemetry, provider call, or external upload is involved.</span></InlineNotice>
      <WizardActions next={navigation.next} nextLabel="Continue to field mapping" disabled={!ready} />
    </>
  );
}

function SourceDrop({ kind, title, description, required, source, onSource }: {
  kind: SourceKind; title: string; description: string; required: boolean; source?: SourceArtifact; onSource: (source: SourceArtifact) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const read = (file: File) => {
    const reader = new FileReader();
    reader.onload = () => onSource(parseArtifact(kind, file.name, String(reader.result ?? "")));
    reader.onerror = () => onSource({ kind, name: file.name, format: "jsonl", content: "", bytes: file.size, rows: 0, status: "error", message: "The browser could not read this file." });
    reader.readAsText(file);
  };
  return (
    <article className={`source-card ${source?.status === "ready" ? "source-ready" : ""} ${source?.status === "error" ? "source-error" : ""}`}>
      <div className="source-title"><div className="file-icon">{kind === "schema" ? <Braces /> : <FileSpreadsheet />}</div><div><h2>{title} {!required && <span>Optional</span>}</h2><p>{description}</p></div></div>
      {source ? (
        <div className="file-summary">
          <div><strong>{source.name}</strong><small>{source.status === "ready" ? `${source.rows.toLocaleString()} ${source.rows === 1 ? "record" : "records"} · ${source.format.toUpperCase()} · ${formatBytes(source.bytes)}` : source.message}</small></div>
          <Status tone={source.status === "ready" ? "pass" : "fail"} label={source.status === "ready" ? "Ready" : "Needs attention"} />
          <button onClick={() => input.current?.click()}>Replace</button>
        </div>
      ) : (
        <button className="dropzone" onClick={() => input.current?.click()} onDragOver={(event) => event.preventDefault()} onDrop={(event) => { event.preventDefault(); const file = event.dataTransfer.files[0]; if (file) read(file); }}>
          <Upload size={20} /><span><strong>Drop a file or choose one</strong><small>JSONL, JSON, or CSV · processed locally</small></span>
        </button>
      )}
      <input ref={input} className="visually-hidden" type="file" accept=".json,.jsonl,.ndjson,.csv" onChange={(event) => { const file = event.target.files?.[0]; if (file) read(file); }} />
    </article>
  );
}

function formatBytes(bytes: number) { return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`; }

function MappingStep() {
  const { draft, updateDraft } = useWorkspace();
  const navigation = useStepNavigation(1);
  const dataset = draft.sources.dataset!;
  const baseline = draft.sources.baseline!;
  const candidate = draft.sources.candidate!;
  const dataRows = useMemo(() => parseRows(dataset.content, dataset.format), [dataset]);
  const baselineRows = useMemo(() => parseRows(baseline.content, baseline.format), [baseline]);
  const candidateRows = useMemo(() => parseRows(candidate.content, candidate.format), [candidate]);
  const dataPointers = useMemo(() => pointerCandidates(dataRows), [dataRows]);
  const baselinePointers = useMemo(() => pointerCandidates(baselineRows), [baselineRows]);
  const candidatePointers = useMemo(() => pointerCandidates(candidateRows), [candidateRows]);
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    if (initialized) return;
    updateDraft({ mapping: {
      datasetId: inferPointer(dataPointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.datasetId),
      datasetInput: inferPointer(dataPointers, ["/input", "/document", "/payload", "/text"], draft.mapping.datasetInput),
      datasetExpected: inferPointer(dataPointers, ["/expected", "/ground_truth", "/reference", "/target"], draft.mapping.datasetExpected),
      baselineId: inferPointer(baselinePointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.baselineId),
      baselineOutput: inferPointer(baselinePointers, ["/output", "/parsed_output", "/raw_output", "/result", "/response", "/prediction"], draft.mapping.baselineOutput),
      candidateId: inferPointer(candidatePointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.candidateId),
      candidateOutput: inferPointer(candidatePointers, ["/output", "/parsed_output", "/raw_output", "/result", "/response", "/prediction"], draft.mapping.candidateOutput),
    }});
    setInitialized(true);
  }, [baselinePointers, candidatePointers, dataPointers, draft.mapping, initialized, updateDraft]);

  const ids = (rows: unknown[], pointer: string) => rows.map((row) => valueAt(row, pointer)).filter((value): value is string => typeof value === "string");
  const dataIds = ids(dataRows, draft.mapping.datasetId);
  const baselineIds = new Set(ids(baselineRows, draft.mapping.baselineId));
  const candidateIds = new Set(ids(candidateRows, draft.mapping.candidateId));
  const duplicates = dataIds.filter((id, index) => dataIds.indexOf(id) !== index);
  const matched = dataIds.filter((id) => baselineIds.has(id) && candidateIds.has(id)).length;
  const missingCandidate = dataIds.filter((id) => !candidateIds.has(id)).length;
  const leakage = draft.mapping.datasetInput === draft.mapping.datasetExpected;
  const valid = dataIds.length > 0 && baselineIds.size > 0 && candidateIds.size > 0 && duplicates.length === 0 && !leakage;

  const set = (key: keyof typeof draft.mapping, value: string) => updateDraft({ mapping: { ...draft.mapping, [key]: value } });
  return (
    <>
      <PageHeader eyebrow="Step 2 of 6" title="Confirm how your files are structured" description="StructTrace suggests fields deterministically. Inspect a sample before continuing." />
      <div className="mapping-grid">
        <MappingCard title="Golden data" source={dataset} rows={dataRows} options={dataPointers} fields={[
          ["Case ID", "datasetId", draft.mapping.datasetId], ["Input", "datasetInput", draft.mapping.datasetInput], ["Expected output", "datasetExpected", draft.mapping.datasetExpected],
        ]} set={set} />
        <MappingCard title="Baseline" source={baseline} rows={baselineRows} options={baselinePointers} fields={[["Case ID", "baselineId", draft.mapping.baselineId], ["Output", "baselineOutput", draft.mapping.baselineOutput]]} set={set} />
        <MappingCard title="Candidate" source={candidate} rows={candidateRows} options={candidatePointers} fields={[["Case ID", "candidateId", draft.mapping.candidateId], ["Output", "candidateOutput", draft.mapping.candidateOutput]]} set={set} />
      </div>
      <Card className="coverage-card">
        <div><strong>{matched.toLocaleString()}</strong><span>matched cases</span></div>
        <div className={duplicates.length ? "metric-bad" : ""}><strong>{duplicates.length}</strong><span>duplicate IDs</span></div>
        <div><strong>{dataIds.filter((id) => !baselineIds.has(id)).length}</strong><span>missing baseline</span></div>
        <div className={missingCandidate ? "metric-warn" : ""}><strong>{missingCandidate}</strong><span>missing candidate</span></div>
      </Card>
      {missingCandidate > 0 && <InlineNotice tone="warning" title="Missing candidate outputs stay in the denominator">{missingCandidate} cases will count as candidate failures. They will not disappear from the result.</InlineNotice>}
      {leakage && <InlineNotice tone="danger" title="Reference leakage risk">Input and expected output resolve to the same path. Choose distinct fields before continuing.</InlineNotice>}
      {duplicates.length > 0 && <InlineNotice tone="danger" title="Duplicate case IDs">Resolve duplicate IDs before running. First affected ID: <code>{duplicates[0]}</code>.</InlineNotice>}
      <WizardActions back={navigation.back} next={navigation.next} nextLabel="Looks right" disabled={!valid} />
    </>
  );
}

type MappingKey = "datasetId" | "datasetInput" | "datasetExpected" | "baselineId" | "baselineOutput" | "candidateId" | "candidateOutput";
function MappingCard({ title, source, rows, options, fields, set }: {
  title: string; source: SourceArtifact; rows: unknown[]; options: string[]; fields: Array<[string, MappingKey, string]>; set: (key: MappingKey, value: string) => void;
}) {
  return (
    <Card className="mapping-card">
      <div className="mapping-title"><div><h2>{title}</h2><p>{source.name}</p></div><Status tone="pass" label={`${rows.length} rows`} /></div>
      {fields.map(([label, key, value]) => <label className="field-select" key={key}><span>{label}<small>High confidence</small></span><div><select value={value} onChange={(event) => set(key, event.target.value)}>{options.map((pointer) => <option key={pointer}>{pointer}</option>)}</select><ChevronDown size={15} /></div></label>)}
      <div className="sample-json"><small>Sample record</small><pre>{JSON.stringify(rows[0], null, 2).slice(0, 620)}</pre></div>
    </Card>
  );
}

function CorrectnessStep() {
  const { draft, setRules } = useWorkspace();
  const navigation = useStepNavigation(2);
  const discovered = useMemo(() => discoverRules(draft.sources.dataset!, draft.sources.baseline!, draft.sources.candidate!, draft.mapping.datasetExpected, draft.mapping.baselineOutput, draft.mapping.candidateOutput), [draft.mapping, draft.sources]);
  const [initialized, setInitialized] = useState(false);
  useEffect(() => { if (!initialized) { setRules(draft.rules.length ? draft.rules : discovered); setInitialized(true); } }, [discovered, draft.rules, initialized, setRules]);
  const update = (pointer: string, next: Partial<FieldRule>) => setRules(draft.rules.map((rule) => rule.pointer === pointer ? { ...rule, ...next } : rule));
  const enabled = draft.rules.filter((rule) => rule.enabled);
  return (
    <>
      <PageHeader eyebrow="Step 3 of 6" title="What does correct mean for your application?" description="Schema validity is never treated as task correctness. Select the fields and deterministic rules that define success." />
      <div className="correctness-summary"><div><CircleDot size={18} /><span><strong>{enabled.length} fields define correctness</strong><small>A case passes only when every selected rule passes.</small></span></div><Status tone={enabled.length ? "info" : "warning"} label={enabled.length ? "All selected rules must pass" : "Select at least one field"} /></div>
      <Card className="rules-card">
        <table className="rules-table">
          <caption>Discovered semantic fields and comparison rules</caption>
          <thead><tr><th scope="col">Use</th><th scope="col">Field</th><th scope="col">Type</th><th scope="col">Expected</th><th scope="col">Baseline</th><th scope="col">Candidate</th><th scope="col">Comparison</th></tr></thead>
          <tbody>{draft.rules.map((rule) => {
            const missing = rule.candidateCoverage < rule.expectedCoverage;
            return <tr key={rule.pointer} className={!rule.enabled ? "disabled-row" : missing ? "missing-row" : ""}><td><input type="checkbox" checked={rule.enabled} onChange={(event) => update(rule.pointer, { enabled: event.target.checked })} aria-label={`Use ${rule.pointer}`} /></td><td><code>{rule.pointer}</code>{missing && <small className="coverage-warning"><AlertTriangle size={13} /> Candidate omission</small>}</td><td>{rule.observedType}</td><td>{formatCoverage(rule.expectedCoverage)}</td><td>{formatCoverage(rule.baselineCoverage)}</td><td>{formatCoverage(rule.candidateCoverage)}</td><td><select value={rule.kind} onChange={(event) => update(rule.pointer, { kind: event.target.value as FieldRule["kind"] })} aria-label={`Comparison for ${rule.pointer}`}><option value="exact">Exact value</option><option value="normalized_string">Normalized text</option><option value="canonical_date">Calendar date</option><option value="exact_integer">Exact integer</option><option value="decimal_exact">Exact decimal</option><option value="decimal_tolerance">Decimal tolerance</option></select></td></tr>;
          })}</tbody>
        </table>
        {!draft.rules.length && <div className="table-empty"><AlertTriangle size={20} /><strong>No semantic fields were discovered</strong><p>Check the expected and output mappings on the previous step.</p></div>}
      </Card>
      <InlineNotice title="Why suggestions appear">Suggestions come only from observed JSON types and field names. They are never silently activated or changed after you review them.</InlineNotice>
      <WizardActions back={navigation.back} next={navigation.next} disabled={!enabled.length} />
    </>
  );
}
function formatCoverage(value: number) { return `${Math.round(value * 100)}%`; }

const modes: Array<{ mode: GateMode; title: string; text: string; badge?: string }> = [
  { mode: "advisory", title: "Advisory analysis", text: "Explore results. Never authorizes deployment." },
  { mode: "regression", title: "Regression check", text: "Detect whether candidate quality regressed. Not release authorization.", badge: "Recommended" },
  { mode: "release", title: "Release decision", text: "Requires sufficient clean evidence and absolute quality floors." },
];
function EvidenceStep() {
  const { draft, updateDraft } = useWorkspace();
  const navigation = useStepNavigation(3);
  const rows = draft.sources.dataset?.rows ?? 0;
  const sufficient = rows >= draft.minCases;
  return (
    <>
      <PageHeader eyebrow="Step 4 of 6" title="How should StructTrace judge this comparison?" description="Choose the authority of this result. Evidence checks cannot be bypassed by a quality metric." />
      <div className="gate-grid">{modes.map(({ mode, title, text, badge }) => <button key={mode} className={draft.gateMode === mode ? "selected" : ""} onClick={() => updateDraft({ gateMode: mode })}><span className="radio-dot">{draft.gateMode === mode && <span />}</span><div><h2>{title}{badge && <em>{badge}</em>}</h2><p>{text}</p></div></button>)}</div>
      <Card className="evidence-profile">
        <div className="panel-heading"><div><h2>{draft.gateMode === "release" ? "Conservative release profile" : "Balanced evidence profile"}</h2><p>Plain-language thresholds generated into the reproducible configuration.</p></div><ShieldCheck size={22} /></div>
        <label className="range-field"><span><strong>Required independent cases</strong><small>Current dataset contains {rows.toLocaleString()} rows.</small></span><input type="number" min="1" max="100000" value={draft.minCases} onChange={(event) => updateDraft({ minCases: Math.max(1, Number(event.target.value)) })} /></label>
        <div className="evidence-rules"><span><Check size={15} /> At least 99% fully evaluated</span><span><Check size={15} /> No repeated-trial conflicts</span><span><Check size={15} /> Candidate may regress by at most 0 pp</span>{draft.gateMode === "release" && <><span><Check size={15} /> Candidate deployment success at least 95%</span><span><Check size={15} /> Candidate strict JSON and schema validity 100%</span></>}</div>
      </Card>
      {!sufficient && draft.gateMode !== "advisory" && <InlineNotice tone="warning" title="Analysis available; authority disabled">This dataset has {rows} rows, below the configured minimum of {draft.minCases}. StructTrace will preserve the result as insufficient evidence instead of overstating it.</InlineNotice>}
      <WizardActions back={navigation.back} next={navigation.next} />
    </>
  );
}

function ReviewStep() {
  const { draft, updateDraft } = useWorkspace();
  const navigation = useStepNavigation(4);
  const matched = Math.min(draft.sources.dataset?.rows ?? 0, draft.sources.baseline?.rows ?? 0, draft.sources.candidate?.rows ?? 0);
  return (
    <>
      <PageHeader eyebrow="Step 5 of 6" title="Ready to compare" description="Review the contract. StructTrace will save the setup, source hashes, evidence, and an offline report." />
      <div className="review-grid">
        <Card className="review-main">
          <label><span>Comparison name</span><input value={draft.name} onChange={(event) => updateDraft({ name: event.target.value })} /></label>
          <div className="name-row"><label><span>Baseline</span><input value={draft.baselineName} onChange={(event) => updateDraft({ baselineName: event.target.value })} /></label><ArrowRight size={18} /><label><span>Candidate</span><input value={draft.candidateName} onChange={(event) => updateDraft({ candidateName: event.target.value })} /></label></div>
          <div className="review-facts"><div><strong>{matched}</strong><span>matched rows</span></div><div><strong>{draft.rules.filter((rule) => rule.enabled).length}</strong><span>correctness rules</span></div><div><strong>{draft.gateMode}</strong><span>decision mode</span></div><div><strong>local</strong><span>processing</span></div></div>
        </Card>
        <Card className="saved-artifacts"><h2>What will be saved</h2><ul><li><FileJson size={16} /><span><strong>structtrace.yaml</strong><small>Reproducible comparison definition</small></span></li><li><LockKeyhole size={16} /><span><strong>Source hashes</strong><small>Immutable input provenance</small></span></li><li><Braces size={16} /><span><strong>Paired case evidence</strong><small>Complete denominator and transitions</small></span></li><li><FolderLock size={16} /><span><strong>Local report</strong><small>Capability-protected case details</small></span></li></ul></Card>
      </div>
      {draft.gateMode !== "release" && <InlineNotice tone="warning" title={draft.gateMode === "regression" ? "Regression check is not release authorization" : "Advisory analysis cannot authorize deployment"}>The result banner will preserve this distinction even if every configured check passes.</InlineNotice>}
      <WizardActions back={navigation.back} next={navigation.next} nextLabel="Run comparison" />
    </>
  );
}

const runStages = ["Validate sources", "Parse structured outputs", "Validate schema", "Run correctness rules", "Build paired evidence", "Generate report", "Verify artifacts"];
function RunStep() {
  const { draft, setResult } = useWorkspace();
  const navigate = useNavigate();
  const [visibleStage, setVisibleStage] = useState(0);
  const request = useMemo<ComparisonRequest>(() => ({
    name: draft.name,
    baselineName: draft.baselineName,
    candidateName: draft.candidateName,
    files: {
      dataset: sourcePayload(draft.sources.dataset!),
      baseline: sourcePayload(draft.sources.baseline!),
      candidate: sourcePayload(draft.sources.candidate!),
      schema: draft.sources.schema ? sourcePayload(draft.sources.schema) : undefined,
    },
    mapping: draft.mapping,
    rules: draft.rules.filter((rule) => rule.enabled).map(({ pointer, kind, tolerance }) => ({ pointer, kind, tolerance })),
    gateMode: draft.gateMode,
    minCases: draft.minCases,
    financialInvariants: draft.financialInvariants,
  }), [draft]);
  const run = useMutation({ mutationFn: () => runComparison(request), onSuccess: (result) => {
    setVisibleStage(runStages.length);
    setResult(result);
    window.setTimeout(() => void navigate({ to: "/runs/$runId", params: { runId: result.runId } }), 450);
  }});
  useEffect(() => { if (run.isIdle) run.mutate(); }, [run]);
  useEffect(() => {
    if (!run.isPending) return;
    const timer = window.setInterval(() => setVisibleStage((current) => Math.min(current + 1, runStages.length - 1)), 500);
    return () => window.clearInterval(timer);
  }, [run.isPending]);
  return (
    <div className="run-screen">
      <PageHeader eyebrow="Step 6 of 6" title={`Comparing ${draft.sources.dataset?.rows ?? 0} cases`} description="Baseline and candidate outputs are loaded. The Rust engine is building verified paired evidence." />
      <Card className="run-progress">
        <div className="progress-head"><div><LoaderCircle className={run.isPending ? "spin" : ""} /><span><strong>{run.isError ? "Comparison stopped" : run.isSuccess ? "Artifacts verified" : "Comparison running"}</strong><small>{run.isError ? "No decision was produced." : "No network calls or telemetry."}</small></span></div><Status tone={run.isError ? "fail" : run.isSuccess ? "pass" : "working"} label={run.isError ? "Run error" : run.isSuccess ? "Complete" : "In progress"} /></div>
        <div className="progress-line"><span style={{ width: `${Math.max(8, (visibleStage / runStages.length) * 100)}%` }} /></div>
        <ol className="stage-list">{runStages.map((stage, index) => <li key={stage} className={index < visibleStage ? "complete" : index === visibleStage && run.isPending ? "active" : "pending"}><span>{index < visibleStage || run.isSuccess ? <Check size={15} /> : index === visibleStage && run.isPending ? <LoaderCircle size={15} className="spin" /> : index + 1}</span><strong>{stage}</strong><small>{index < visibleStage || run.isSuccess ? "Complete" : index === visibleStage && run.isPending ? "Working…" : "Waiting"}</small></li>)}</ol>
      </Card>
      {run.isError && <InlineNotice tone="danger" title="StructTrace could not complete this comparison"><p>{run.error.message}</p><Button variant="secondary" onClick={() => run.mutate()}>Try again</Button></InlineNotice>}
    </div>
  );
}

function sourcePayload(source: SourceArtifact) {
  return { name: source.name, format: source.format, content: source.content };
}
