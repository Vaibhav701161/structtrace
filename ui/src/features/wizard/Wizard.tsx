import { useMutation, useQuery } from "@tanstack/react-query";
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
  ShieldCheck,
  Upload,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { cancelComparisonJob, createComparisonJob, getComparisonJob, getFieldInventory, getRun, retryComparisonJob, stageSource } from "../../api/client";
import type { ComparisonRequest, FieldRule, GateMode, Mapping, SourceArtifact, SourceKind } from "../../api/types";
import { Button, Card, InlineNotice, PageHeader, Skeleton, Status, Stepper, WizardActions } from "../../design-system/components";
import { useWorkspace } from "../../state/workspace";
import { detectFormat, inferPointer, parseRows, pointerCandidates } from "../import/inspect";
import { exactJsonStringify } from "../../lib/lossless-json";

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
  const { draft, setSource, draftStatus, draftError, clearSensitiveDraft } = useWorkspace();
  const navigation = useStepNavigation(0);
  const ready = ["dataset", "baseline", "candidate"].every((kind) => draft.sources[kind as SourceKind]?.status === "ready");
  return (
    <>
      <PageHeader eyebrow="New comparison" title="What are you comparing?" description="Drop matched artifacts. Files are parsed and evaluated on this machine." />
      <div className="mode-switch" role="group" aria-label="Comparison source mode"><button className="selected"><FileJson size={18} /><span><strong>Recorded outputs</strong><small>Stable workflow</small></span><CheckCircle2 size={18} /></button></div>
      <div className="source-grid">
        {sourceDefinitions.map((definition) => <SourceDrop key={definition.kind} {...definition} source={draft.sources[definition.kind]} onSource={(source) => setSource(definition.kind, source)} />)}
      </div>
      <InlineNotice title="Local by design"><span>StructTrace reads these files through its loopback-only server. No account, telemetry, provider call, or external upload is involved.</span></InlineNotice>
      <div className="draft-controls"><Status tone={draftStatus === "error" ? "fail" : draftStatus === "saving" ? "working" : "neutral"} label={draftStatus === "error" ? "Draft not saved" : draftStatus === "saving" ? "Saving references" : "Source references saved"} /><Button variant="ghost" onClick={() => void clearSensitiveDraft()}>Clear active draft</Button></div>
      {draftError && <InlineNotice tone="danger" title="Draft persistence failed">{draftError}</InlineNotice>}
      <WizardActions next={navigation.next} nextLabel="Continue to field mapping" disabled={!ready} />
    </>
  );
}

