//! Offline report generation and loopback-only serving.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use minijinja::{AutoEscape, Environment};
use serde::Serialize;
use serde_json::Value;
use structtrace_core::{
    artifact::{PairedCaseRecord, RunManifest, RunStatus, RunSummary},
    config::Config,
    privacy::{REDACTION_MARKER, redact_matching_values, selected_values},
};
use tempfile::NamedTempFile;

/// The report asset format is versioned independently from stored scores.
pub const REPORT_FORMAT_VERSION: u32 = 1;

/// Generated report location.
#[derive(Debug, Clone)]
pub struct GeneratedReport {
    /// Main offline page.
    pub index_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct ReportView {
    project_name: String,
    run_id: String,
    gate_label: String,
    gate_class: String,
    default_filter: String,
    difference: String,
    interval: String,
    baseline_primary: MetricView,
    candidate_primary: MetricView,
    structural_rows: Vec<ComparisonRow>,
    transition: TransitionView,
    research_studies: Vec<ResearchStudyView>,
    gate_rules: Vec<GateRuleView>,
    evaluator_rows: Vec<ComparisonRow>,
    operational_rows: Vec<OperationalRow>,
    hotspots: Vec<HotspotView>,
    cases: Vec<CaseView>,
    manifest_rows: Vec<(String, String)>,
}

#[derive(Debug, Serialize)]
struct MetricView {
    count: usize,
    total: usize,
    percent: String,
}

#[derive(Debug, Serialize)]
struct ComparisonRow {
    label: String,
    baseline: MetricView,
    candidate: MetricView,
}

#[derive(Debug, Serialize)]
struct OperationalRow {
    label: String,
    baseline: String,
    candidate: String,
}

#[derive(Debug, Serialize)]
struct TransitionView {
    both_pass: usize,
    baseline_only: usize,
    candidate_only: usize,
    both_fail: usize,
}

#[derive(Debug, Serialize)]
struct ResearchStudyView {
    label: String,
    baseline: MetricView,
    candidate: MetricView,
    candidate_only: usize,
    baseline_only: usize,
}

#[derive(Debug, Serialize)]
struct GateRuleView {
    name: String,
    state: String,
    class: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct HotspotView {
    pointer: String,
    regressions: usize,
    improvements: usize,
    failures: usize,
}

#[derive(Debug, Serialize)]
struct CaseView {
    id: String,
    transition: String,
    filters: String,
    input: String,
    expected: String,
    metadata: String,
    baseline_raw: String,
    candidate_raw: String,
    baseline_parsed: String,
    candidate_parsed: String,
    diffs: Vec<JsonDiffEntry>,
    baseline_schema_errors: String,
    candidate_schema_errors: String,
    baseline_evaluators: String,
    candidate_evaluators: String,
    baseline_metadata: String,
    candidate_metadata: String,
    baseline_execution: String,
    candidate_execution: String,
    baseline_latency: String,
    candidate_latency: String,
}

/// Structured JSON difference.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonDiffEntry {
    /// JSON Pointer path.
    pub path: String,
    /// `added`, `removed`, `value_changed`, `type_changed`, or `array_item_changed`.
    pub kind: String,
    /// Baseline value.
    pub baseline: String,
    /// Candidate value.
    pub candidate: String,
}

/// Generate the complete offline report from finalized portable artifacts.
pub fn generate(run_dir: &Path) -> anyhow::Result<GeneratedReport> {
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    anyhow::ensure!(
        manifest.status != RunStatus::Complete,
        "completed run reports are immutable; use the finalized report or export a copy"
    );
    let config: Config = read_json(&run_dir.join("inputs/configuration.json"))?;
    let cases: Vec<PairedCaseRecord> = read_jsonl(&run_dir.join("cases.jsonl"))?;
    let view = build_view(&summary, &manifest, &cases, &config)?;
    let mut environment = Environment::new();
    environment.set_auto_escape_callback(|_| AutoEscape::Html);
    environment.add_template("report.html", TEMPLATE)?;
    let html = environment.get_template("report.html")?.render(view)?;
    let report_dir = run_dir.join("report");
    std::fs::create_dir_all(&report_dir)?;
    let index_path = report_dir.join("index.html");
    atomic_write(&index_path, html.as_bytes())?;
    Ok(GeneratedReport { index_path })
}

/// Export the generated report as one self-contained HTML file.
pub fn export_single_file(run_dir: &Path, destination: &Path) -> anyhow::Result<()> {
    let generated = finalized_report(run_dir)?;
    let bytes = std::fs::read(&generated.index_path)?;
    atomic_write(destination, &bytes)
}

/// Serve a report on a random loopback-only port until interrupted.
pub async fn serve(run_dir: &Path, open_browser: bool) -> anyhow::Result<()> {
    let generated = finalized_report(run_dir)?;
    let directory = generated
        .index_path
        .parent()
        .context("generated report has no directory")?
        .to_owned();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let url = format!("http://127.0.0.1:{}/", address.port());
    println!("StructTrace report: {url}");
    if open_browser {
        open::that(&url).context("could not open the default browser")?;
    }
    let service =
        axum::Router::new().fallback_service(tower_http::services::ServeDir::new(directory));
    axum::serve(listener, service)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

/// Resolve an existing report without mutating a completed run.
pub fn finalized_report(run_dir: &Path) -> anyhow::Result<GeneratedReport> {
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    anyhow::ensure!(
        manifest.status == RunStatus::Complete,
        "run `{}` is {:?}; only complete runs have finalized reports",
        manifest.run_id,
        manifest.status
    );
    let index_path = run_dir.join("report/index.html");
    anyhow::ensure!(
        index_path.is_file(),
        "finalized report is missing at {}",
        index_path.display()
    );
    Ok(GeneratedReport { index_path })
}

/// Compute a JSON-aware recursive diff.
pub fn json_diff(baseline: &Value, candidate: &Value) -> Vec<JsonDiffEntry> {
    let mut entries = Vec::new();
    diff_value("", baseline, candidate, &mut entries);
    entries
}

fn diff_value(path: &str, baseline: &Value, candidate: &Value, entries: &mut Vec<JsonDiffEntry>) {
    match (baseline, candidate) {
        (Value::Object(left), Value::Object(right)) => {
            let mut keys = left.keys().chain(right.keys()).collect::<Vec<_>>();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child = format!("{}/{}", path, escape_pointer(key));
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => diff_value(&child, left, right, entries),
                    (Some(left), None) => {
                        entries.push(diff_entry(&child, "removed", left, &Value::Null))
                    }
                    (None, Some(right)) => {
                        entries.push(diff_entry(&child, "added", &Value::Null, right))
                    }
                    (None, None) => {}
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                let child = format!("{path}/{index}");
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) if left != right => {
                        let before = entries.len();
                        diff_value(&child, left, right, entries);
                        if entries.len() == before {
                            entries.push(diff_entry(&child, "array_item_changed", left, right));
                        }
                    }
                    (Some(left), None) => {
                        entries.push(diff_entry(&child, "removed", left, &Value::Null))
                    }
                    (None, Some(right)) => {
                        entries.push(diff_entry(&child, "added", &Value::Null, right))
                    }
                    _ => {}
                }
            }
        }
        _ if baseline == candidate => {}
        _ => {
            let kind = if value_type(baseline) == value_type(candidate) {
                "value_changed"
            } else {
                "type_changed"
            };
            entries.push(diff_entry(
                if path.is_empty() { "/" } else { path },
                kind,
                baseline,
                candidate,
            ));
        }
    }
}

