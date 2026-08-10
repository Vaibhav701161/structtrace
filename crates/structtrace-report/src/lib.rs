//! Offline report generation and loopback-only serving.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use axum::response::IntoResponse;
use minijinja::{AutoEscape, Environment};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_core::{
    artifact::{
        EvaluatorStateCounts, PairedCaseRecord, RunManifest, RunStatus, RunSummary, VariantSummary,
    },
    config::{Config, TextRedactionMode},
    gate::{GateRuleStatus, GateStatus},
    hashing::hash_file,
    privacy::{
        REDACTION_MARKER, redact_matching_values_with_policy, redact_text_with_policy,
        selected_values,
    },
};
use tempfile::NamedTempFile;

/// The report asset format is versioned independently from stored scores.
pub const REPORT_FORMAT_VERSION: u32 = 3;

const CASE_CHUNK_SIZE: usize = 50;

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
    deployment_authorized: bool,
    has_quality_failures: bool,
    has_evidence_failures: bool,
    has_runtime_errors: bool,
    default_filter: String,
    difference: String,
    interval: String,
    total_rows: usize,
    unique_cases: usize,
    duplicate_groups: usize,
    largest_duplicate_group: usize,
    repeated_trial_groups: usize,
    label_conflict_groups: usize,
    inference_unit: String,
    evidence_denominator: usize,
    semantic_jointly_scored: usize,
    semantic_excluded: usize,
    semantic_difference: String,
    baseline_primary: MetricView,
    candidate_primary: MetricView,
    structural_rows: Vec<ComparisonRow>,
    descriptive_rows: Vec<ComparisonRow>,
    transition: TransitionView,
    research_studies: Vec<ResearchStudyView>,
    gate_rules: Vec<GateRuleView>,
    evaluator_rows: Vec<EvaluatorRowView>,
    operational_rows: Vec<OperationalRow>,
    hotspots: Vec<HotspotView>,
    diagnostic_hotspots: Vec<HotspotView>,
    cases: Vec<CaseView>,
    embedded_cases_json: Option<String>,
    share_derivative: bool,
    manifest_rows: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
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
struct EvaluatorRowView {
    id: String,
    baseline: EvaluatorCountsView,
    candidate: EvaluatorCountsView,
}

#[derive(Debug, Serialize)]
struct EvaluatorCountsView {
    pass: usize,
    fail: usize,
    error: usize,
    not_applicable: usize,
    unscored: usize,
}

#[derive(Debug, Serialize)]
struct TransitionView {
    both_pass: usize,
    baseline_only: usize,
    candidate_only: usize,
    both_fail: usize,
}

#[derive(Debug, Clone, Serialize)]
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
    evaluator_id: String,
    pointer: String,
    regressions: usize,
    improvements: usize,
    failures: usize,
    baseline_states: String,
    candidate_states: String,
}

#[derive(Debug, Serialize)]
struct CaseView {
    id: String,
    transition: String,
    filters: String,
    input: String,
    expected: String,
    metadata: String,
    model_visible_metadata: String,
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

#[derive(Debug, Serialize, Deserialize)]
struct CaseIndexEntry {
    id: String,
    transition: String,
    filters: String,
    search: String,
    chunk: usize,
    offset: usize,
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
    let report_dir = run_dir.join("report");
    if report_dir.exists() {
        anyhow::ensure!(
            std::fs::read_dir(&report_dir)?.next().is_none(),
            "refusing to replace a non-empty report directory"
        );
        std::fs::remove_dir(&report_dir)?;
    }
    let temporary = tempfile::Builder::new()
        .prefix("report.tmp.")
        .tempdir_in(run_dir)?;
    let temporary_report = temporary.path();
    let streamed = stream_case_chunks(&run_dir.join("cases.jsonl"), temporary_report, &config)?;
    let mut view = build_view(&summary, &manifest, &[], &config)?;
    view.research_studies = streamed.research_studies;
    let mut environment = Environment::new();
    environment.set_auto_escape_callback(|_| AutoEscape::Html);
    environment.add_template("report.html", TEMPLATE)?;
    let html = environment.get_template("report.html")?.render(view)?;
    let index_path = temporary_report.join("index.html");
    atomic_write(&index_path, html.as_bytes())?;

    let conservative_single_estimate = html
        .len()
        .saturating_mul(2)
        .saturating_add(streamed.chunk_bytes.saturating_mul(8))
        .saturating_add(1024 * 1024);
    if conservative_single_estimate <= config.limits.max_single_file_report_bytes {
        let mut single_view = build_view(&summary, &manifest, &[], &config)?;
        single_view.research_studies = streamed.research_studies_for_single;
        single_view.embedded_cases_json = Some(read_embedded_chunks(
            temporary_report,
            streamed.chunk_count,
            streamed.case_count,
        )?);
        let single_html = environment
            .get_template("report.html")?
            .render(single_view)?;
        anyhow::ensure!(
            single_html.len() <= config.limits.max_single_file_report_bytes,
            "single-file report exceeded its conservative pre-render budget"
        );
        atomic_write(
            &temporary_report.join("single.html"),
            single_html.as_bytes(),
        )?;
    }

    let report_bytes = directory_size(temporary_report)?;
    anyhow::ensure!(
        report_bytes <= config.limits.max_report_total_bytes as u64,
        "generated report is {report_bytes} bytes, above limits.max_report_total_bytes ({})",
        config.limits.max_report_total_bytes
    );
    let temporary_path = temporary.keep();
    std::fs::rename(&temporary_path, &report_dir)?;
    Ok(GeneratedReport {
        index_path: report_dir.join("index.html"),
    })
}

struct StreamedCases {
    case_count: usize,
    chunk_count: usize,
    chunk_bytes: usize,
    research_studies: Vec<ResearchStudyView>,
    research_studies_for_single: Vec<ResearchStudyView>,
}

fn stream_case_chunks(
    source: &Path,
    report_dir: &Path,
    config: &Config,
) -> anyhow::Result<StreamedCases> {
    let metadata = std::fs::symlink_metadata(source)?;
    anyhow::ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "cases artifact must be a regular non-symlink file"
    );
    anyhow::ensure!(
        metadata.len() <= config.limits.max_replay_artifact_bytes as u64,
        "cases artifact exceeds limits.max_replay_artifact_bytes"
    );
    let cases_dir = report_dir.join("cases");
    std::fs::create_dir_all(&cases_dir)?;
    let mut source = BufReader::new(std::fs::File::open(source)?);
    let index_file = std::fs::File::create(report_dir.join("case-index.json"))?;
    let mut index = BufWriter::new(index_file);
    index.write_all(b"[")?;
    let mut first_index = true;
    let mut line = Vec::new();
    let mut chunk = Vec::with_capacity(CASE_CHUNK_SIZE);
    let mut case_count = 0usize;
    let mut chunk_count = 0usize;
    let mut chunk_bytes = 0usize;
    let mut studies = BTreeMap::<String, (String, usize, usize, usize, usize)>::new();
    loop {
        line.clear();
        let read = source.read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        anyhow::ensure!(
            line.len() <= config.limits.max_jsonl_line_bytes,
            "cases.jsonl line {} exceeds the retained line limit",
            case_count + 1
        );
        anyhow::ensure!(!line.is_empty(), "cases.jsonl contains a blank line");
        anyhow::ensure!(
            case_count < config.limits.max_cases,
            "cases.jsonl exceeds the retained case-count limit"
        );
        let text = std::str::from_utf8(&line)
            .with_context(|| format!("cases.jsonl line {} is not UTF-8", case_count + 1))?;
        let record: PairedCaseRecord = structtrace_core::strict_json::from_str(text)
            .with_context(|| format!("invalid cases.jsonl line {}", case_count + 1))?;
        if let Some(metadata) = record.case.metadata.as_ref() {
            if let Some(study) = metadata.pointer("/study").and_then(Value::as_str) {
                let label = metadata
                    .pointer("/study_label")
                    .and_then(Value::as_str)
                    .unwrap_or(study)
                    .to_owned();
                let counts = studies
                    .entry(study.to_owned())
                    .or_insert((label, 0, 0, 0, 0));
                match (
                    record.baseline_evaluation.primary_pass,
                    record.candidate_evaluation.primary_pass,
                ) {
                    (true, true) => counts.1 += 1,
                    (true, false) => counts.2 += 1,
                    (false, true) => counts.3 += 1,
                    (false, false) => counts.4 += 1,
                }
            }
        }
        chunk.push(case_view(&record, config)?);
        case_count += 1;
        if chunk.len() == CASE_CHUNK_SIZE {
            chunk_bytes += write_case_chunk(
                &cases_dir,
                chunk_count,
                &chunk,
                &mut index,
                &mut first_index,
            )?;
            chunk.clear();
            chunk_count += 1;
        }
    }
    if !chunk.is_empty() {
        chunk_bytes += write_case_chunk(
            &cases_dir,
            chunk_count,
            &chunk,
            &mut index,
            &mut first_index,
        )?;
        chunk_count += 1;
    }
    index.write_all(b"]")?;
    index.flush()?;
    let research_studies = studies
        .into_values()
        .map(
            |(label, both_pass, baseline_only, candidate_only, both_fail)| {
                let total = both_pass + baseline_only + candidate_only + both_fail;
                ResearchStudyView {
                    label,
                    baseline: metric(both_pass + baseline_only, total),
                    candidate: metric(both_pass + candidate_only, total),
                    candidate_only,
                    baseline_only,
                }
            },
        )
        .collect::<Vec<_>>();
    Ok(StreamedCases {
        case_count,
        chunk_count,
        chunk_bytes,
        research_studies: research_studies.clone(),
        research_studies_for_single: research_studies,
    })
}