function SourceDrop({ kind, title, description, required, source, onSource }: {
  kind: SourceKind; title: string; description: string; required: boolean; source?: SourceArtifact; onSource: (source: SourceArtifact) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const read = (file: File) => {
    void file.arrayBuffer().then((bytes) => {
      const content = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      const pending: SourceArtifact = { kind, name: file.name, format: kind === "schema" ? "json" : detectFormat(file.name, content), content, bytes: new Blob([content]).size, rows: 0, status: "staging", message: "Strict validation and hashing on the local server…", sourceId: "", hash: "" };
      onSource(pending);
      void stageSource(kind, pending)
        .then((staged) => onSource({ ...pending, ...staged, status: "ready", message: undefined }))
        .catch((error: Error) => onSource({ ...pending, sourceId: "", hash: "", status: "error", message: error.message }));
    }).catch(() => onSource({ kind, name: file.name, format: "jsonl", content: "", bytes: file.size, rows: 0, status: "error", message: "The file is not valid UTF-8 and was refused before parsing.", sourceId: "", hash: "" }));
  };
  return (
    <article className={`source-card ${source?.status === "ready" ? "source-ready" : ""} ${source?.status === "error" ? "source-error" : ""}`}>
      <div className="source-title"><div className="file-icon">{kind === "schema" ? <Braces /> : <FileSpreadsheet />}</div><div><h2>{title} {!required && <span>Optional</span>}</h2><p>{description}</p></div></div>
      {source ? (
        <div className="file-summary">
          <div><strong>{source.name}</strong><small>{source.status === "ready" ? `${source.rows.toLocaleString()} ${source.rows === 1 ? "record" : "records"} · ${source.format.toUpperCase()} · ${formatBytes(source.bytes)}` : source.message}</small></div>
          <Status tone={source.status === "ready" ? "pass" : source.status === "staging" ? "working" : "fail"} label={source.status === "ready" ? "Ready" : source.status === "staging" ? "Staging" : "Needs attention"} />
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
  const dataRows = useMemo(() => dataset.preview ?? parseRows(dataset.content, dataset.format), [dataset]);
  const baselineRows = useMemo(() => baseline.preview ?? parseRows(baseline.content, baseline.format), [baseline]);
  const candidateRows = useMemo(() => candidate.preview ?? parseRows(candidate.content, candidate.format), [candidate]);
  const dataPointers = useMemo(() => pointerCandidates(dataRows), [dataRows]);
  const baselinePointers = useMemo(() => pointerCandidates(baselineRows), [baselineRows]);
  const candidatePointers = useMemo(() => pointerCandidates(candidateRows), [candidateRows]);
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    if (initialized) return;
    updateDraft({ mapping: {
      ...draft.mapping,
      datasetId: inferPointer(dataPointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.datasetId),
      datasetInput: inferPointer(dataPointers, ["/input", "/document", "/payload", "/text"], draft.mapping.datasetInput),
      datasetExpected: inferPointer(dataPointers, ["/expected", "/ground_truth", "/reference", "/target"], draft.mapping.datasetExpected),
      baselineId: inferPointer(baselinePointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.baselineId),
      baselineOutput: inferPointer(baselinePointers, ["/output", "/parsed_output", "/raw_output", "/result", "/response", "/prediction"], draft.mapping.baselineOutput),
      candidateId: inferPointer(candidatePointers, ["/id", "/case_id", "/document_id", "/invoice_id"], draft.mapping.candidateId),
      candidateOutput: inferPointer(candidatePointers, ["/output", "/parsed_output", "/raw_output", "/result", "/response", "/prediction"], draft.mapping.candidateOutput),
      baselineStatus: inferOptionalPointer(baselinePointers, ["/status"], draft.mapping.baselineStatus), baselineError: inferOptionalPointer(baselinePointers, ["/error"], draft.mapping.baselineError), baselineLatency: inferOptionalPointer(baselinePointers, ["/latency_ms", "/latency"], draft.mapping.baselineLatency), baselineUsage: inferOptionalPointer(baselinePointers, ["/usage", "/tokens"], draft.mapping.baselineUsage), baselineCost: inferOptionalPointer(baselinePointers, ["/cost"], draft.mapping.baselineCost), baselineMetadata: inferOptionalPointer(baselinePointers, ["/metadata", "/provider_metadata"], draft.mapping.baselineMetadata),
      candidateStatus: inferOptionalPointer(candidatePointers, ["/status"], draft.mapping.candidateStatus), candidateError: inferOptionalPointer(candidatePointers, ["/error"], draft.mapping.candidateError), candidateLatency: inferOptionalPointer(candidatePointers, ["/latency_ms", "/latency"], draft.mapping.candidateLatency), candidateUsage: inferOptionalPointer(candidatePointers, ["/usage", "/tokens"], draft.mapping.candidateUsage), candidateCost: inferOptionalPointer(candidatePointers, ["/cost"], draft.mapping.candidateCost), candidateMetadata: inferOptionalPointer(candidatePointers, ["/metadata", "/provider_metadata"], draft.mapping.candidateMetadata),
    }});
    setInitialized(true);
  }, [baselinePointers, candidatePointers, dataPointers, draft.mapping, initialized, updateDraft]);
  const inventory = useQuery({
    queryKey: ["mapping-inventory", dataset.sourceId, baseline.sourceId, candidate.sourceId, draft.sources.schema?.sourceId, draft.mapping],
    enabled: initialized,
    queryFn: () => getFieldInventory({
      dataset: { sourceId: dataset.sourceId }, baseline: { sourceId: baseline.sourceId }, candidate: { sourceId: candidate.sourceId },
      schema: draft.sources.schema ? { sourceId: draft.sources.schema.sourceId } : undefined,
      datasetOutput: draft.mapping.datasetExpected, baselineOutput: draft.mapping.baselineOutput, candidateOutput: draft.mapping.candidateOutput,
      datasetId: draft.mapping.datasetId, baselineId: draft.mapping.baselineId, candidateId: draft.mapping.candidateId,
    }),
  });
  const mappingAudit = inventory.data?.mapping;
  const duplicates = mappingAudit?.duplicateDatasetIds ?? [];
  const matched = mappingAudit?.matched ?? 0;
  const missingCandidate = mappingAudit?.missingCandidate ?? 0;
  const leakage = draft.mapping.datasetInput === draft.mapping.datasetExpected;
  const invalidIds = (mappingAudit?.invalidDatasetIds ?? 1) + (mappingAudit?.invalidBaselineIds ?? 0) + (mappingAudit?.invalidCandidateIds ?? 0);
  const valid = Boolean(mappingAudit) && matched > 0 && duplicates.length === 0 && invalidIds === 0 && !leakage;

  const set = (key: keyof typeof draft.mapping, value: string) => updateDraft({ mapping: { ...draft.mapping, [key]: value } });
  return (
    <>
      <PageHeader eyebrow="Step 2 of 6" title="Confirm how your files are structured" description="StructTrace suggests mappings from a server-parsed sample. Complete field coverage is analyzed in the next step." />
      <div className="mapping-grid">
        <MappingCard title="Golden data" source={dataset} rows={dataRows} options={dataPointers} fields={[
          ["Case ID", "datasetId", draft.mapping.datasetId], ["Input", "datasetInput", draft.mapping.datasetInput], ["Expected output", "datasetExpected", draft.mapping.datasetExpected],
        ]} set={set} />
        <MappingCard title="Baseline" source={baseline} rows={baselineRows} options={baselinePointers} fields={[["Case ID", "baselineId", draft.mapping.baselineId], ["Output", "baselineOutput", draft.mapping.baselineOutput]]} set={set} />
        <MappingCard title="Candidate" source={candidate} rows={candidateRows} options={candidatePointers} fields={[["Case ID", "candidateId", draft.mapping.candidateId], ["Output", "candidateOutput", draft.mapping.candidateOutput]]} set={set} />
      </div>
      <Card className="envelope-mapping"><div className="panel-heading"><div><h2>Optional output-envelope fields</h2><p>Preserve provider status, errors, latency, token usage, cost, and metadata in immutable evidence. Leave unavailable fields blank.</p></div></div><div className="envelope-grid"><EnvelopeFields prefix="baseline" title="Baseline envelope" mapping={draft.mapping} options={baselinePointers} set={set} /><EnvelopeFields prefix="candidate" title="Candidate envelope" mapping={draft.mapping} options={candidatePointers} set={set} /></div></Card>
      <Card className="coverage-card">
        <div><strong>{matched.toLocaleString()}</strong><span>matched cases</span></div>
        <div className={duplicates.length ? "metric-bad" : ""}><strong>{duplicates.length}</strong><span>duplicate IDs</span></div>
        <div><strong>{mappingAudit?.missingBaseline ?? 0}</strong><span>missing baseline</span></div>
        <div className={missingCandidate ? "metric-warn" : ""}><strong>{missingCandidate}</strong><span>missing candidate</span></div>
      </Card>
      {missingCandidate > 0 && <InlineNotice tone="warning" title="Missing candidate outputs stay in the denominator">{missingCandidate} cases will count as candidate failures. They will not disappear from the result.</InlineNotice>}
      {leakage && <InlineNotice tone="danger" title="Reference leakage risk">Input and expected output resolve to the same path. Choose distinct fields before continuing.</InlineNotice>}
      {duplicates.length > 0 && <InlineNotice tone="danger" title="Duplicate case IDs">Resolve duplicate IDs before running. First affected ID: <code>{duplicates[0]}</code>.</InlineNotice>}
      {invalidIds > 0 && <InlineNotice tone="danger" title="Case IDs must be non-empty strings">The full-source audit found {invalidIds} missing, empty, or non-string ID values across the three sources.</InlineNotice>}
      {inventory.error && <InlineNotice tone="danger" title="Complete mapping audit failed">{inventory.error.message}</InlineNotice>}
      <WizardActions back={navigation.back} next={navigation.next} nextLabel="Looks right" disabled={!valid} />
    </>
  );
}

type MappingKey = keyof Mapping;
function MappingCard({ title, source, rows, options, fields, set }: {
  title: string; source: SourceArtifact; rows: unknown[]; options: string[]; fields: Array<[string, MappingKey, string]>; set: (key: MappingKey, value: string) => void;
}) {
  return (
    <Card className="mapping-card">
      <div className="mapping-title"><div><h2>{title}</h2><p>{source.name}</p></div><Status tone="pass" label={`${source.rows} total rows`} /></div>
      {fields.map(([label, key, value]) => <label className="field-select" key={key}><span>{label}<small>Suggested · verify</small></span><div><select value={value} onChange={(event) => set(key, event.target.value)}>{options.map((pointer) => <option key={pointer}>{pointer}</option>)}</select><ChevronDown size={15} /></div></label>)}
      <div className="sample-json"><small>Server-parsed sample record</small><pre>{exactJsonStringify(rows[0], 2).slice(0, 620)}</pre></div>
    </Card>
  );
}

function inferOptionalPointer(options: string[], candidates: string[], previous?: string) { return candidates.find((pointer) => options.includes(pointer)) ?? (previous && options.includes(previous) ? previous : ""); }
function EnvelopeFields({ prefix, title, mapping, options, set }: { prefix: "baseline" | "candidate"; title: string; mapping: Mapping; options: string[]; set: (key: MappingKey, value: string) => void }) {
  const suffixes = [["Status", "Status"], ["Error", "Error"], ["Latency", "Latency"], ["Token usage", "Usage"], ["Cost", "Cost"], ["Provider metadata", "Metadata"]] as const;
  return <fieldset><legend>{title}</legend>{suffixes.map(([label, suffix]) => { const key = `${prefix}${suffix}` as MappingKey; return <label key={key}><span>{label}</span><select value={mapping[key] ?? ""} onChange={(event) => set(key, event.target.value)}><option value="">Not available</option>{options.map((pointer) => <option key={pointer}>{pointer}</option>)}</select></label>; })}</fieldset>;
}

function CorrectnessStep() {
  const { draft, setRules } = useWorkspace();
  const navigation = useStepNavigation(2);
  const inventory = useQuery({
    queryKey: ["field-inventory", draft.sources.dataset?.sourceId, draft.sources.baseline?.sourceId, draft.sources.candidate?.sourceId, draft.sources.schema?.sourceId, draft.mapping.datasetExpected, draft.mapping.baselineOutput, draft.mapping.candidateOutput],
    queryFn: () => getFieldInventory({
      dataset: { sourceId: draft.sources.dataset!.sourceId }, baseline: { sourceId: draft.sources.baseline!.sourceId }, candidate: { sourceId: draft.sources.candidate!.sourceId },
      schema: draft.sources.schema ? { sourceId: draft.sources.schema.sourceId } : undefined,
      datasetOutput: draft.mapping.datasetExpected, baselineOutput: draft.mapping.baselineOutput, candidateOutput: draft.mapping.candidateOutput,
      datasetId: draft.mapping.datasetId, baselineId: draft.mapping.baselineId, candidateId: draft.mapping.candidateId,
    }),
  });
  const discovered = useMemo(() => (inventory.data?.fields ?? []).filter((field) => !field.pointer.includes("/*/")).map((field) => {
    const children = (inventory.data?.fields ?? []).filter((candidate) => candidate.pointer.startsWith(`${field.pointer}/*/`));
    const identity = children.find((candidate) => /\/(id|sku|product_code|code)$/i.test(candidate.pointer));
    return {
      pointer: field.pointer, kind: field.suggestedRule, enabled: false,
      expectedCoverage: field.expectedCoverage, baselineCoverage: field.baselineCoverage,
      candidateCoverage: field.candidateCoverage, observedType: field.observedType,
      keys: field.suggestedRule === "keyed_array" ? identity?.pointer.slice(field.pointer.length + 2) : undefined,
      keyFields: field.suggestedRule === "keyed_array" ? (identity ? [identity.pointer.slice(field.pointer.length + 2)] : []) : undefined,
      arrayFields: field.suggestedRule === "keyed_array" ? children.filter((item) => item !== identity).map((item) => ({ pointer: item.pointer.slice(field.pointer.length + 2), kind: item.suggestedRule === "keyed_array" || item.suggestedRule === "decimal_exact" ? "exact" as const : item.suggestedRule })) : undefined,
    } satisfies FieldRule;
  }), [inventory.data]);
  const [initialized, setInitialized] = useState(false);
  useEffect(() => {
    if (initialized || !inventory.data) return;
    setRules(draft.rules.length ? draft.rules : discovered);
    setInitialized(true);
  }, [discovered, draft.rules, initialized, inventory.data, setRules]);
  const update = (pointer: string, next: Partial<FieldRule>) => setRules(draft.rules.map((rule) => rule.pointer === pointer ? { ...rule, ...next } : rule));
  const enabled = draft.rules.filter((rule) => rule.enabled);
  const keyedRules = enabled.filter((rule) => rule.kind === "keyed_array");
  return (
    <>
      <PageHeader eyebrow="Step 3 of 6" title="What does correct mean for your application?" description="Schema validity is never treated as task correctness. Every bounded source row and the caller schema are analyzed before suggestions appear." />
      {inventory.isLoading && <Card><Skeleton /><Skeleton width="82%" /><Skeleton width="64%" /></Card>}
      {inventory.error && <InlineNotice tone="danger" title="Full-source field analysis failed">{inventory.error.message}</InlineNotice>}
      {inventory.data && <InlineNotice tone="success" title="Complete source inventory verified">Rust analyzed all {inventory.data.datasetRows.toLocaleString()} expected, {inventory.data.baselineRows.toLocaleString()} baseline, and {inventory.data.candidateRows.toLocaleString()} candidate rows. Preview rows are display-only.</InlineNotice>}
      <div className="correctness-summary"><div><CircleDot size={18} /><span><strong>{enabled.length} fields define correctness</strong><small>A case passes only when every selected rule passes.</small></span></div><Status tone={enabled.length ? "info" : "warning"} label={enabled.length ? "All selected rules must pass" : "Select at least one field"} /></div>
      <Card className="rules-card">
        <table className="rules-table">
          <caption>Discovered semantic fields and comparison rules</caption>
          <thead><tr><th scope="col">Use</th><th scope="col">Field</th><th scope="col">Type</th><th scope="col">Expected</th><th scope="col">Baseline</th><th scope="col">Candidate</th><th scope="col">Comparison</th></tr></thead>
          <tbody>{draft.rules.map((rule) => {
            const missing = rule.candidateCoverage < rule.expectedCoverage;
            return <tr key={rule.pointer} className={!rule.enabled ? "disabled-row" : missing ? "missing-row" : ""}><td><input type="checkbox" checked={rule.enabled} onChange={(event) => update(rule.pointer, { enabled: event.target.checked })} aria-label={`Use ${rule.pointer}`} /></td><td><code>{rule.pointer}</code>{missing && <small className="coverage-warning"><AlertTriangle size={13} /> Candidate omission</small>}</td><td>{rule.observedType}</td><td>{formatCoverage(rule.expectedCoverage)}</td><td>{formatCoverage(rule.baselineCoverage)}</td><td>{formatCoverage(rule.candidateCoverage)}</td><td><select value={rule.kind} onChange={(event) => update(rule.pointer, { kind: event.target.value as FieldRule["kind"] })} aria-label={`Comparison for ${rule.pointer}`}><option value="exact">Exact value</option><option value="required_fields">Required presence</option><option value="normalized_string">Normalized text</option><option value="canonical_date">Calendar date</option><option value="exact_integer">Exact integer</option><option value="decimal_exact">Exact decimal</option><option value="decimal_tolerance">Decimal tolerance</option><option value="keyed_array">Keyed array</option></select>{rule.kind === "normalized_string" && <label className="inline-option"><input type="checkbox" checked={rule.caseInsensitive ?? true} onChange={(event) => update(rule.pointer, { caseInsensitive: event.target.checked })} /> Ignore case</label>}{rule.kind === "canonical_date" && <select value={rule.formats ?? "iso"} onChange={(event) => update(rule.pointer, { formats: event.target.value })} aria-label={`Accepted date formats for ${rule.pointer}`}><option value="iso">ISO only</option><option value="iso,dmy_slash">ISO + DMY slash</option><option value="iso,mdy_slash">ISO + MDY slash</option></select>}{rule.kind === "decimal_tolerance" && <input value={rule.tolerance ?? "0.01"} onChange={(event) => update(rule.pointer, { tolerance: event.target.value })} aria-label={`Absolute tolerance for ${rule.pointer}`} />}{rule.kind === "keyed_array" && <small className="configured-label">Configure item matching below</small>}</td></tr>;
          })}</tbody>
        </table>
        {!draft.rules.length && <div className="table-empty"><AlertTriangle size={20} /><strong>No semantic fields were discovered</strong><p>Check the expected and output mappings on the previous step.</p></div>}
      </Card>
      {keyedRules.map((rule) => <KeyedArrayBuilder key={rule.pointer} rule={rule} update={(next) => update(rule.pointer, next)} />)}
      {enabled.length > 0 && <Card className="rule-preview"><div className="panel-heading"><div><h2>Rule behavior preview</h2><p>These examples describe the deterministic result states before the configuration is saved.</p></div><Status tone="info" label={`${enabled.length} active`} /></div><div className="rule-preview-grid">{enabled.slice(0, 6).map((rule) => <div key={rule.pointer}><code>{rule.pointer}</code><span><b className="preview-pass">Pass</b>{passExample(rule)}</span><span><b className="preview-fail">Regression</b>{failureExample(rule)}</span><span><b className="preview-error">Unscored/error</b>Missing expected reference or evaluator failure never counts as a pass.</span></div>)}</div></Card>}
      <InlineNotice title="Why suggestions appear">Suggestions come only from observed JSON types and field names. They are never silently activated or changed after you review them.</InlineNotice>
      <WizardActions back={navigation.back} next={navigation.next} disabled={!enabled.length} />
    </>
  );
}
function passExample(rule: FieldRule) { return rule.kind === "normalized_string" ? "Whitespace and configured case normalization agree." : rule.kind === "canonical_date" ? "Both values resolve to the same declared calendar date." : rule.kind === "decimal_tolerance" ? `Absolute difference is at most ${rule.tolerance ?? "0.01"}.` : rule.kind === "keyed_array" ? "Every keyed item pairs and all selected item fields pass." : "Expected and output values satisfy the selected exact policy."; }
function failureExample(rule: FieldRule) { return rule.kind === "keyed_array" ? "An item is missing, duplicated, or a compared field differs." : rule.kind === "required_fields" ? "The output omits the required field." : "The candidate value violates the selected field policy."; }

function KeyedArrayBuilder({ rule, update }: { rule: FieldRule; update: (next: Partial<FieldRule>) => void }) {
  const fields = rule.arrayFields ?? [];
  const keys = new Set(rule.keyFields ?? (rule.keys ? rule.keys.split(",").filter(Boolean) : []));
  const allPointers = [...new Set([...keys, ...fields.map((field) => field.pointer)])].sort();
  const comparison = (pointer: string) => fields.find((field) => field.pointer === pointer);
  const setRole = (pointer: string, role: string) => {
    const nextKeys = new Set(keys);
    let nextFields = fields.filter((field) => field.pointer !== pointer);
    if (role === "key") nextKeys.add(pointer); else nextKeys.delete(pointer);
    if (role !== "key" && role !== "ignore") {
      nextFields = [...nextFields, { pointer, kind: role as NonNullable<FieldRule["arrayFields"]>[number]["kind"], tolerance: role === "decimal_tolerance" ? "0.01" : undefined }].sort((left, right) => left.pointer.localeCompare(right.pointer));
    }
    update({ keyFields: [...nextKeys].sort(), arrayFields: nextFields });
  };
  return <Card className="array-builder">
    <div className="panel-heading"><div><h2>Match items in <code>{rule.pointer}</code></h2><p>Choose stable identity fields, then define how each remaining item field is compared. Array order is ignored.</p></div><Status tone={keys.size ? "pass" : "warning"} label={keys.size ? `${keys.size} match ${keys.size === 1 ? "key" : "keys"}` : "Match key required"} /></div>
    <div className="array-field-grid" role="group" aria-label={`Item rules for ${rule.pointer}`}>
      <div className="array-field-head"><span>Item field</span><span>Role and comparison</span><span>Policy</span></div>
      {allPointers.map((pointer) => {
        const field = comparison(pointer);
        const role = keys.has(pointer) ? "key" : field?.kind ?? "ignore";
        return <div className="array-field-row" key={pointer}><code>{pointer}</code><select value={role} onChange={(event) => setRole(pointer, event.target.value)} aria-label={`Role for ${pointer}`}><option value="key">Match key</option><option value="exact">Exact value</option><option value="normalized_string">Normalized text</option><option value="canonical_date">Calendar date</option><option value="exact_integer">Exact integer</option><option value="decimal_tolerance">Decimal tolerance</option><option value="ignore">Do not compare</option></select><span>{role === "key" ? "Pairs the same item across variants" : role === "ignore" ? "Excluded from correctness" : role === "decimal_tolerance" ? <label>± <input value={field?.tolerance ?? "0.01"} onChange={(event) => update({ arrayFields: fields.map((item) => item.pointer === pointer ? { ...item, tolerance: event.target.value } : item) })} aria-label={`Tolerance for ${pointer}`} /></label> : "Deterministic evaluator"}</span></div>;
      })}
    </div>
    {!keys.size && <InlineNotice tone="danger" title="Choose at least one stable item key">A keyed array cannot pair items safely without an identity such as SKU, product code, or ID.</InlineNotice>}
  </Card>;
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
  const releaseAvailable = Boolean(draft.sources.schema);
  const financialReady = ["/line_items", "/subtotal", "/tax", "/total"].every((pointer) => draft.rules.some((rule) => rule.pointer === pointer));
  return (
    <>
      <PageHeader eyebrow="Step 4 of 6" title="How should StructTrace judge this comparison?" description="Choose the authority of this result. Evidence checks cannot be bypassed by a quality metric." />
      <div className="gate-grid">{modes.map(({ mode, title, text, badge }) => <button key={mode} disabled={mode === "release" && !releaseAvailable} className={draft.gateMode === mode ? "selected" : ""} onClick={() => updateDraft({ gateMode: mode, minCases: mode === "release" ? Math.max(100, draft.minCases) : draft.minCases })}><span className="radio-dot">{draft.gateMode === mode && <span />}</span><div><h2>{title}{badge && <em>{badge}</em>}</h2><p>{mode === "release" && !releaseAvailable ? "Unavailable until you provide the caller-facing JSON Schema." : text}</p></div></button>)}</div>
      <Card className="evidence-profile">
        <div className="panel-heading"><div><h2>{draft.gateMode === "release" ? "Conservative release profile" : "Balanced evidence profile"}</h2><p>Plain-language thresholds generated into the reproducible configuration.</p></div><ShieldCheck size={22} /></div>
        <label className="range-field"><span><strong>Required independent cases</strong><small>Current dataset contains {rows.toLocaleString()} rows.{draft.gateMode === "release" ? " Release authority requires at least 100." : ""}</small></span><input type="number" min={draft.gateMode === "release" ? "100" : "1"} max="100000" value={draft.minCases} onChange={(event) => updateDraft({ minCases: Math.max(draft.gateMode === "release" ? 100 : 1, Number(event.target.value)) })} /></label>
        <label className={`setting-row ${!financialReady ? "disabled-row" : ""}`}><span><strong>Invoice financial invariants</strong><small>{financialReady ? "Cross-checks line amounts, subtotal, tax, and total with absolute tolerance 0.01." : "Unavailable because /line_items, /subtotal, /tax, and /total were not all discovered."}</small></span><input type="checkbox" disabled={!financialReady} checked={draft.financialInvariants && financialReady} onChange={(event) => updateDraft({ financialInvariants: event.target.checked })} /></label>
        <div className="evidence-rules"><span><Check size={15} /> At least 99% fully evaluated</span><span><Check size={15} /> No repeated-trial conflicts</span><span><Check size={15} /> Candidate may regress by at most 0 pp</span>{draft.gateMode === "release" && <><span><Check size={15} /> Candidate deployment success at least 95%</span><span><Check size={15} /> Candidate strict JSON and schema validity 100%</span></>}</div>
      </Card>
      {!sufficient && draft.gateMode !== "advisory" && <InlineNotice tone="warning" title="Analysis available; authority disabled">This dataset has {rows} rows, below the configured minimum of {draft.minCases}. StructTrace will preserve the result as insufficient evidence instead of overstating it.</InlineNotice>}
      {!releaseAvailable && <InlineNotice tone="warning" title="Release mode requires the real contract">Without a caller-supplied JSON Schema, inferred shape checks remain diagnostic and can be used only in Advisory or Regression mode.</InlineNotice>}
      <WizardActions back={navigation.back} next={navigation.next} disabled={draft.gateMode === "release" && !releaseAvailable} />
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

function RunStep() {
  const { draft, setResult, updateDraft, updateDraftAndPersist, draftStatus } = useWorkspace();
  const navigate = useNavigate();
  const request = useMemo<ComparisonRequest | null>(() => {
    if (!draft.sources.dataset || !draft.sources.baseline || !draft.sources.candidate) return null;
    return ({
    projectId: draft.projectId,
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
    rules: draft.rules.filter((rule) => rule.enabled).map(({ pointer, kind, tolerance, keys, fields, keyFields, arrayFields, formats, caseInsensitive }) => ({
      pointer, kind, tolerance,
      keys: keyFields?.join(",") || keys,
      fields: arrayFields?.map((field) => `${field.pointer}:${field.kind === "decimal_tolerance" ? `decimal_tolerance:${field.tolerance ?? "0.01"}` : field.kind}`).join(",") || fields,
      formats, caseInsensitive,
    })),
    gateMode: draft.gateMode,
    minCases: draft.minCases,
    financialInvariants: draft.financialInvariants,
    });
  }, [draft]);
  const create = useMutation({
    mutationFn: () => request ? createComparisonJob(request) : Promise.reject(new Error("The saved comparison sources are not available.")),
    onSuccess: async (createdJob) => { await updateDraftAndPersist({ activeJobId: createdJob.jobId }); },
  });
  const job = useQuery({ queryKey: ["comparison-job", draft.activeJobId], queryFn: () => getComparisonJob(draft.activeJobId!), enabled: Boolean(draft.activeJobId), refetchInterval: (query) => ["queued", "waiting_for_executor", "running"].includes(query.state.data?.status ?? "") ? 300 : false });
  const cancel = useMutation({ mutationFn: () => cancelComparisonJob(draft.activeJobId!), onSuccess: () => void job.refetch() });
  const retry = useMutation({ mutationFn: () => retryComparisonJob(draft.activeJobId!), onSuccess: async (next) => { await updateDraftAndPersist({ activeJobId: next.jobId }); } });
  useEffect(() => { if (request && !draft.activeJobId && create.isIdle) create.mutate(); }, [create, draft.activeJobId, request]);
  useEffect(() => {
    const runId = job.data?.status === "complete" ? job.data.runId : null;
    if (!runId) return;
    void getRun(runId).then((result) => { setResult(result); updateDraft({ activeJobId: undefined }); void navigate({ to: "/runs/$runId", params: { runId } }); });
  }, [job.data?.runId, job.data?.status, navigate, setResult, updateDraft]);
  const progress = job.data ? Math.round(job.data.completed / Math.max(1, job.data.total) * 100) : 0;
  const terminalError = create.error ?? job.error;
  const active = !job.data || ["queued", "waiting_for_executor", "running"].includes(job.data.status);
  if (!request && draftStatus === "loading") return <div className="run-screen"><Card className="run-progress"><Skeleton /><Skeleton width="70%" /></Card></div>;
  if (!request) return <div className="run-screen"><InlineNotice tone="danger" title="Comparison sources are unavailable">Return to the first step and attach the dataset, baseline, and candidate sources before running.</InlineNotice></div>;
  return (
    <div className="run-screen">
      <PageHeader eyebrow="Step 6 of 6" title={`Comparing ${draft.sources.dataset?.rows ?? 0} cases`} description="Baseline and candidate outputs are loaded. The Rust engine is building verified paired evidence." />
      <Card className="run-progress">
        <div className="progress-head"><div><LoaderCircle className={active ? "spin" : ""} /><span><strong>{job.data?.status === "failed" ? "Comparison stopped" : job.data?.status === "cancelled" ? "Comparison cancelled" : job.data?.status === "interrupted" ? "Comparison interrupted" : job.data?.status === "complete" ? "Artifacts verified" : job.data?.status === "waiting_for_executor" ? "Waiting for local executor" : "Comparison running"}</strong><small>{job.data ? stageLabel(job.data.stage) : "Starting an isolated local job…"}</small></span></div><Status tone={job.data?.status === "complete" ? "pass" : ["failed", "cancelled", "interrupted"].includes(job.data?.status ?? "") ? "fail" : "working"} label={job.data?.status === "waiting_for_executor" ? "Queued" : job.data?.status ?? "Starting"} /></div>
        <div className="progress-line" aria-label={`${progress}% complete`}><span style={{ width: `${progress}%` }} /></div>
        {job.data && <div className="job-progress-meta"><span><strong>{job.data.completed.toLocaleString()}</strong> / {job.data.total.toLocaleString()} work units</span><code>{job.data.jobId}</code></div>}
        {job.data?.events.length ? <ol className="stage-list">{job.data.events.map((event, index) => <li className={index === job.data!.events.length - 1 && active ? "active" : "complete"} key={`${event.stage}:${event.at}:${index}`}><span>{index + 1}</span><strong>{stageLabel(event.stage)}</strong><small>{new Date(event.at * 1000).toLocaleTimeString()}</small></li>)}</ol> : null}
        {active && draft.activeJobId && <div className="job-actions"><Button variant="secondary" icon={XCircle} onClick={() => cancel.mutate()} disabled={cancel.isPending}>{cancel.isPending ? "Requesting cancellation…" : "Cancel safely"}</Button><small>Reload-safe. Cancellation occurs at the next engine checkpoint.</small></div>}
      </Card>
      {terminalError && <InlineNotice tone="danger" title="StructTrace could not start this comparison"><p>{terminalError.message}</p><Button variant="secondary" onClick={() => create.mutate()}>Try again</Button></InlineNotice>}
      {job.data && ["failed", "cancelled", "interrupted"].includes(job.data.status) && <InlineNotice tone="danger" title="No decision was produced"><p>{job.data.message}</p><p>Retry reuses the retained source references and restarts the complete comparison. It is not a partial resume.</p><Button variant="secondary" onClick={() => retry.mutate()} disabled={retry.isPending}>{retry.isPending ? "Retrying…" : "Retry from retained sources"}</Button></InlineNotice>}
    </div>
  );
}

function stageLabel(stage: string) { return ({ queued: "Waiting for the local engine", preparing_project: "Preparing the reproducible project", normalizing_sources: "Strictly validating and normalizing sources", validating_configuration: "Validating configuration and release policy", loading_inputs: "Loading hash-bound inputs", evaluating_cases: "Evaluating matched cases", analyzing_evidence: "Computing paired evidence", writing_artifacts: "Writing and verifying immutable artifacts", complete: "Comparison complete", cancelling: "Stopping at a safe boundary", cancelled: "Cancelled safely", interrupted: "Interrupted", failed: "Comparison failed" } as Record<string, string>)[stage] ?? stage.replaceAll("_", " "); }

function sourcePayload(source: SourceArtifact) {
  return { sourceId: source.sourceId };
}