fn diff_entry(path: &str, kind: &str, baseline: &Value, candidate: &Value) -> JsonDiffEntry {
    JsonDiffEntry {
        path: path.to_owned(),
        kind: kind.to_owned(),
        baseline: compact_json(baseline),
        candidate: compact_json(candidate),
    }
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn build_view(
    summary: &RunSummary,
    manifest: &RunManifest,
    records: &[PairedCaseRecord],
    config: &Config,
) -> anyhow::Result<ReportView> {
    let structural_rows = vec![
        comparison(
            "Strict JSON",
            summary.baseline.parse_valid,
            summary.candidate.parse_valid,
            summary.baseline.total,
        ),
        comparison(
            "Schema valid",
            summary.baseline.schema_valid,
            summary.candidate.schema_valid,
            summary.baseline.total,
        ),
        comparison(
            "Semantic or executable outcome",
            summary.baseline.primary_pass,
            summary.candidate.primary_pass,
            summary.baseline.total,
        ),
        comparison(
            "Valid but wrong",
            summary.baseline.valid_but_wrong,
            summary.candidate.valid_but_wrong,
            summary.baseline.total,
        ),
        comparison(
            "Adapter error or missing output",
            summary.baseline.errors,
            summary.candidate.errors,
            summary.baseline.total,
        ),
    ];
    let evaluator_rows = summary
        .evaluator_passes
        .iter()
        .map(|(id, counts)| {
            comparison(
                id,
                counts.baseline_pass,
                counts.candidate_pass,
                counts.total,
            )
        })
        .collect();
    let cases = records
        .iter()
        .map(|record| case_view(record, config))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(ReportView {
        project_name: config
            .report
            .title
            .clone()
            .unwrap_or_else(|| manifest.project_name.clone()),
        run_id: summary.run_id.clone(),
        gate_label: if summary.gate.passed {
            "PASSED"
        } else {
            "FAILED"
        }
        .to_owned(),
        gate_class: if summary.gate.passed { "pass" } else { "fail" }.to_owned(),
        default_filter: match config.report.default_case_filter.as_str() {
            "baseline_only_pass"
            | "candidate_only_pass"
            | "both_fail"
            | "valid_but_wrong"
            | "parse_failure"
            | "schema_failure"
            | "adapter_error"
            | "discordant" => config.report.default_case_filter.clone(),
            _ => "all".to_owned(),
        },
        difference: format!("{:+.2} pp", summary.paired.difference_pp),
        interval: format!(
            "[{:.2}, {:.2}] pp",
            summary.bootstrap.lower_pp, summary.bootstrap.upper_pp
        ),
        baseline_primary: metric(summary.baseline.primary_pass, summary.baseline.total),
        candidate_primary: metric(summary.candidate.primary_pass, summary.candidate.total),
        structural_rows,
        transition: TransitionView {
            both_pass: summary.paired.both_pass,
            baseline_only: summary.paired.baseline_only_pass,
            candidate_only: summary.paired.candidate_only_pass,
            both_fail: summary.paired.both_fail,
        },
        research_studies: research_studies(records),
        gate_rules: summary
            .gate
            .rules
            .iter()
            .map(|rule| {
                let (state, class) = match rule.passed {
                    Some(true) => ("passed", "pass"),
                    Some(false) => ("failed", "fail"),
                    None => ("not evaluated", "neutral"),
                };
                GateRuleView {
                    name: rule.rule.clone(),
                    state: state.to_owned(),
                    class: class.to_owned(),
                    message: rule.message.clone(),
                }
            })
            .collect(),
        evaluator_rows,
        operational_rows: operational_rows(summary),
        hotspots: summary
            .field_hotspots
            .iter()
            .map(|item| HotspotView {
                pointer: item.pointer.clone(),
                regressions: item.regressions,
                improvements: item.improvements,
                failures: item.candidate_failures,
            })
            .collect(),
        cases,
        manifest_rows: vec![
            ("Run ID".to_owned(), manifest.run_id.clone()),
            (
                "StructTrace version".to_owned(),
                manifest.structtrace_version.clone(),
            ),
            (
                "Artifact format".to_owned(),
                manifest.artifact_format_version.to_string(),
            ),
            ("Dataset hash".to_owned(), manifest.dataset_hash.clone()),
            ("Schema hash".to_owned(), manifest.schema_hash.clone()),
            (
                "Configuration hash".to_owned(),
                manifest.normalized_configuration_hash.clone(),
            ),
            ("Binary target".to_owned(), manifest.binary_target.clone()),
        ],
    })
}

fn operational_rows(summary: &RunSummary) -> Vec<OperationalRow> {
    let baseline = &summary.baseline.operational;
    let candidate = &summary.candidate.operational;
    vec![
        operational(
            "Mean latency",
            milliseconds(baseline.mean_latency_ms),
            milliseconds(candidate.mean_latency_ms),
        ),
        operational(
            "Median latency",
            milliseconds(baseline.median_latency_ms),
            milliseconds(candidate.median_latency_ms),
        ),
        operational(
            "p95 latency",
            milliseconds(baseline.p95_latency_ms),
            milliseconds(candidate.p95_latency_ms),
        ),
        operational(
            "Latency observations",
            baseline.latency_observations.to_string(),
            candidate.latency_observations.to_string(),
        ),
        operational(
            "Matched latency pairs",
            format!(
                "{} / {}",
                summary.matched_operational.latency_pairs, summary.matched_operational.total_pairs
            ),
            format!(
                "{} / {}",
                summary.matched_operational.latency_pairs, summary.matched_operational.total_pairs
            ),
        ),
        operational(
            "Matched-pair p95 latency",
            milliseconds(summary.matched_operational.baseline_p95_latency_ms),
            milliseconds(summary.matched_operational.candidate_p95_latency_ms),
        ),
        operational(
            "Retry attempts",
            baseline.retry_attempts.to_string(),
            candidate.retry_attempts.to_string(),
        ),
        operational(
            "Input / output tokens",
            format!("{} / {}", baseline.input_tokens, baseline.output_tokens),
            format!("{} / {}", candidate.input_tokens, candidate.output_tokens),
        ),
        operational(
            "Average cost",
            format_cost(
                baseline.average_cost.as_deref(),
                baseline.currency.as_deref(),
            ),
            format_cost(
                candidate.average_cost.as_deref(),
                candidate.currency.as_deref(),
            ),
        ),
        operational(
            "Matched cost pairs",
            format!(
                "{} / {}",
                summary.matched_operational.cost_pairs, summary.matched_operational.total_pairs
            ),
            format!(
                "{} / {}",
                summary.matched_operational.cost_pairs, summary.matched_operational.total_pairs
            ),
        ),
        operational(
            "Matched-pair average cost",
            format_cost(
                summary.matched_operational.baseline_average_cost.as_deref(),
                summary.matched_operational.currency.as_deref(),
            ),
            format_cost(
                summary
                    .matched_operational
                    .candidate_average_cost
                    .as_deref(),
                summary.matched_operational.currency.as_deref(),
            ),
        ),
        operational(
            "Total cost",
            format_cost(baseline.total_cost.as_deref(), baseline.currency.as_deref()),
            format_cost(
                candidate.total_cost.as_deref(),
                candidate.currency.as_deref(),
            ),
        ),
    ]
}

fn operational(label: &str, baseline: String, candidate: String) -> OperationalRow {
    OperationalRow {
        label: label.to_owned(),
        baseline,
        candidate,
    }
}

fn milliseconds(value: Option<f64>) -> String {
    value.map_or_else(
        || "not available".to_owned(),
        |value| format!("{value:.1} ms"),
    )
}

fn format_cost(value: Option<&str>, currency: Option<&str>) -> String {
    match (value, currency) {
        (Some(value), Some(currency)) => format!("{value} {currency}"),
        _ => "not available".to_owned(),
    }
}

fn research_studies(records: &[PairedCaseRecord]) -> Vec<ResearchStudyView> {
    let mut groups: BTreeMap<String, (String, Vec<(bool, bool)>)> = BTreeMap::new();
    for record in records {
        let Some(metadata) = record.case.metadata.as_ref() else {
            continue;
        };
        let Some(study) = metadata.pointer("/study").and_then(Value::as_str) else {
            continue;
        };
        let label = metadata
            .pointer("/study_label")
            .and_then(Value::as_str)
            .unwrap_or(study)
            .to_owned();
        groups
            .entry(study.to_owned())
            .or_insert_with(|| (label, Vec::new()))
            .1
            .push((
                record.baseline_evaluation.primary_pass,
                record.candidate_evaluation.primary_pass,
            ));
    }
    groups
        .into_values()
        .map(|(label, pairs)| {
            let metrics = structtrace_core::statistics::paired_metrics(&pairs);
            ResearchStudyView {
                label,
                baseline: metric(metrics.baseline_pass, metrics.total),
                candidate: metric(metrics.candidate_pass, metrics.total),
                candidate_only: metrics.candidate_only_pass,
                baseline_only: metrics.baseline_only_pass,
            }
        })
        .collect()
}

fn case_view(record: &PairedCaseRecord, config: &Config) -> anyhow::Result<CaseView> {
    let source = serde_json::json!({
        "id": record.case.id,
        "input": record.case.input,
        "expected": record.case.expected,
        "metadata": record.case.metadata,
    });
    let secrets = selected_values(&source, &config.storage.redaction.json_pointers);
    let mut redacted_value = serde_json::to_value(record)
        .context("could not build fail-closed report view for a case")?;
    redact_matching_values(&mut redacted_value, &secrets);
    redact_value_raw_text(&mut redacted_value, "/baseline_output/raw_output", &secrets);
    redact_value_raw_text(
        &mut redacted_value,
        "/candidate_output/raw_output",
        &secrets,
    );
    if !config.report.include_raw_outputs || !config.storage.retain_raw_outputs {
        remove_object_key(&mut redacted_value, "/baseline_output", "raw_output");
        remove_object_key(&mut redacted_value, "/candidate_output", "raw_output");
        strip_provider_echoes(&mut redacted_value, "/baseline_output");
        strip_provider_echoes(&mut redacted_value, "/candidate_output");
    }
    if !config.report.include_prompts {
        remove_object_key(
            &mut redacted_value,
            "/baseline_output/metadata",
            "rendered_prompt",
        );
        remove_object_key(
            &mut redacted_value,
            "/candidate_output/metadata",
            "rendered_prompt",
        );
    }
    let mut filters = vec![record.transition.clone()];
    if matches!(
        record.transition.as_str(),
        "baseline_only_pass" | "candidate_only_pass"
    ) {
        filters.push("discordant".to_owned());
    }
    if record.baseline_evaluation.valid_but_wrong || record.candidate_evaluation.valid_but_wrong {
        filters.push("valid_but_wrong".to_owned());
    }
    if !record.baseline_evaluation.parse_valid || !record.candidate_evaluation.parse_valid {
        filters.push("parse_failure".to_owned());
    }
    if !record.baseline_evaluation.schema_valid || !record.candidate_evaluation.schema_valid {
        filters.push("schema_failure".to_owned());
    }
    if record.baseline_output.status != structtrace_core::output::OutputStatus::Ok
        || record.candidate_output.status != structtrace_core::output::OutputStatus::Ok
    {
        filters.push("adapter_error".to_owned());
    }
    let baseline_parsed = redacted_value
        .pointer("/baseline_evaluation/parsed_output")
        .cloned()
        .unwrap_or(Value::Null);
    let candidate_parsed = redacted_value
        .pointer("/candidate_evaluation/parsed_output")
        .cloned()
        .unwrap_or(Value::Null);
    let mut filter_string = filters.join(" ");
    redact_string(&mut filter_string, &secrets);
    Ok(CaseView {
        id: redacted_value
            .pointer("/case/id")
            .and_then(Value::as_str)
            .unwrap_or(REDACTION_MARKER)
            .to_owned(),
        transition: redacted_value
            .pointer("/transition")
            .and_then(Value::as_str)
            .unwrap_or(REDACTION_MARKER)
            .replace('_', " "),
        filters: filter_string,
        input: pretty_json(pointer_or_null(&redacted_value, "/case/input")),
        expected: optional_pretty(&redacted_value, "/case/expected"),
        metadata: optional_pretty(&redacted_value, "/case/metadata"),
        baseline_raw: raw_for_report(
            &redacted_value,
            "/baseline_output/raw_output",
            config.limits.max_report_raw_bytes_per_case,
        ),
        candidate_raw: raw_for_report(
            &redacted_value,
            "/candidate_output/raw_output",
            config.limits.max_report_raw_bytes_per_case,
        ),
        baseline_parsed: pretty_json(&baseline_parsed),
        candidate_parsed: pretty_json(&candidate_parsed),
        diffs: json_diff(&baseline_parsed, &candidate_parsed),
        baseline_schema_errors: pretty_json(pointer_or_null(
            &redacted_value,
            "/baseline_evaluation/schema_errors",
        )),
        candidate_schema_errors: pretty_json(pointer_or_null(
            &redacted_value,
            "/candidate_evaluation/schema_errors",
        )),
        baseline_evaluators: pretty_json(pointer_or_null(
            &redacted_value,
            "/baseline_evaluation/evaluators",
        )),
        candidate_evaluators: pretty_json(pointer_or_null(
            &redacted_value,
            "/candidate_evaluation/evaluators",
        )),
        baseline_metadata: pretty_json(pointer_or_null(
            &redacted_value,
            "/baseline_output/metadata",
        )),
        candidate_metadata: pretty_json(pointer_or_null(
            &redacted_value,
            "/candidate_output/metadata",
        )),
        baseline_execution: execution_view(&redacted_value, "/baseline_output"),
        candidate_execution: execution_view(&redacted_value, "/candidate_output"),
        baseline_latency: record
            .baseline_output
            .latency_ms
            .map_or_else(|| "Not recorded".to_owned(), |value| format!("{value} ms")),
        candidate_latency: record
            .candidate_output
            .latency_ms
            .map_or_else(|| "Not recorded".to_owned(), |value| format!("{value} ms")),
    })
}

fn execution_view(value: &Value, output_pointer: &str) -> String {
    let output = pointer_or_null(value, output_pointer);
    pretty_json(&serde_json::json!({
        "status": output.pointer("/status"),
        "error": output.pointer("/error"),
        "latency_ms": output.pointer("/latency_ms"),
        "usage": output.pointer("/usage"),
        "cost": output.pointer("/cost"),
        "retries": output.pointer("/retries"),
        "finish_reason": output.pointer("/metadata/finish_reason"),
    }))
}

fn pointer_or_null<'a>(value: &'a Value, pointer: &str) -> &'a Value {
    value.pointer(pointer).unwrap_or(&Value::Null)
}