fn write_case_chunk(
    cases_dir: &Path,
    chunk_index: usize,
    chunk: &[CaseView],
    index: &mut BufWriter<std::fs::File>,
    first_index: &mut bool,
) -> anyhow::Result<usize> {
    let bytes = serde_json::to_vec(chunk)?;
    atomic_write(&cases_dir.join(format!("{chunk_index:05}.json")), &bytes)?;
    for (offset, case) in chunk.iter().enumerate() {
        if !*first_index {
            index.write_all(b",")?;
        }
        *first_index = false;
        serde_json::to_writer(
            &mut *index,
            &CaseIndexEntry {
                id: case.id.clone(),
                transition: case.transition.clone(),
                filters: case.filters.clone(),
                search: format!(
                    "{} {} {} {}",
                    case.id, case.metadata, case.model_visible_metadata, case.filters
                )
                .to_lowercase(),
                chunk: chunk_index,
                offset,
            },
        )?;
    }
    Ok(bytes.len())
}

fn read_embedded_chunks(
    report_dir: &Path,
    chunk_count: usize,
    case_count: usize,
) -> anyhow::Result<String> {
    let mut embedded = String::with_capacity(case_count.saturating_mul(256));
    embedded.push('[');
    for chunk_index in 0..chunk_count {
        let bytes = std::fs::read(
            report_dir
                .join("cases")
                .join(format!("{chunk_index:05}.json")),
        )?;
        let text = std::str::from_utf8(&bytes)?;
        let body = text
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .context("generated case chunk was not a JSON array")?;
        if !body.is_empty() {
            if embedded.len() > 1 {
                embedded.push(',');
            }
            embedded.push_str(body);
        }
    }
    embedded.push(']');
    Ok(embedded)
}

/// Export the generated report as one self-contained HTML file.
pub fn export_single_file(run_dir: &Path, destination: &Path) -> anyhow::Result<()> {
    let generated = finalized_report(run_dir)?;
    let single_path = generated
        .index_path
        .parent()
        .context("generated report has no directory")?
        .join("single.html");
    anyhow::ensure!(
        single_path.is_file(),
        "this report exceeds limits.max_single_file_report_bytes; use the chunked report directory"
    );
    let bytes = std::fs::read(single_path)?;
    atomic_write(destination, &bytes)
}

/// Export an aggregate-only report derivative with no case bodies, prompts, outputs, or metadata.
pub fn export_share_directory(run_dir: &Path, destination: &Path) -> anyhow::Result<()> {
    finalized_report(run_dir)?;
    anyhow::ensure!(
        !destination.exists(),
        "share destination already exists: {}",
        destination.display()
    );
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    verify_bound_artifact(run_dir, &manifest, "summary.json")?;
    let config: Config = read_json(&run_dir.join("inputs/configuration.json"))?;
    write_share_directory(&summary, &manifest, &config, destination)
}

fn write_share_directory(
    summary: &RunSummary,
    manifest: &RunManifest,
    config: &Config,
    destination: &Path,
) -> anyhow::Result<()> {
    let mut view = build_view(summary, manifest, &[], config)?;
    view.share_derivative = true;
    view.project_name = "StructTrace aggregate report".to_owned();
    view.default_filter = "all".to_owned();
    view.evaluator_rows.clear();
    view.hotspots.clear();
    view.diagnostic_hotspots.clear();
    view.research_studies.clear();
    view.manifest_rows = vec![
        ("Run ID".to_owned(), manifest.run_id.clone()),
        (
            "Run kind".to_owned(),
            serde_json::to_value(manifest.run_kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_owned(),
        ),
        (
            "StructTrace version".to_owned(),
            manifest.structtrace_version.clone(),
        ),
        (
            "Total paired cases".to_owned(),
            summary.paired.total.to_string(),
        ),
        (
            "Jointly scored cases".to_owned(),
            jointly_scored_cases(summary).to_string(),
        ),
        (
            "Discordant cases".to_owned(),
            (summary.paired.baseline_only_pass + summary.paired.candidate_only_pass).to_string(),
        ),
        (
            "Exact McNemar p-value".to_owned(),
            format!("{:.6}", summary.paired.mcnemar_exact_p),
        ),
        (
            "Bootstrap".to_owned(),
            format!(
                "{} samples, {:.1}% interval, seed {}",
                summary.bootstrap.samples,
                summary.bootstrap.confidence * 100.0,
                summary.bootstrap.seed
            ),
        ),
        (
            "Execution schedule".to_owned(),
            manifest.execution_schedule.clone(),
        ),
        (
            "Artifact format".to_owned(),
            manifest.artifact_format_version.to_string(),
        ),
        (
            "Report format".to_owned(),
            REPORT_FORMAT_VERSION.to_string(),
        ),
    ];
    let mut environment = Environment::new();
    environment.set_auto_escape_callback(|_| AutoEscape::Html);
    environment.add_template("report.html", TEMPLATE)?;
    let html = environment.get_template("report.html")?.render(view)?;
    std::fs::create_dir_all(destination)?;
    atomic_write(&destination.join("index.html"), html.as_bytes())?;
    atomic_write(&destination.join("case-index.json"), b"[]")
}

fn jointly_scored_cases(summary: &RunSummary) -> usize {
    summary.primary_jointly_scored
}

fn field_states(counts: &EvaluatorStateCounts) -> String {
    format!(
        "pass {} · fail {} · error {} · n/a {} · unscored {}",
        counts.passed, counts.failed, counts.error, counts.not_applicable, counts.unscored
    )
}

/// Serve a report on a random loopback-only port until interrupted.
pub async fn serve(run_dir: &Path, open_browser: bool) -> anyhow::Result<()> {
    let generated = finalized_report(run_dir)?;
    let directory = generated
        .index_path
        .parent()
        .context("generated report has no directory")?
        .to_owned();
    serve_assets(load_verified_report_assets(&directory)?, open_browser).await
}

/// Serve one escaped research index and its separate, verified study reports.
pub async fn serve_research(
    index_path: &Path,
    studies: &[(String, PathBuf)],
    open_browser: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(index_path.is_file(), "research index is missing");
    let mut assets = VerifiedReportAssets::new();
    assets.insert(
        "index.html".to_owned(),
        verified_asset(index_path, "text/html; charset=utf-8")?,
    );
    for (slug, run_dir) in studies {
        anyhow::ensure!(
            !slug.is_empty()
                && slug
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
            "unsafe research study slug `{slug}`"
        );
        let generated = finalized_report(run_dir)?;
        let report_dir = generated
            .index_path
            .parent()
            .context("generated study report has no directory")?;
        for (relative, asset) in load_verified_report_assets(report_dir)? {
            let key = format!("{slug}/{relative}");
            anyhow::ensure!(
                assets.insert(key, asset).is_none(),
                "duplicate research asset"
            );
        }
    }
    serve_assets(assets, open_browser).await
}

async fn serve_assets(assets: VerifiedReportAssets, open_browser: bool) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let token = capability_token();
    let expected_host = format!("127.0.0.1:{}", address.port());
    let url = format!("http://{expected_host}/{token}/");
    println!("StructTrace report: {url}");
    if open_browser {
        if let Err(error) = open::that(&url) {
            eprintln!(
                "warning: could not open the default browser ({error}); the verified report remains available at {url}"
            );
        }
    }
    let state = Arc::new(ReportServerState {
        token,
        expected_host,
        assets,
    });
    let service = axum::Router::new()
        .fallback(serve_verified_asset)
        .with_state(state);
    axum::serve(listener, service)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[derive(Debug)]
struct VerifiedReportAsset {
    content_type: String,
    path: PathBuf,
    blake3: String,
}

type VerifiedReportAssets = BTreeMap<String, VerifiedReportAsset>;

#[derive(Debug)]
struct ReportServerState {
    token: String,
    expected_host: String,
    assets: VerifiedReportAssets,
}

fn capability_token() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_verified_report_assets(directory: &Path) -> anyhow::Result<VerifiedReportAssets> {
    fn visit(
        root: &Path,
        directory: &Path,
        output: &mut VerifiedReportAssets,
    ) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            anyhow::ensure!(
                !file_type.is_symlink(),
                "report asset must not be a symlink"
            );
            if file_type.is_dir() {
                visit(root, &path, output)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let content_type = match path.extension().and_then(|value| value.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("json") => "application/json; charset=utf-8",
                    _ => "application/octet-stream",
                };
                output.insert(
                    relative,
                    VerifiedReportAsset {
                        content_type: content_type.to_owned(),
                        blake3: hash_file(&path)?,
                        path,
                    },
                );
            }
        }
        Ok(())
    }
    let mut assets = BTreeMap::new();
    visit(directory, directory, &mut assets)?;
    Ok(assets)
}

fn verified_asset(path: &Path, content_type: &str) -> anyhow::Result<VerifiedReportAsset> {
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink());
    Ok(VerifiedReportAsset {
        content_type: content_type.to_owned(),
        blake3: hash_file(path)?,
        path: path.to_owned(),
    })
}

async fn serve_verified_asset(
    axum::extract::State(state): axum::extract::State<Arc<ReportServerState>>,
    headers: axum::http::HeaderMap,
    uri: axum::http::Uri,
) -> axum::response::Response {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok());
    if host != Some(state.expected_host.as_str()) {
        return axum::http::StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    let expected_origin = format!("http://{}", state.expected_host);
    for header in [axum::http::header::ORIGIN, axum::http::header::REFERER] {
        if let Some(value) = headers.get(header).and_then(|value| value.to_str().ok()) {
            if value != expected_origin
                && !value
                    .strip_prefix(&expected_origin)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            {
                return axum::http::StatusCode::FORBIDDEN.into_response();
            }
        }
    }
    let path = uri.path().trim_start_matches('/');
    let Some((provided_token, requested)) = path.split_once('/') else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    if provided_token != state.token {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    let requested = if requested.is_empty() {
        "index.html"
    } else {
        requested
    };
    if requested.contains("..") || requested.contains('\\') {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    let Some(asset) = state.assets.get(requested) else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    if hash_file(&asset.path).ok().as_deref() != Some(asset.blake3.as_str()) {
        return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let file = match tokio::fs::File::open(&asset.path).await {
        Ok(file) => file,
        Err(_) => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut response = axum::body::Body::from_stream(stream).into_response();
    let response_headers = response.headers_mut();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        asset
            .content_type
            .parse()
            .expect("static content type is valid"),
    );
    response_headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response_headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    response_headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("no-referrer"),
    );
    response_headers.insert(
        "content-security-policy",
        axum::http::HeaderValue::from_static(
            "default-src 'none'; connect-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'",
        ),
    );
    response_headers.insert(
        "cross-origin-resource-policy",
        axum::http::HeaderValue::from_static("same-origin"),
    );
    response
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
    for relative in manifest
        .artifacts
        .keys()
        .filter(|relative| relative.starts_with("report/"))
    {
        verify_bound_artifact(run_dir, &manifest, relative)?;
    }
    let expected = manifest
        .artifacts
        .keys()
        .filter(|relative| relative.starts_with("report/"))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut observed = std::collections::BTreeSet::new();
    collect_report_files(run_dir, &run_dir.join("report"), &mut observed)?;
    anyhow::ensure!(
        observed == expected,
        "report directory contains files that are missing, unbound, or not allowlisted"
    );
    Ok(GeneratedReport { index_path })
}

fn collect_report_files(
    run_dir: &Path,
    directory: &Path,
    output: &mut std::collections::BTreeSet<String>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        anyhow::ensure!(!file_type.is_symlink(), "report contains a symbolic link");
        if file_type.is_dir() {
            collect_report_files(run_dir, &path, output)?;
        } else if file_type.is_file() {
            output.insert(
                path.strip_prefix(run_dir)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        } else {
            anyhow::bail!("report contains a non-regular filesystem entry");
        }
    }
    Ok(())
}

fn verify_bound_artifact(
    run_dir: &Path,
    manifest: &RunManifest,
    relative: &str,
) -> anyhow::Result<()> {
    let relative_path = Path::new(relative);
    anyhow::ensure!(
        !relative_path.is_absolute()
            && relative_path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
        "manifest contains unsafe artifact path `{relative}`"
    );
    let expected = manifest
        .artifacts
        .get(relative)
        .with_context(|| format!("manifest does not bind `{relative}`"))?;
    let canonical_root = run_dir.canonicalize()?;
    let mut path = canonical_root.clone();
    for component in relative_path.components() {
        let std::path::Component::Normal(component) = component else {
            unreachable!()
        };
        path.push(component);
        let metadata = std::fs::symlink_metadata(&path)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "bound artifact contains a symlink: {relative}"
        );
    }
    anyhow::ensure!(path.is_file(), "bound artifact is missing: {relative}");
    anyhow::ensure!(
        path.canonicalize()?.starts_with(canonical_root),
        "bound artifact escaped run directory: {relative}"
    );
    let observed = hash_file(&path)?;
    anyhow::ensure!(
        &observed == expected,
        "bound artifact hash mismatch for `{relative}`"
    );
    Ok(())
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
    let structural_rows = summary_rows(&summary.baseline, &summary.candidate);
    let descriptive_rows = summary_rows(
        &summary.descriptive_baseline,
        &summary.descriptive_candidate,
    );
    let evaluator_rows = summary
        .evaluator_passes
        .iter()
        .map(|(id, counts)| EvaluatorRowView {
            id: id.clone(),
            baseline: EvaluatorCountsView {
                pass: counts.baseline.passed,
                fail: counts.baseline.failed,
                error: counts.baseline.error,
                not_applicable: counts.baseline.not_applicable,
                unscored: counts.baseline.unscored,
            },
            candidate: EvaluatorCountsView {
                pass: counts.candidate.passed,
                fail: counts.candidate.failed,
                error: counts.candidate.error,
                not_applicable: counts.candidate.not_applicable,
                unscored: counts.candidate.unscored,
            },
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
        gate_label: summary.gate.status.label().to_owned(),
        gate_class: match summary.gate.status {
            GateStatus::Passed => "pass",
            GateStatus::Failed | GateStatus::Error => "fail",
            GateStatus::NotConfigured | GateStatus::InsufficientEvidence => "warn",
        }
        .to_owned(),
        deployment_authorized: summary.gate.deployment_authorized,
        has_quality_failures: !summary.gate.quality_failures.is_empty(),
        has_evidence_failures: !summary.gate.evidence_failures.is_empty(),
        has_runtime_errors: !summary.gate.runtime_errors.is_empty(),
        default_filter: match config.report.default_case_filter.as_str() {
            "baseline_only_pass"
            | "candidate_only_pass"
            | "both_fail"
            | "valid_but_wrong"
            | "parse_failure"
            | "schema_failure"
            | "adapter_error"
            | "evaluator_error"
            | "not_applicable"
            | "unscored"
            | "discordant" => config.report.default_case_filter.clone(),
            _ => "all".to_owned(),
        },
        difference: format!("{:+.2} pp", summary.paired.difference_pp),
        interval: format!(
            "[{:.2}, {:.2}] pp",
            summary.bootstrap.lower_pp, summary.bootstrap.upper_pp
        ),
        total_rows: summary.evidence.total_rows,
        unique_cases: summary.evidence.singleton_evidence_units
            + summary.evidence.exact_duplicate_groups
            + summary.evidence.repeated_trial_groups,
        duplicate_groups: summary.evidence.exact_duplicate_groups,
        largest_duplicate_group: summary.evidence.largest_group,
        repeated_trial_groups: summary.evidence.repeated_trial_groups,
        label_conflict_groups: summary.evidence.label_conflict_groups,
        inference_unit: summary.evidence.inference_policy.clone(),
        evidence_denominator: summary.evidence.effective_inference_units,
        semantic_jointly_scored: summary.jointly_scored_semantic.jointly_scored_cases,
        semantic_excluded: summary.jointly_scored_semantic.excluded_pairs,
        semantic_difference: format!(
            "{:+.2} pp",
            summary.jointly_scored_semantic.paired.difference_pp
        ),
        baseline_primary: metric(summary.baseline.primary_pass, summary.baseline.total),
        candidate_primary: metric(summary.candidate.primary_pass, summary.candidate.total),
        structural_rows,
        descriptive_rows,
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
                let (state, class) = match rule.status {
                    GateRuleStatus::Passed => ("passed", "pass"),
                    GateRuleStatus::Failed => ("failed", "fail"),
                    GateRuleStatus::NotConfigured => ("not configured", "neutral"),
                    GateRuleStatus::InsufficientEvidence => ("insufficient evidence", "warn"),
                    GateRuleStatus::Error => ("error", "fail"),
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
            .primary_field_hotspots
            .iter()
            .map(|item| HotspotView {
                evaluator_id: item.evaluator_id.clone(),
                pointer: item.pointer.clone(),
                regressions: item.regressions,
                improvements: item.improvements,
                failures: item.candidate_failures,
                baseline_states: field_states(&item.baseline),
                candidate_states: field_states(&item.candidate),
            })
            .collect(),
        diagnostic_hotspots: summary
            .all_evaluator_field_diagnostics
            .iter()
            .map(|item| HotspotView {
                evaluator_id: item.evaluator_id.clone(),
                pointer: item.pointer.clone(),
                regressions: item.regressions,
                improvements: item.improvements,
                failures: item.candidate_failures,
                baseline_states: field_states(&item.baseline),
                candidate_states: field_states(&item.candidate),
            })
            .collect(),
        cases,
        embedded_cases_json: None,
        share_derivative: false,
        manifest_rows: vec![
            ("Run ID".to_owned(), manifest.run_id.clone()),
            (
                "Run kind".to_owned(),
                serde_json::to_value(manifest.run_kind)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned(),
            ),
            (
                "StructTrace version".to_owned(),
                manifest.structtrace_version.clone(),
            ),
            (
                "Primary outcome".to_owned(),
                summary.primary_outcome.clone(),
            ),
            (
                "Evaluation definition".to_owned(),
                compact_json(&manifest.evaluation_definition),
            ),
            (
                "Total paired cases".to_owned(),
                summary.evidence.total_rows.to_string(),
            ),
            (
                "Configured evidence units".to_owned(),
                (summary.evidence.singleton_evidence_units
                    + summary.evidence.exact_duplicate_groups
                    + summary.evidence.repeated_trial_groups)
                    .to_string(),
            ),
            (
                "Exact duplicate groups".to_owned(),
                summary.evidence.exact_duplicate_groups.to_string(),
            ),
            (
                "Inference policy".to_owned(),
                summary.evidence.inference_policy.clone(),
            ),
            (
                "Repeated-trial groups".to_owned(),
                summary.evidence.repeated_trial_groups.to_string(),
            ),
            (
                "Label-conflict groups".to_owned(),
                summary.evidence.label_conflict_groups.to_string(),
            ),
            (
                "Primary scored cases".to_owned(),
                format!(
                    "baseline {} / {}, candidate {} / {}",
                    summary.baseline.primary_pass + summary.baseline.primary_failed,
                    summary.baseline.total,
                    summary.candidate.primary_pass + summary.candidate.primary_failed,
                    summary.candidate.total
                ),
            ),
            (
                "Jointly scored cases".to_owned(),
                format!(
                    "{} / {}",
                    summary.primary_jointly_scored, summary.evidence.effective_inference_units
                ),
            ),
            (
                "Semantic exclusion reasons".to_owned(),
                compact_json(&serde_json::to_value(
                    &summary.jointly_scored_semantic.exclusion_reasons,
                )?),
            ),
            (
                "Discordant cases".to_owned(),
                (summary.paired.baseline_only_pass + summary.paired.candidate_only_pass)
                    .to_string(),
            ),
            (
                "Exact McNemar p-value".to_owned(),
                format!("{:.6}", summary.paired.mcnemar_exact_p),
            ),
            (
                "Bootstrap".to_owned(),
                format!(
                    "{} samples, {:.1}% interval, seed {}",
                    summary.bootstrap.samples,
                    summary.bootstrap.confidence * 100.0,
                    summary.bootstrap.seed
                ),
            ),
            (
                "Execution schedule".to_owned(),
                manifest.execution_schedule.clone(),
            ),
            (
                "Implementation fingerprint".to_owned(),
                manifest
                    .implementation_fingerprint
                    .clone()
                    .unwrap_or_else(|| {
                        "recorded outputs; source artifacts are hash-bound".to_owned()
                    }),
            ),
            (
                "Variant definitions".to_owned(),
                compact_json(&manifest.variants),
            ),
            (
                "Artifact format".to_owned(),
                manifest.artifact_format_version.to_string(),
            ),
            (
                "Report format".to_owned(),
                REPORT_FORMAT_VERSION.to_string(),
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

fn summary_rows(baseline: &VariantSummary, candidate: &VariantSummary) -> Vec<ComparisonRow> {
    vec![
        comparison(
            "Strict JSON",
            baseline.parse_valid,
            candidate.parse_valid,
            baseline.total,
        ),
        comparison(
            "Schema valid",
            baseline.schema_valid,
            candidate.schema_valid,
            baseline.total,
        ),
        comparison(
            "Primary outcome pass",
            baseline.primary_pass,
            candidate.primary_pass,
            baseline.total,
        ),
        comparison(
            "Primary fully evaluated",
            baseline.primary_fully_evaluated,
            candidate.primary_fully_evaluated,
            baseline.total,
        ),
        comparison_with_denominators(
            "Primary component errors",
            baseline.primary_component_errors,
            baseline.primary_required_components,
            candidate.primary_component_errors,
            candidate.primary_required_components,
        ),
        comparison(
            "Explicit primary failure",
            baseline.primary_failed,
            candidate.primary_failed,
            baseline.total,
        ),
        comparison(
            "Primary evaluator error",
            baseline.primary_error,
            candidate.primary_error,
            baseline.total,
        ),
        comparison(
            "Primary not applicable",
            baseline.primary_not_applicable,
            candidate.primary_not_applicable,
            baseline.total,
        ),
        comparison(
            "Primary unscored",
            baseline.primary_unscored,
            candidate.primary_unscored,
            baseline.total,
        ),
        comparison(
            "Valid but wrong",
            baseline.valid_but_wrong,
            candidate.valid_but_wrong,
            baseline.total,
        ),
        comparison(
            "Fully evaluated valid but wrong",
            baseline.fully_evaluated_valid_but_wrong,
            candidate.fully_evaluated_valid_but_wrong,
            baseline.total,
        ),
        comparison(
            "Adapter error or missing output",
            baseline.errors,
            candidate.errors,
            baseline.total,
        ),
    ]
}

fn directory_size(path: &Path) -> anyhow::Result<u64> {
    let mut total = 0_u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        total = total.saturating_add(if metadata.is_dir() {
            directory_size(&entry.path())?
        } else {
            metadata.len()
        });
    }
    Ok(total)
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
        "model_visible_metadata": record.case.model_visible_metadata,
        "metadata": record.case.metadata,
    });
    let secrets = selected_values(&source, &config.storage.redaction.json_pointers);
    let aggressive = matches!(
        config.storage.redaction.text_mode,
        TextRedactionMode::AggressiveTextual
    );
    let patterns = &config.storage.redaction.custom_patterns;
    let mut redacted_value = serde_json::to_value(record)
        .context("could not build fail-closed report view for a case")?;
    redact_matching_values_with_policy(&mut redacted_value, &secrets, aggressive, patterns);
    redact_value_raw_text(
        &mut redacted_value,
        "/baseline_output/raw_output",
        &secrets,
        aggressive,
        patterns,
    );
    redact_value_raw_text(
        &mut redacted_value,
        "/candidate_output/raw_output",
        &secrets,
        aggressive,
        patterns,
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
    let primary = &config.analysis.primary_outcome;
    let primary_results = [
        record.baseline_evaluation.outcomes.get(primary),
        record.candidate_evaluation.outcomes.get(primary),
    ];
    if primary_results.iter().flatten().any(|result| {
        result.truth == structtrace_core::evaluation::OutcomeStatus::Error
            || result.error_components > 0
    }) {
        filters.push("evaluator_error".to_owned());
    }
    if primary_results.iter().flatten().any(|result| {
        result.truth == structtrace_core::evaluation::OutcomeStatus::NotApplicable
            || result.not_applicable_components > 0
    }) {
        filters.push("not_applicable".to_owned());
    }
    if primary_results.contains(&None)
        || primary_results
            .iter()
            .flatten()
            .any(|result| result.unscored_components > 0)
    {
        filters.push("unscored".to_owned());
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
    redact_text_with_policy(&mut filter_string, &secrets, aggressive, patterns);
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
        model_visible_metadata: optional_pretty(&redacted_value, "/case/model_visible_metadata"),
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

fn redact_value_raw_text(
    value: &mut Value,
    pointer: &str,
    secrets: &[Value],
    aggressive: bool,
    patterns: &[String],
) {
    let raw = value
        .pointer_mut(pointer)
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let Some(mut raw) = raw else {
        return;
    };
    redact_text_with_policy(&mut raw, secrets, aggressive, patterns);
    if let Some(target) = value.pointer_mut(pointer) {
        *target = Value::String(raw);
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

fn comparison_with_denominators(
    label: &str,
    baseline: usize,
    baseline_total: usize,
    candidate: usize,
    candidate_total: usize,
) -> ComparisonRow {
    ComparisonRow {
        label: label.to_owned(),
        baseline: metric(baseline, baseline_total),
        candidate: metric(candidate, candidate_total),
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
    structtrace_core::strict_json::from_slice(&bytes)
        .with_context(|| format!("invalid JSON in {}", path.display()))
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
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; connect-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'">
  <meta name="color-scheme" content="light dark">
  <title>{{ project_name }} · StructTrace</title>
  <style>
    :root { color-scheme: light dark; --bg:#f6f8fb; --surface:#fff; --ink:#142238; --muted:#5c6b7e; --line:#d9e1eb; --blue:#155eef; --green:#087a55; --red:#b42318; --amber:#a15c00; --code:#0e1a2a; }
    @media (prefers-color-scheme:dark){:root{--bg:#0b1017;--surface:#111a26;--ink:#edf3fa;--muted:#a8b4c4;--line:#29384c;--blue:#70a0ff;--green:#5ed6aa;--red:#ff8c86;--amber:#ffc46b;--code:#070c12}}
    *{box-sizing:border-box} body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif} a{color:var(--blue)}
    header{background:#0b2138;color:#fff;border-bottom:1px solid #29425e} .bar{max-width:1180px;margin:auto;padding:18px 24px;display:flex;align-items:center;justify-content:space-between;gap:20px}.brand{font-weight:800;letter-spacing:.01em}.tagline{color:#c5d5e8;font-size:13px}
    main{max-width:1180px;margin:auto;padding:28px 24px 80px} h1{font-size:clamp(30px,5vw,48px);line-height:1.05;margin:.15em 0} h2{margin-top:48px;font-size:24px} h3{font-size:17px}.eyebrow{text-transform:uppercase;letter-spacing:.12em;font-weight:800;font-size:11px;color:var(--blue)} .muted{color:var(--muted)}
    .hero{display:grid;grid-template-columns:1fr auto;gap:28px;align-items:end}.gate{padding:14px 18px;border-radius:12px;font-weight:900;letter-spacing:.08em}.gate.pass{background:#d8f5e8;color:#075b40}.gate.fail{background:#fee4e2;color:#8f1710}.gate.warn{background:#fff0c7;color:#714100}
    .metrics{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;margin:26px 0}.metric,.panel{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:18px;box-shadow:0 4px 18px rgba(20,34,56,.04)}.metric strong{font-size:26px;display:block}.metric span{color:var(--muted);font-size:12px}
    table{width:100%;border-collapse:collapse;background:var(--surface);border:1px solid var(--line)}th,td{text-align:left;padding:11px 13px;border-bottom:1px solid var(--line)}th{font-size:12px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted)}td.num{text-align:right;font-variant-numeric:tabular-nums}
    .matrix{display:grid;grid-template-columns:repeat(2,minmax(120px,1fr));max-width:520px;border:1px solid var(--line);border-radius:12px;overflow:hidden}.cell{padding:24px;border:1px solid var(--line);background:var(--surface)}.cell strong{display:block;font-size:30px}.cell.win{background:color-mix(in srgb,var(--green) 13%,var(--surface))}.cell.loss{background:color-mix(in srgb,var(--red) 13%,var(--surface))}
    .rule{display:grid;grid-template-columns:150px 1fr;gap:12px;padding:13px 0;border-bottom:1px solid var(--line)}.pill{font-size:11px;font-weight:800;text-transform:uppercase;letter-spacing:.06em}.pill.pass{color:var(--green)}.pill.fail{color:var(--red)}.pill.warn{color:var(--amber)}.pill.neutral{color:var(--muted)}
    .filters{display:flex;flex-wrap:wrap;gap:8px;margin:14px 0}.filters button,.pager button{border:1px solid var(--line);background:var(--surface);color:var(--ink);border-radius:999px;padding:7px 12px;cursor:pointer}.filters button[aria-pressed=true]{background:var(--blue);border-color:var(--blue);color:#fff}.case-tools{display:grid;grid-template-columns:minmax(220px,1fr) auto;gap:12px;align-items:center}.case-tools input{width:100%;padding:10px 12px;border:1px solid var(--line);border-radius:9px;background:var(--surface);color:var(--ink)}.pager{display:flex;align-items:center;justify-content:space-between;gap:12px;margin:16px 0}.pager-controls{display:flex;gap:8px}
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
  {% if share_derivative %}<div class="panel"><strong>Aggregate-only share derivative</strong><p class="muted">Case inputs, expected values, outputs, prompts, adapter metadata, and evaluation metadata were deliberately omitted. Use the hash-bound local run for case-level audit.</p></div>{% endif %}
  <section class="metrics" aria-label="Executive summary">
    <div class="metric"><span>Baseline primary outcome · evidence units</span><strong>{{ baseline_primary.percent }}</strong>{{ baseline_primary.count }}/{{ baseline_primary.total }}</div>
    <div class="metric"><span>Candidate primary outcome · evidence units</span><strong>{{ candidate_primary.percent }}</strong>{{ candidate_primary.count }}/{{ candidate_primary.total }}</div>
    <div class="metric"><span>Independent paired difference</span><strong>{{ difference }}</strong>candidate minus baseline</div>
    <div class="metric"><span>Independent bootstrap interval</span><strong>{{ interval }}</strong>one configured evidence unit per pair</div>
  </section>

  <h2>Evidence independence</h2>
  <p class="muted">Exact duplicates remain visible descriptively but count once. Repeated trials are never resolved by row order or case ID; v1 excludes them from independent inference and makes the gate insufficient. Label conflicts fail before execution.</p>
  <table><thead><tr><th>Captured rows</th><th>Evidence groups</th><th>Inference units</th><th>Exact duplicate groups</th><th>Repeated trials</th><th>Label conflicts</th><th>Largest group</th></tr></thead><tbody><tr><td class="num">{{ total_rows }}</td><td class="num">{{ unique_cases }}</td><td class="num">{{ evidence_denominator }}</td><td class="num">{{ duplicate_groups }}</td><td class="num">{{ repeated_trial_groups }}</td><td class="num">{{ label_conflict_groups }}</td><td class="num">{{ largest_duplicate_group }}</td></tr></tbody></table>
  <p class="muted">Inference policy: <code>{{ inference_unit }}</code></p>

  <h3>Descriptive execution totals</h3>
  <p class="muted">All captured rows, including repeats. These values make no independence claim and are not used by the release gate.</p>
  <table><thead><tr><th>Metric across all captured rows</th><th>Baseline</th><th>Candidate</th></tr></thead><tbody>{% for row in descriptive_rows %}<tr><td>{{ row.label }}</td><td class="num">{{ row.baseline.count }}/{{ row.baseline.total }} · {{ row.baseline.percent }}</td><td class="num">{{ row.candidate.count }}/{{ row.candidate.total }} · {{ row.candidate.percent }}</td></tr>{% endfor %}</tbody></table>

  <h2>Deployment success versus semantic effect</h2>
  <p class="muted">The release gate uses complete-denominator deployment success. The semantic-only estimate includes only independent pairs where both primary outcomes explicitly resolved to true or false; operational failures are not relabeled as semantic errors.</p>
  <table><thead><tr><th>Estimate</th><th>Included pairs</th><th>Excluded operational/error pairs</th><th>Candidate minus baseline</th></tr></thead><tbody><tr><td>Jointly scored semantic effect</td><td class="num">{{ semantic_jointly_scored }}</td><td class="num">{{ semantic_excluded }}</td><td class="num">{{ semantic_difference }}</td></tr></tbody></table>

  {% if research_studies %}<section><h2>Accepted research matrices</h2><p class="muted">The same class of contract-preserving change had different effects across evaluated systems. These are compact normalized outcomes, not universal model rankings.</p><table><thead><tr><th>Study</th><th>Baseline correct</th><th>Candidate correct</th><th>Candidate-only</th><th>Baseline-only</th></tr></thead><tbody>{% for study in research_studies %}<tr><td>{{ study.label }}</td><td class="num">{{ study.baseline.count }}/{{ study.baseline.total }}</td><td class="num">{{ study.candidate.count }}/{{ study.candidate.total }}</td><td class="num">{{ study.candidate_only }}</td><td class="num">{{ study.baseline_only }}</td></tr>{% endfor %}</tbody></table></section>{% endif %}

  <h2>Structural validity versus correctness</h2>
  <p class="muted">Validity and correctness are deliberately separate. A schema-valid output can still fail the configured semantic or executable outcome.</p>
  <table><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th></tr></thead><tbody>{% for row in structural_rows %}<tr><td>{{ row.label }}</td><td class="num">{{ row.baseline.count }}/{{ row.baseline.total }} · {{ row.baseline.percent }}</td><td class="num">{{ row.candidate.count }}/{{ row.candidate.total }} · {{ row.candidate.percent }}</td></tr>{% endfor %}</tbody></table>

  <h2>Independent deployment-success transition matrix</h2>
  <div class="matrix" aria-label="Paired transition matrix"><div class="cell"><span>Both pass</span><strong>{{ transition.both_pass }}</strong></div><div class="cell loss"><span>Baseline-only pass</span><strong>{{ transition.baseline_only }}</strong></div><div class="cell win"><span>Candidate-only pass</span><strong>{{ transition.candidate_only }}</strong></div><div class="cell"><span>Both fail</span><strong>{{ transition.both_fail }}</strong></div></div>

  <h2>Release gate</h2><div class="panel"><strong>{% if deployment_authorized %}DEPLOYMENT AUTHORIZED{% else %}DO NOT DEPLOY{% endif %}</strong>{% if has_quality_failures %}<p>Quality threshold failed.</p>{% endif %}{% if has_evidence_failures %}<p>Evidence requirements are also insufficient.</p>{% endif %}{% if has_runtime_errors %}<p>One or more rules could not be evaluated safely.</p>{% endif %}{% if gate_rules %}{% for rule in gate_rules %}<div class="rule"><div class="pill {{ rule.class }}">{{ rule.state }}</div><div><strong>{{ rule.name }}</strong><br><span class="muted">{{ rule.message }}</span></div></div>{% endfor %}{% else %}<strong>No release criteria were configured.</strong><p class="muted">This run was analyzed, but StructTrace cannot make a deployment decision.</p>{% endif %}</div>

  <h2>Evaluator results</h2><p class="muted">Every evaluator state remains explicit; errors, not-applicable results, and missing scores are never folded into semantic failure.</p><table><thead><tr><th rowspan="2">Evaluator</th><th colspan="5">Baseline</th><th colspan="5">Candidate</th></tr><tr><th>Pass</th><th>Fail</th><th>Error</th><th>N/A</th><th>Unscored</th><th>Pass</th><th>Fail</th><th>Error</th><th>N/A</th><th>Unscored</th></tr></thead><tbody>{% for row in evaluator_rows %}<tr><td><code>{{ row.id }}</code></td><td class="num">{{ row.baseline.pass }}</td><td class="num">{{ row.baseline.fail }}</td><td class="num">{{ row.baseline.error }}</td><td class="num">{{ row.baseline.not_applicable }}</td><td class="num">{{ row.baseline.unscored }}</td><td class="num">{{ row.candidate.pass }}</td><td class="num">{{ row.candidate.fail }}</td><td class="num">{{ row.candidate.error }}</td><td class="num">{{ row.candidate.not_applicable }}</td><td class="num">{{ row.candidate.unscored }}</td></tr>{% endfor %}</tbody></table>

  <h2>Operational comparison</h2><p class="muted">Latency is descriptive unless a threshold is configured. Costs are shown only from explicit adapter pricing and are never inferred.</p><table><thead><tr><th>Metric</th><th>Baseline</th><th>Candidate</th></tr></thead><tbody>{% for row in operational_rows %}<tr><td>{{ row.label }}</td><td class="num">{{ row.baseline }}</td><td class="num">{{ row.candidate }}</td></tr>{% endfor %}</tbody></table>

  <h2>Primary-outcome field hotspots</h2><p class="muted">Only evaluators reachable from the selected primary outcome are included.</p>{% if hotspots %}<table><thead><tr><th>Evaluator</th><th>JSON Pointer</th><th>Candidate regressions</th><th>Candidate improvements</th><th>Baseline states</th><th>Candidate states</th></tr></thead><tbody>{% for item in hotspots %}<tr><td><code>{{ item.evaluator_id }}</code></td><td><code>{{ item.pointer }}</code></td><td class="num">{{ item.regressions }}</td><td class="num">{{ item.improvements }}</td><td>{{ item.baseline_states }}</td><td>{{ item.candidate_states }}</td></tr>{% endfor %}</tbody></table>{% else %}<p class="empty">The primary outcome has no field-level facts.</p>{% endif %}
  <h2>All-evaluator field diagnostics</h2><p class="muted">Diagnostic only. Evaluator identities and pass, fail, error, not-applicable, and unscored states remain separate.</p>{% if diagnostic_hotspots %}<table><thead><tr><th>Evaluator</th><th>JSON Pointer</th><th>Regressions</th><th>Improvements</th><th>Baseline states</th><th>Candidate states</th></tr></thead><tbody>{% for item in diagnostic_hotspots %}<tr><td><code>{{ item.evaluator_id }}</code></td><td><code>{{ item.pointer }}</code></td><td class="num">{{ item.regressions }}</td><td class="num">{{ item.improvements }}</td><td>{{ item.baseline_states }}</td><td>{{ item.candidate_states }}</td></tr>{% endfor %}</tbody></table>{% else %}<p class="empty">No field-level evaluator diagnostics are available.</p>{% endif %}

  {% if not share_derivative %}<h2>Case explorer</h2>
  <p class="muted">Case details are loaded in bounded chunks. Search covers case IDs and redacted metadata; filters include outcome, validity, adapter, and evaluator states.</p>
  <div class="case-tools"><label><span class="sr-only">Search cases</span><input id="case-search" type="search" placeholder="Search case ID or metadata"></label><span id="case-count" class="muted">Loading case index…</span></div>
  <div class="filters" role="group" aria-label="Case filters"><button data-filter="all" aria-pressed="false">All</button><button data-filter="discordant" aria-pressed="false">Discordant</button><button data-filter="baseline_only_pass" aria-pressed="false">Baseline-only</button><button data-filter="candidate_only_pass" aria-pressed="false">Candidate-only</button><button data-filter="both_fail" aria-pressed="false">Both fail</button><button data-filter="valid_but_wrong" aria-pressed="false">Valid but wrong</button><button data-filter="parse_failure" aria-pressed="false">Parse failures</button><button data-filter="schema_failure" aria-pressed="false">Schema failures</button><button data-filter="adapter_error" aria-pressed="false">Adapter errors</button><button data-filter="evaluator_error" aria-pressed="false">Evaluator errors</button><button data-filter="not_applicable" aria-pressed="false">Not applicable</button><button data-filter="unscored" aria-pressed="false">Unscored</button></div>
  <div id="cases" aria-live="polite"></div>
  <div class="pager"><span id="page-status" class="muted"></span><div class="pager-controls"><button id="previous-page" type="button">Previous</button><button id="next-page" type="button">Next</button></div></div>{% endif %}

  <h2>Reproducibility</h2><div class="repro">{% for row in manifest_rows %}<strong>{{ row.0 }}</strong><code>{{ row.1 }}</code>{% endfor %}</div><p><code>structtrace replay {{ run_id }}</code></p>
  <footer>Generated locally by StructTrace. No telemetry, external assets, or analytics.</footer>
</main>
{% if not share_derivative %}<script>
  const embeddedCases={% if embedded_cases_json %}JSON.parse({{ embedded_cases_json|tojson }}){% else %}null{% endif %};
  const buttons=[...document.querySelectorAll('[data-filter]')], container=document.getElementById('cases'), search=document.getElementById('case-search'), count=document.getElementById('case-count'), pageStatus=document.getElementById('page-status');
  const cache=new Map(), pageSize=25;let index=[],filtered=[],activeFilter='{{ default_filter }}',page=0;
  const element=(name,className,text)=>{const node=document.createElement(name);if(className)node.className=className;if(text!==undefined)node.textContent=text;return node};
  const section=(title,value)=>{const node=element('section');node.append(element('h3','',title),element('pre','code',value));return node};
  const grid=(...children)=>{const node=element('div','case-grid');node.append(...children);return node};
  function renderCase(item){const root=element('details','case'),head=element('summary','',`${item.id} · ${item.transition}`),body=element('div','case-body');root.append(head,body);body.append(grid(section('Input',item.input),section('Expected',item.expected)),grid(section(`Baseline raw · ${item.baseline_latency}`,item.baseline_raw),section(`Candidate raw · ${item.candidate_latency}`,item.candidate_raw)),grid(section('Baseline parsed',item.baseline_parsed),section('Candidate parsed',item.candidate_parsed)),element('h3','','Structured diff'));
    if(item.diffs.length){const diff=element('div','diff');for(const label of ['Path','Change','Baseline','Candidate'])diff.append(element('div','head',label));for(const row of item.diffs)diff.append(element('code','',row.path),element('span','',row.kind),element('code','',row.baseline),element('code','',row.candidate));body.append(diff)}else body.append(element('p','empty','Parsed outputs are identical.'));
    body.append(grid(section('Baseline schema errors',item.baseline_schema_errors),section('Candidate schema errors',item.candidate_schema_errors)),grid(section('Baseline evaluators',item.baseline_evaluators),section('Candidate evaluators',item.candidate_evaluators)),grid(section('Baseline execution evidence',item.baseline_execution),section('Candidate execution evidence',item.candidate_execution)),grid(section('Baseline adapter metadata',item.baseline_metadata),section('Candidate adapter metadata',item.candidate_metadata)),grid(section('Model-visible metadata',item.model_visible_metadata),section('Evaluation-only metadata',item.metadata)));return root}
  async function chunk(number){if(embeddedCases)return embeddedCases.slice(number*50,number*50+50);if(!cache.has(number))cache.set(number,fetch(`cases/${String(number).padStart(5,'0')}.json`).then(response=>{if(!response.ok)throw new Error(`case chunk ${number} could not be loaded`);return response.json()}));return cache.get(number)}
  async function render(){const start=page*pageSize, rows=filtered.slice(start,start+pageSize), fragments=[];for(const entry of rows){const values=await chunk(entry.chunk);fragments.push(renderCase(values[entry.offset]))}container.replaceChildren(...fragments);count.textContent=`${filtered.length} of ${index.length} cases`;pageStatus.textContent=filtered.length?`Page ${page+1} of ${Math.ceil(filtered.length/pageSize)}`:'No matching cases';document.getElementById('previous-page').disabled=page===0;document.getElementById('next-page').disabled=start+pageSize>=filtered.length}
  function apply(){const query=search.value.trim().toLowerCase();filtered=index.filter(item=>(activeFilter==='all'||item.filters.split(' ').includes(activeFilter))&&(!query||item.search.includes(query)));page=0;render().catch(error=>{container.textContent=error.message})}
  for(const button of buttons)button.addEventListener('click',()=>{activeFilter=button.dataset.filter;for(const item of buttons)item.setAttribute('aria-pressed',String(item===button));apply()});search.addEventListener('input',apply);document.getElementById('previous-page').addEventListener('click',()=>{if(page>0){page--;render()}});document.getElementById('next-page').addEventListener('click',()=>{if((page+1)*pageSize<filtered.length){page++;render()}});
  (async()=>{index=embeddedCases?embeddedCases.map((item,position)=>({id:item.id,transition:item.transition,filters:item.filters,search:`${item.id} ${item.metadata} ${item.model_visible_metadata} ${item.filters}`.toLowerCase(),chunk:Math.floor(position/50),offset:position%50})):await fetch('case-index.json').then(response=>response.json());const initial=buttons.find(button=>button.dataset.filter===activeFilter)||buttons[0];initial.click()})().catch(error=>{container.textContent=`Report data could not be loaded: ${error.message}`});
</script>{% endif %}
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
            "version": 2,
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
        passing_record_with_expected(id, input, json!({"answer": "ok"}), None)
    }

    fn passing_record_with_expected(
        id: String,
        input: Value,
        expected: Value,
        metadata: Option<Value>,
    ) -> PairedCaseRecord {
        let case = Case {
            id,
            input,
            expected: Some(expected.clone()),
            model_visible_metadata: None,
            metadata,
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
        let summary = summary_for(total);
        let manifest = RunManifest::new("report-run".to_owned(), "report-test".to_owned());
        let mut view = build_view(&summary, &manifest, records, config).unwrap();
        let cases = std::mem::take(&mut view.cases);
        view.embedded_cases_json = Some(serde_json::to_string(&cases).unwrap());
        let mut environment = Environment::new();
        environment.set_auto_escape_callback(|_| AutoEscape::Html);
        environment.add_template("report.html", TEMPLATE).unwrap();
        environment
            .get_template("report.html")
            .unwrap()
            .render(view)
            .unwrap()
    }

    fn summary_for(total: usize) -> RunSummary {
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
        RunSummary {
            artifact_format_version: structtrace_core::ARTIFACT_FORMAT_VERSION,
            run_id: "report-run".to_owned(),
            primary_outcome: "correct".to_owned(),
            baseline: variant.clone(),
            candidate: variant.clone(),
            descriptive_baseline: variant.clone(),
            descriptive_candidate: variant,
            primary_jointly_scored: total,
            evidence: structtrace_core::artifact::EvidenceSummary {
                total_rows: total,
                singleton_evidence_units: total,
                exact_duplicate_groups: 0,
                repeated_trial_groups: 0,
                label_conflict_groups: 0,
                exact_duplicate_rows: 0,
                largest_group: usize::from(total > 0),
                exact_duplicate_row_rate: 0.0,
                effective_inference_units: total,
                inference_policy: "fingerprint:/input,/expected,/model_visible_metadata".to_owned(),
                groups: Vec::new(),
            },
            independent_paired: paired.clone(),
            independent_bootstrap: BootstrapInterval {
                lower_pp: 0.0,
                upper_pp: 0.0,
                confidence: 0.95,
                samples: 100,
                seed: 17,
            },
            jointly_scored_semantic: structtrace_core::artifact::SemanticEffectSummary {
                jointly_scored_cases: total,
                excluded_pairs: 0,
                exclusion_reasons: BTreeMap::new(),
                paired: paired.clone(),
                bootstrap: None,
            },
            matched_operational: Default::default(),
            descriptive_matched_operational: Default::default(),
            paired,
            bootstrap: BootstrapInterval {
                lower_pp: 0.0,
                upper_pp: 0.0,
                confidence: 0.95,
                samples: 100,
                seed: 17,
            },
            gate: GateDecision {
                status: GateStatus::Passed,
                deployment_authorized: true,
                quality_failures: Vec::new(),
                evidence_failures: Vec::new(),
                runtime_errors: Vec::new(),
                rules: vec![],
            },
            evaluator_passes: BTreeMap::new(),
            field_hotspots: vec![],
            primary_field_hotspots: vec![],
            all_evaluator_field_diagnostics: vec![],
        }
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
        assert!(html.contains("The primary outcome has no field-level facts."));
        assert!(html.contains("No field-level evaluator diagnostics are available."));
    }

    #[test]
    fn share_directory_omits_all_user_controlled_provenance_sentinels() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("share");
        let mut config = report_config("SECRET_TITLE_91");
        config.project.name = "SECRET_PROJECT_92".to_owned();
        config.dataset.path = PathBuf::from("SECRET_DATASET_PATH_93");
        config.schema.path = PathBuf::from("SECRET_SCHEMA_PATH_94");
        let summary = summary_for(10);
        let mut manifest = RunManifest::new(
            "report-run".to_owned(),
            "SECRET_MANIFEST_PROJECT_95".to_owned(),
        );
        manifest.variants = json!({
            "prompt": "SECRET_PROMPT_96",
            "command": ["SECRET_COMMAND_ARG_97"],
            "endpoint": "SECRET_ENDPOINT_98"
        });
        manifest.evaluation_definition = json!({
            "callable": "SECRET_EVALUATOR_PATH_99"
        });

        write_share_directory(&summary, &manifest, &config, &destination).unwrap();

        let sentinels = [
            "SECRET_TITLE_91",
            "SECRET_PROJECT_92",
            "SECRET_DATASET_PATH_93",
            "SECRET_SCHEMA_PATH_94",
            "SECRET_MANIFEST_PROJECT_95",
            "SECRET_PROMPT_96",
            "SECRET_COMMAND_ARG_97",
            "SECRET_ENDPOINT_98",
            "SECRET_EVALUATOR_PATH_99",
        ];
        for entry in std::fs::read_dir(&destination).unwrap() {
            let entry = entry.unwrap();
            let contents = std::fs::read(entry.path()).unwrap();
            for sentinel in sentinels {
                assert!(
                    !contents
                        .windows(sentinel.len())
                        .any(|window| window == sentinel.as_bytes()),
                    "{sentinel} leaked to {}",
                    entry.path().display()
                );
            }
        }
    }

    #[test]
    fn self_contained_report_preserves_every_case_in_a_large_set() {
        let config = report_config("large report");
        let records = (0..1_000)
            .map(|index| {
                passing_record_with_expected(
                    format!("invoice-{index:04}"),
                    json!({"document_text": format!("Invoice INV-{index:04} from Example Supply with two line items, exact subtotal, tax, and total.")}),
                    json!({
                        "invoice_number": format!("INV-{index:04}"),
                        "vendor_name": "Example Supply",
                        "currency": "USD",
                        "subtotal": "100.00",
                        "tax": "18.00",
                        "total": "118.00",
                        "line_items": [
                            {"description": "Paper", "quantity": 2, "unit_price": "40.00", "amount": "80.00"},
                            {"description": "Ink", "quantity": 1, "unit_price": "20.00", "amount": "20.00"}
                        ]
                    }),
                    Some(json!({"split": "scale-validation", "region": if index % 2 == 0 {"us"} else {"eu"}})),
                )
            })
            .collect::<Vec<_>>();
        let html = render_records(&config, &records);
        assert!(html.contains("invoice-0000"));
        assert!(html.contains("invoice-0999"));
        assert!(html.contains("pageSize=25"));
        assert!(html.contains("Search case ID or metadata"));
    }

    #[test]
    fn thousand_case_report_is_chunked_searchable_and_bounded() {
        let temporary = tempfile::tempdir().unwrap();
        let run_dir = temporary.path();
        std::fs::create_dir_all(run_dir.join("inputs")).unwrap();
        let mut config = report_config("large chunked report");
        config.limits.max_single_file_report_bytes = 1;
        let records = (0..1_000)
            .map(|index| {
                passing_record_with_expected(
                    format!("invoice-{index:04}"),
                    json!({"document_text": format!("Invoice INV-{index:04} from Example Supply with two line items, exact subtotal, tax, and total.")}),
                    json!({
                        "invoice_number": format!("INV-{index:04}"),
                        "vendor_name": "Example Supply",
                        "currency": "USD",
                        "subtotal": "100.00",
                        "tax": "18.00",
                        "total": "118.00",
                        "line_items": [
                            {"description": "Paper", "quantity": 2, "unit_price": "40.00", "amount": "80.00"},
                            {"description": "Ink", "quantity": 1, "unit_price": "20.00", "amount": "20.00"}
                        ]
                    }),
                    Some(json!({"split": "scale-validation", "region": if index % 2 == 0 {"us"} else {"eu"}})),
                )
            })
            .collect::<Vec<_>>();
        std::fs::write(
            run_dir.join("summary.json"),
            serde_json::to_vec(&summary_for(records.len())).unwrap(),
        )
        .unwrap();
        std::fs::write(
            run_dir.join("manifest.json"),
            serde_json::to_vec(&RunManifest::new(
                "report-run".to_owned(),
                "report-test".to_owned(),
            ))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            run_dir.join("inputs/configuration.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let mut cases = Vec::new();
        for record in &records {
            serde_json::to_writer(&mut cases, record).unwrap();
            cases.push(b'\n');
        }
        std::fs::write(run_dir.join("cases.jsonl"), cases).unwrap();

        let started = std::time::Instant::now();
        let generated = generate(run_dir).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
        let index_html = std::fs::read_to_string(&generated.index_path).unwrap();
        assert!(!index_html.contains("invoice-0999"));
        assert!(index_html.contains("Search case ID or metadata"));
        assert!(std::fs::metadata(&generated.index_path).unwrap().len() < 512 * 1024);
        let index: Vec<CaseIndexEntry> =
            serde_json::from_slice(&std::fs::read(run_dir.join("report/case-index.json")).unwrap())
                .unwrap();
        assert_eq!(index.len(), 1_000);
        assert_eq!(
            std::fs::read_dir(run_dir.join("report/cases"))
                .unwrap()
                .count(),
            20
        );
        assert!(!run_dir.join("report/single.html").exists());
        assert!(directory_size(&run_dir.join("report")).unwrap() < 16 * 1024 * 1024);
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
            "version": 2,
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
    fn model_visible_metadata_secret_echo_is_redacted() {
        let mut config = report_config("metadata privacy");
        config.storage.redaction.json_pointers =
            vec!["/model_visible_metadata/account_name".to_owned()];
        let secret = "PRIVATE_ACCOUNT_7d21";
        let mut record = passing_record("metadata-case".to_owned(), json!({"text": "invoice"}));
        record.case.model_visible_metadata = Some(json!({"account_name": secret}));
        record.baseline_output.raw_output = Some(json!({"account": secret}).to_string());
        record.candidate_output.raw_output = record.baseline_output.raw_output.clone();
        record.baseline_output.metadata = json!({"echo": secret});
        record.candidate_output.metadata = record.baseline_output.metadata.clone();
        let view = case_view(&record, &config).unwrap();
        let serialized = serde_json::to_string(&view).unwrap();
        assert!(!serialized.contains(secret));
        assert!(serialized.contains(REDACTION_MARKER));
        assert!(view.model_visible_metadata.contains(REDACTION_MARKER));
    }

    #[test]
    fn redaction_never_falls_back_for_typed_or_reserved_values() {
        let config: Config = serde_json::from_value(json!({
            "version": 2,
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

    fn server_state() -> Arc<ReportServerState> {
        let path = std::env::temp_dir().join(format!(
            "structtrace-report-server-test-{}.html",
            std::process::id()
        ));
        std::fs::write(&path, b"safe").unwrap();
        Arc::new(ReportServerState {
            token: "secret-token".to_owned(),
            expected_host: "127.0.0.1:43210".to_owned(),
            assets: BTreeMap::from([(
                "index.html".to_owned(),
                VerifiedReportAsset {
                    content_type: "text/html; charset=utf-8".to_owned(),
                    blake3: hash_file(&path).unwrap(),
                    path,
                },
            )]),
        })
    }

    fn request_headers(host: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(axum::http::header::HOST, host.parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn report_requires_capability_token_and_rejects_foreign_host() {
        let missing = serve_verified_asset(
            axum::extract::State(server_state()),
            request_headers("127.0.0.1:43210"),
            "/index.html".parse().unwrap(),
        )
        .await;
        assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);

        let foreign = serve_verified_asset(
            axum::extract::State(server_state()),
            request_headers("attacker.invalid"),
            "/secret-token/".parse().unwrap(),
        )
        .await;
        assert_eq!(
            foreign.status(),
            axum::http::StatusCode::MISDIRECTED_REQUEST
        );
    }

    #[tokio::test]
    async fn report_rejects_foreign_origin_and_sets_security_headers() {
        let mut headers = request_headers("127.0.0.1:43210");
        headers.insert(
            axum::http::header::ORIGIN,
            "https://attacker.invalid".parse().unwrap(),
        );
        let foreign = serve_verified_asset(
            axum::extract::State(server_state()),
            headers,
            "/secret-token/".parse().unwrap(),
        )
        .await;
        assert_eq!(foreign.status(), axum::http::StatusCode::FORBIDDEN);

        let response = serve_verified_asset(
            axum::extract::State(server_state()),
            request_headers("127.0.0.1:43210"),
            "/secret-token/".parse().unwrap(),
        )
        .await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response.headers()[axum::http::header::CACHE_CONTROL],
            "no-store"
        );
        assert_eq!(
            response.headers()[axum::http::header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert!(response.headers().contains_key("content-security-policy"));
        assert!(
            response
                .headers()
                .contains_key("cross-origin-resource-policy")
        );
    }
}