fn optional_pretty(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .map_or_else(|| "Not provided".to_owned(), pretty_json)
}

fn raw_for_report(value: &Value, pointer: &str, max_bytes: usize) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(|raw| truncate_for_report(raw.to_owned(), max_bytes))
        .unwrap_or_else(|| "Not retained".to_owned())
}

fn redact_value_raw_text(value: &mut Value, pointer: &str, secrets: &[Value]) {
    let raw = value
        .pointer_mut(pointer)
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let Some(mut raw) = raw else {
        return;
    };
    redact_string(&mut raw, secrets);
    if let Some(target) = value.pointer_mut(pointer) {
        *target = Value::String(raw);
    }
}

fn redact_string(value: &mut String, secrets: &[Value]) {
    for secret in secrets {
        let needle = match secret {
            Value::String(value) => value.clone(),
            Value::Null => "null".to_owned(),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
        if !needle.is_empty() {
            *value = value.replace(&needle, REDACTION_MARKER);
        }
    }
}

fn remove_object_key(value: &mut Value, object_pointer: &str, key: &str) {
    if let Some(object) = value
        .pointer_mut(object_pointer)
        .and_then(Value::as_object_mut)
    {
        object.remove(key);
    }
}

fn strip_provider_echoes(value: &mut Value, output_pointer: &str) {
    remove_object_key(
        value,
        &format!("{output_pointer}/metadata"),
        "provider_response",
    );
    if let Some(retries) = value
        .pointer_mut(&format!("{output_pointer}/retries"))
        .and_then(Value::as_array_mut)
    {
        for retry in retries {
            if let Some(object) = retry.as_object_mut() {
                object.remove("response");
            }
        }
    }
}

fn truncate_for_report(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes.min(value.len());
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    value.push_str("\n[StructTrace: raw output truncated for report]");
    value
}

fn comparison(label: &str, baseline: usize, candidate: usize, total: usize) -> ComparisonRow {
    ComparisonRow {
        label: label.to_owned(),
        baseline: metric(baseline, total),
        candidate: metric(candidate, total),
    }
}

fn metric(count: usize, total: usize) -> MetricView {
    MetricView {
        count,
        total,
        percent: if total == 0 {
            "n/a".to_owned()
        } else {
            format!("{:.1}%", 100.0 * count as f64 / total as f64)
        },
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Vec<T>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("invalid JSON at {}:{}", path.display(), index + 1))
        })
        .collect()
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "<serialization failed>".to_owned())
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<serialization failed>".to_owned())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path.parent().context("report path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().sync_all()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

const TEMPLATE: &str = r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <meta name="color-scheme" content="light dark">
  <title>{{ project_name }} · StructTrace</title>
  <style>
    :root { color-scheme: light dark; --bg:#f6f8fb; --surface:#fff; --ink:#142238; --muted:#5c6b7e; --line:#d9e1eb; --blue:#155eef; --green:#087a55; --red:#b42318; --amber:#a15c00; --code:#0e1a2a; }
    @media (prefers-color-scheme:dark){:root{--bg:#0b1017;--surface:#111a26;--ink:#edf3fa;--muted:#a8b4c4;--line:#29384c;--blue:#70a0ff;--green:#5ed6aa;--red:#ff8c86;--amber:#ffc46b;--code:#070c12}}
    *{box-sizing:border-box} body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif} a{color:var(--blue)}
    header{background:#0b2138;color:#fff;border-bottom:1px solid #29425e} .bar{max-width:1180px;margin:auto;padding:18px 24px;display:flex;align-items:center;justify-content:space-between;gap:20px}.brand{font-weight:800;letter-spacing:.01em}.tagline{color:#c5d5e8;font-size:13px}
    main{max-width:1180px;margin:auto;padding:28px 24px 80px} h1{font-size:clamp(30px,5vw,48px);line-height:1.05;margin:.15em 0} h2{margin-top:48px;font-size:24px} h3{font-size:17px}.eyebrow{text-transform:uppercase;letter-spacing:.12em;font-weight:800;font-size:11px;color:var(--blue)} .muted{color:var(--muted)}
    .hero{display:grid;grid-template-columns:1fr auto;gap:28px;align-items:end}.gate{padding:14px 18px;border-radius:12px;font-weight:900;letter-spacing:.08em}.gate.pass{background:#d8f5e8;color:#075b40}.gate.fail{background:#fee4e2;color:#8f1710}
    .metrics{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin:26px 0}.metric,.panel{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:18px;box-shadow:0 4px 18px rgba(20,34,56,.04)}.metric strong{font-size:26px;display:block}.metric span{color:var(--muted);font-size:12px}
    table{width:100%;border-collapse:collapse;background:var(--surface);border:1px solid var(--line)}th,td{text-align:left;padding:11px 13px;border-bottom:1px solid var(--line)}th{font-size:12px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted)}td.num{text-align:right;font-variant-numeric:tabular-nums}
    .matrix{display:grid;grid-template-columns:repeat(2,minmax(120px,1fr));max-width:520px;border:1px solid var(--line);border-radius:12px;overflow:hidden}.cell{padding:24px;border:1px solid var(--line);background:var(--surface)}.cell strong{display:block;font-size:30px}.cell.win{background:color-mix(in srgb,var(--green) 13%,var(--surface))}.cell.loss{background:color-mix(in srgb,var(--red) 13%,var(--surface))}
    .rule{display:grid;grid-template-columns:110px 1fr;gap:12px;padding:13px 0;border-bottom:1px solid var(--line)}.pill{font-size:11px;font-weight:800;text-transform:uppercase;letter-spacing:.06em}.pill.pass{color:var(--green)}.pill.fail{color:var(--red)}.pill.neutral{color:var(--muted)}
    .filters{display:flex;flex-wrap:wrap;gap:8px;margin:14px 0}.filters button{border:1px solid var(--line);background:var(--surface);color:var(--ink);border-radius:999px;padding:7px 12px;cursor:pointer}.filters button[aria-pressed=true]{background:var(--blue);border-color:var(--blue);color:#fff}
    .case{background:var(--surface);border:1px solid var(--line);border-radius:12px;margin:10px 0;overflow:hidden}.case>summary{cursor:pointer;padding:14px 16px;font-weight:700}.case-body{padding:0 16px 18px}.case-grid{display:grid;grid-template-columns:1fr 1fr;gap:12px}.code{background:var(--code);color:#e7eef8;border-radius:9px;padding:12px;white-space:pre-wrap;overflow-wrap:anywhere;max-height:360px;overflow:auto;font:12px/1.5 ui-monospace,SFMono-Regular,Consolas,monospace}.diff{display:grid;grid-template-columns:minmax(90px,.7fr) .6fr 1fr 1fr;gap:1px;background:var(--line);border:1px solid var(--line)}.diff>*{background:var(--surface);padding:8px;overflow-wrap:anywhere}.diff .head{font-size:11px;font-weight:800;text-transform:uppercase;color:var(--muted)}
    .repro{display:grid;grid-template-columns:minmax(160px,.4fr) 1fr;gap:1px;background:var(--line);border:1px solid var(--line)}.repro>*{padding:9px;background:var(--surface);overflow-wrap:anywhere}.empty{color:var(--muted);font-style:italic}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}
    footer{margin-top:60px;padding-top:20px;border-top:1px solid var(--line);color:var(--muted);font-size:12px}@media(max-width:760px){.hero{grid-template-columns:1fr}.metrics{grid-template-columns:1fr 1fr}.case-grid{grid-template-columns:1fr}.diff{grid-template-columns:1fr}.diff .head{display:none}.repro{grid-template-columns:1fr}.bar{align-items:flex-start;flex-direction:column}}@media(max-width:420px){main{padding-left:14px;padding-right:14px}.metrics{grid-template-columns:1fr}}
    @media print{header,.filters{display:none}body{background:#fff;color:#111}.panel,.metric,.case{box-shadow:none;break-inside:avoid}details:not([open])>*:not(summary){display:block}}
  </style>
</head>
<body>
<header><div class="bar"><div><div class="brand">StructTrace</div><div class="tagline">Your schema passed. Did the answer?</div></div><div>Local paired regression report</div></div></header>
<main>
  <section class="hero"><div><div class="eyebrow">Structured-output release evidence</div><h1>{{ project_name }}</h1><p class="muted">Run {{ run_id }}</p></div><div class="gate {{ gate_class }}">{{ gate_label }}</div></section>
  <section class="metrics" aria-label="Executive summary">
    <div class="metric"><span>Baseline primary outcome</span><strong>{{ baseline_primary.percent }}</strong>{{ baseline_primary.count }}/{{ baseline_primary.total }}</div>
    <div class="metric"><span>Candidate primary outcome</span><strong>{{ candidate_primary.percent }}</strong>{{ candidate_primary.count }}/{{ candidate_primary.total }}</div>
    <div class="metric"><span>Paired difference</span><strong>{{ difference }}</strong>candidate minus baseline</div>
    <div class="metric"><span>Paired bootstrap interval</span><strong>{{ interval }}</strong>seeded matched resampling</div>
  </section>

  {% if research_studies %}<section><h2>Accepted research matrices</h2><p class="muted">The same class of contract-preserving change had different effects across evaluated systems. These are compact normalized outcomes, not universal model rankings.</p><table><thead><tr><th>Study</th><th>Baseline correct</th><th>Candidate correct</th><th>Candidate-only</th><th>Baseline-only</th></tr></thead><tbody>{% for study in research_studies %}<tr><td>{{ study.label }}</td><td class="num">{{ study.baseline.count }}/{{ study.baseline.total }}</td><td class="num">{{ study.candidate.count }}/{{ study.candidate.total }}</td><td class="num">{{ study.candidate_only }}</td><td class="num">{{ study.baseline_only }}</td></tr>{% endfor %}</tbody></table></section>{% endif %}

  <h2>Structural validity versus correctness</h2>
  <p class="muted">Validity and correctness are deliberately separate. A schema-valid output can still fail the configured semantic or executable outcome.</p>
  <table><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th></tr></thead><tbody>{% for row in structural_rows %}<tr><td>{{ row.label }}</td><td class="num">{{ row.baseline.count }}/{{ row.baseline.total }} · {{ row.baseline.percent }}</td><td class="num">{{ row.candidate.count }}/{{ row.candidate.total }} · {{ row.candidate.percent }}</td></tr>{% endfor %}</tbody></table>

  <h2>Paired transition matrix</h2>
  <div class="matrix" aria-label="Paired transition matrix"><div class="cell"><span>Both pass</span><strong>{{ transition.both_pass }}</strong></div><div class="cell loss"><span>Baseline-only pass</span><strong>{{ transition.baseline_only }}</strong></div><div class="cell win"><span>Candidate-only pass</span><strong>{{ transition.candidate_only }}</strong></div><div class="cell"><span>Both fail</span><strong>{{ transition.both_fail }}</strong></div></div>

  <h2>Release gate</h2><div class="panel">{% for rule in gate_rules %}<div class="rule"><div class="pill {{ rule.class }}">{{ rule.state }}</div><div><strong>{{ rule.name }}</strong><br><span class="muted">{{ rule.message }}</span></div></div>{% endfor %}</div>

  <h2>Evaluator results</h2><table><thead><tr><th>Evaluator</th><th>Baseline passes</th><th>Candidate passes</th></tr></thead><tbody>{% for row in evaluator_rows %}<tr><td><code>{{ row.label }}</code></td><td class="num">{{ row.baseline.count }}/{{ row.baseline.total }}</td><td class="num">{{ row.candidate.count }}/{{ row.candidate.total }}</td></tr>{% endfor %}</tbody></table>

  <h2>Operational comparison</h2><p class="muted">Latency is descriptive unless a threshold is configured. Costs are shown only from explicit adapter pricing and are never inferred.</p><table><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th></tr></thead><tbody>{% for row in operational_rows %}<tr><td>{{ row.label }}</td><td class="num">{{ row.baseline }}</td><td class="num">{{ row.candidate }}</td></tr>{% endfor %}</tbody></table>

  <h2>Field-level hotspots</h2>{% if hotspots %}<table><thead><tr><th>JSON Pointer</th><th>Candidate regressions</th><th>Candidate improvements</th><th>Candidate failures</th></tr></thead><tbody>{% for item in hotspots %}<tr><td><code>{{ item.pointer }}</code></td><td class="num">{{ item.regressions }}</td><td class="num">{{ item.improvements }}</td><td class="num">{{ item.failures }}</td></tr>{% endfor %}</tbody></table>{% else %}<p class="empty">No field-level evaluators were configured.</p>{% endif %}

  <h2>Discordant case explorer</h2>
  <div class="filters" role="group" aria-label="Case filters"><button data-filter="all" aria-pressed="false">All</button><button data-filter="discordant" aria-pressed="false">Discordant</button><button data-filter="baseline_only_pass" aria-pressed="false">Baseline-only</button><button data-filter="candidate_only_pass" aria-pressed="false">Candidate-only</button><button data-filter="both_fail" aria-pressed="false">Both fail</button><button data-filter="valid_but_wrong" aria-pressed="false">Valid but wrong</button><button data-filter="parse_failure" aria-pressed="false">Parse failures</button><button data-filter="schema_failure" aria-pressed="false">Schema failures</button><button data-filter="adapter_error" aria-pressed="false">Adapter errors</button></div>
  <div id="cases">{% for case in cases %}<details class="case" data-filters="{{ case.filters }}"><summary>{{ case.id }} · {{ case.transition }}</summary><div class="case-body">
    <div class="case-grid"><section><h3>Input</h3><pre class="code">{{ case.input }}</pre></section><section><h3>Expected</h3><pre class="code">{{ case.expected }}</pre></section></div>
    <div class="case-grid"><section><h3>Baseline raw · {{ case.baseline_latency }}</h3><pre class="code">{{ case.baseline_raw }}</pre></section><section><h3>Candidate raw · {{ case.candidate_latency }}</h3><pre class="code">{{ case.candidate_raw }}</pre></section></div>
    <div class="case-grid"><section><h3>Baseline parsed</h3><pre class="code">{{ case.baseline_parsed }}</pre></section><section><h3>Candidate parsed</h3><pre class="code">{{ case.candidate_parsed }}</pre></section></div>
    <h3>Structured diff</h3>{% if case.diffs %}<div class="diff"><div class="head">Path</div><div class="head">Change</div><div class="head">Baseline</div><div class="head">Candidate</div>{% for diff in case.diffs %}<code>{{ diff.path }}</code><span>{{ diff.kind }}</span><code>{{ diff.baseline }}</code><code>{{ diff.candidate }}</code>{% endfor %}</div>{% else %}<p class="empty">Parsed outputs are identical.</p>{% endif %}
    <div class="case-grid"><section><h3>Baseline schema errors</h3><pre class="code">{{ case.baseline_schema_errors }}</pre></section><section><h3>Candidate schema errors</h3><pre class="code">{{ case.candidate_schema_errors }}</pre></section></div>
    <div class="case-grid"><section><h3>Baseline evaluators</h3><pre class="code">{{ case.baseline_evaluators }}</pre></section><section><h3>Candidate evaluators</h3><pre class="code">{{ case.candidate_evaluators }}</pre></section></div>
    <div class="case-grid"><section><h3>Baseline execution evidence</h3><pre class="code">{{ case.baseline_execution }}</pre></section><section><h3>Candidate execution evidence</h3><pre class="code">{{ case.candidate_execution }}</pre></section></div>
    <div class="case-grid"><section><h3>Baseline adapter metadata</h3><pre class="code">{{ case.baseline_metadata }}</pre></section><section><h3>Candidate adapter metadata</h3><pre class="code">{{ case.candidate_metadata }}</pre></section></div>
    <h3>Case metadata</h3><pre class="code">{{ case.metadata }}</pre>
  </div></details>{% endfor %}</div>

  <h2>Reproducibility</h2><div class="repro">{% for row in manifest_rows %}<strong>{{ row.0 }}</strong><code>{{ row.1 }}</code>{% endfor %}</div><p><code>structtrace replay {{ run_id }}</code></p>
  <footer>Generated locally by StructTrace. No telemetry, external assets, or analytics.</footer>
</main>
<script>
  const buttons=[...document.querySelectorAll('[data-filter]')], cases=[...document.querySelectorAll('.case')];
  function applyFilter(button){const filter=button.dataset.filter;for(const item of buttons)item.setAttribute('aria-pressed',String(item===button));for(const item of cases)item.hidden=filter!=='all'&&!item.dataset.filters.split(' ').includes(filter);}
  for(const button of buttons)button.addEventListener('click',()=>applyFilter(button));
  applyFilter(buttons.find(button=>button.dataset.filter==='{{ default_filter }}')||buttons[0]);
</script>
</body></html>"##;

#[cfg(test)]
mod tests {
    use serde_json::json;
    use structtrace_core::{
        artifact::{OperationalSummary, RunSummary, VariantSummary},
        config::Config,
        dataset::Case,
        evaluation::{compile_schema, evaluate_case},
        gate::GateDecision,
        output::{OutputStatus, VariantOutput},
        statistics::{BootstrapInterval, paired_metrics},
    };

    use super::*;

    fn report_config(title: &str) -> Config {
        serde_json::from_value(json!({
            "version": 1,
            "project": {"name": "report-test"},
            "dataset": {"path": "data.jsonl", "format": "jsonl"},
            "schema": {"path": "schema.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {"kind": "recorded", "path": "candidate.jsonl"}
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"},
            "report": {"title": title, "default_case_filter": "all"}
        }))
        .unwrap()
    }

    fn passing_record(id: String, input: Value) -> PairedCaseRecord {
        let expected = json!({"answer": "ok"});
        let case = Case {
            id,
            input,
            expected: Some(expected.clone()),
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let output = VariantOutput {
            case_id: case.id.clone(),
            status: OutputStatus::Ok,
            raw_output: Some(expected.to_string()),
            parsed_output: None,
            error: None,
            latency_ms: None,
            usage: None,
            cost: None,
            metadata: Value::Null,
            retries: vec![],
        };
        let config = report_config("test");
        let schema = compile_schema(&json!({"type": "object"})).unwrap();
        let evaluation = evaluate_case(
            &case,
            &output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
        );
        PairedCaseRecord {
            case,
            baseline_output: output.clone(),
            candidate_output: output,
            baseline_evaluation: evaluation.clone(),
            candidate_evaluation: evaluation,
            transition: "both_pass".to_owned(),
        }
    }

    fn render_records(config: &Config, records: &[PairedCaseRecord]) -> String {
        let total = records.len();
        let paired = paired_metrics(&vec![(true, true); total]);
        let variant = VariantSummary {
            total,
            parse_valid: total,
            schema_valid: total,
            primary_pass: total,
            valid_but_wrong: 0,
            errors: 0,
            timeouts: 0,
            operational: OperationalSummary::default(),
            ..VariantSummary::default()
        };
        let summary = RunSummary {
            artifact_format_version: structtrace_core::ARTIFACT_FORMAT_VERSION,
            run_id: "report-run".to_owned(),
            primary_outcome: "correct".to_owned(),
            baseline: variant.clone(),
            candidate: variant,
            matched_operational: Default::default(),
            paired,
            bootstrap: BootstrapInterval {
                lower_pp: 0.0,
                upper_pp: 0.0,
                confidence: 0.95,
                samples: 100,
                seed: 17,
            },
            gate: GateDecision {
                passed: true,
                rules: vec![],
            },
            evaluator_passes: BTreeMap::new(),
            field_hotspots: vec![],
        };
        let manifest = RunManifest::new("report-run".to_owned(), "report-test".to_owned());
        let view = build_view(&summary, &manifest, records, config).unwrap();
        let mut environment = Environment::new();
        environment.set_auto_escape_callback(|_| AutoEscape::Html);
        environment.add_template("report.html", TEMPLATE).unwrap();
        environment
            .get_template("report.html")
            .unwrap()
            .render(view)
            .unwrap()
    }

    #[test]
    fn structured_diff_distinguishes_change_types() {
        let differences = json_diff(
            &json!({"a": 1, "b": "x", "gone": true}),
            &json!({"a": 2, "b": 3, "new": false}),
        );
        assert!(
            differences
                .iter()
                .any(|item| item.path == "/a" && item.kind == "value_changed")
        );
        assert!(
            differences
                .iter()
                .any(|item| item.path == "/b" && item.kind == "type_changed")
        );
        assert!(
            differences
                .iter()
                .any(|item| item.path == "/gone" && item.kind == "removed")
        );
        assert!(
            differences
                .iter()
                .any(|item| item.path == "/new" && item.kind == "added")
        );
    }

    #[test]
    fn report_template_has_no_external_assets() {
        assert!(!TEMPLATE.contains("https://"));
        assert!(!TEMPLATE.contains("http://"));
        assert!(!TEMPLATE.contains("cdn"));
    }

    #[test]
    fn rendered_report_escapes_untrusted_content_and_renders_empty_sections() {
        let attack = "<script>alert('structtrace')</script>";
        let config = report_config(attack);
        let records = vec![passing_record(attack.to_owned(), json!({"text": attack}))];
        let html = render_records(&config, &records);
        assert!(!html.contains(attack));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("No field-level evaluators were configured."));
    }

    #[test]
    fn rendered_report_preserves_every_case_in_a_large_set() {
        let config = report_config("large report");
        let records = (0..500)
            .map(|index| passing_record(format!("case-{index:04}"), json!({"index": index})))
            .collect::<Vec<_>>();
        let html = render_records(&config, &records);
        assert_eq!(html.matches("<details class=\"case\"").count(), 500);
        assert!(html.contains("case-0000"));
        assert!(html.contains("case-0499"));
    }

    #[test]
    fn rendered_report_bounds_embedded_raw_output_at_utf8_boundary() {
        let mut config = report_config("bounded report");
        config.limits.max_report_raw_bytes_per_case = 11;
        let mut record = passing_record("bounded".to_owned(), json!({"safe": true}));
        let oversized = "🙂🙂🙂END_OF_RAW".to_owned();
        record.baseline_output.raw_output = Some(oversized.clone());
        record.candidate_output.raw_output = Some(oversized);
        let html = render_records(&config, &[record]);
        assert!(html.contains("StructTrace: raw output truncated for report"));
        assert!(!html.contains("END_OF_RAW"));
    }

    #[test]
    fn rendered_prompts_are_visible_only_after_explicit_opt_in() {
        let prompt = "PROMPT_SENTINEL_4b15";
        let mut record = passing_record("prompt-case".to_owned(), json!({"safe": true}));
        record.baseline_output.metadata = json!({"rendered_prompt": prompt, "model": "local"});
        record.candidate_output.metadata = record.baseline_output.metadata.clone();

        let hidden = render_records(&report_config("prompts hidden"), &[record.clone()]);
        assert!(!hidden.contains(prompt));
        assert!(hidden.contains("local"));

        let mut visible_config = report_config("prompts visible");
        visible_config.report.include_prompts = true;
        let visible = render_records(&visible_config, &[record]);
        assert!(visible.contains(prompt));
    }

    #[test]
    fn case_view_redacts_input_secrets_and_their_output_echoes() {
        let config: Config = serde_json::from_value(json!({
            "version": 1,
            "project": {"name": "privacy"},
            "storage": {
                "root": ".structtrace",
                "retain_raw_outputs": true,
                "retain_provider_responses": true,
                "redaction": {"json_pointers": ["/input/customer_email"]}
            },
            "dataset": {"path": "data.jsonl", "format": "jsonl"},
            "schema": {"path": "schema.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {"kind": "recorded", "path": "candidate.jsonl"}
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"}
        }))
        .unwrap();
        let secret = "private@example.com";
        let case = Case {
            id: "one".to_owned(),
            input: json!({"customer_email": secret}),
            expected: Some(json!({"email": secret})),
            model_visible_metadata: None,
            metadata: None,
            source_line: 1,
        };
        let output = VariantOutput {
            case_id: "one".to_owned(),
            status: OutputStatus::Ok,
            raw_output: Some(format!("{{\"email\":\"{secret}\"}}")),
            parsed_output: None,
            error: None,
            latency_ms: Some(2),
            usage: None,
            cost: None,
            metadata: Value::Null,
            retries: vec![],
        };
        let schema = compile_schema(&json!({"type": "object"})).unwrap();
        let evaluation = evaluate_case(
            &case,
            &output,
            &schema,
            &config.evaluators,
            &config.outcomes,
            &config.analysis.primary_outcome,
        );
        let record = PairedCaseRecord {
            case,
            baseline_output: output.clone(),
            candidate_output: output,
            baseline_evaluation: evaluation.clone(),
            candidate_evaluation: evaluation,
            transition: "both_pass".to_owned(),
        };
        let view = case_view(&record, &config).unwrap();
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains(secret));
        assert!(serialized.contains(REDACTION_MARKER));
    }

    #[test]
    fn redaction_never_falls_back_for_typed_or_reserved_values() {
        let config: Config = serde_json::from_value(json!({
            "version": 1,
            "project": {"name": "privacy-adversarial"},
            "storage": {
                "redaction": {"json_pointers": ["/input/secret"]}
            },
            "dataset": {"path": "data.jsonl"},
            "schema": {"path": "schema.json"},
            "variants": {
                "baseline": {"kind": "recorded", "path": "baseline.jsonl"},
                "candidate": {"kind": "recorded", "path": "candidate.jsonl"}
            },
            "evaluators": [{"id": "exact", "kind": "exact_json"}],
            "outcomes": {"correct": {"all_of": ["exact"]}},
            "analysis": {"primary_outcome": "correct"}
        }))
        .unwrap();
        for secret in [
            json!("ok"),
            json!("error"),
            json!("both_pass"),
            json!(1),
            json!(true),
        ] {
            let mut record = passing_record("privacy-case".to_owned(), json!({"secret": secret}));
            record.case.expected = Some(json!({"echo": secret}));
            record.baseline_output.metadata = json!({
                "provider_response": {"echo": secret},
                "status_echo": secret
            });
            record.candidate_output.metadata = record.baseline_output.metadata.clone();
            let view = case_view(&record, &config).unwrap();
            let serialized = serde_json::to_string(&view).unwrap();
            assert!(serialized.contains(REDACTION_MARKER));
            let literal = serde_json::to_string(&secret).unwrap();
            assert!(!view.input.contains(&literal));
            assert!(!view.expected.contains(&literal));
            assert!(!view.baseline_metadata.contains(&literal));
            if secret == json!("both_pass") {
                assert!(!view.filters.contains("both_pass"));
            }
        }
    }

    #[test]
    fn hidden_raw_output_removes_provider_response_echo() {
        let mut config = report_config("provider privacy");
        config.report.include_raw_outputs = false;
        config.storage.retain_provider_responses = true;
        let echo = "PROVIDER_ECHO_SENTINEL";
        let mut record = passing_record("provider-case".to_owned(), json!({"safe": true}));
        record.baseline_output.metadata = json!({"provider_response": {"content": echo}});
        record.candidate_output.metadata = record.baseline_output.metadata.clone();
        record.baseline_output.retries = vec![json!({"response": {"content": echo}})];
        record.candidate_output.retries = record.baseline_output.retries.clone();
        let view = case_view(&record, &config).unwrap();
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains(echo));
        assert!(!serialized.contains("provider_response"));
    }
}
