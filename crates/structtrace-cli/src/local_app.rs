//! Capability-protected local browser product backed by the StructTrace engine.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path as AxumPath, Query, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use structtrace_core::{
    artifact::{PairedCaseRecord, RunSummary},
    config::{Config, DatasetFields, GateMode},
};
use ulid::Ulid;

use crate::initialize::{FromOutputsOptions, SimpleOutputFields, initialize_from_outputs};

const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const INDEX_HTML: &[u8] = include_bytes!("../../../ui/dist/index.html");
const APP_JS: &[u8] = include_bytes!("../../../ui/dist/assets/app.js");
const APP_CSS: &[u8] = include_bytes!("../../../ui/dist/assets/app.css");
const LOGO_MARK: &[u8] = include_bytes!("../../../ui/dist/assets/structtrace-logo-mark.svg");
const APP_ICON: &[u8] = include_bytes!("../../../ui/dist/assets/structtrace-app-icon.svg");
const WORDMARK: &[u8] = include_bytes!("../../../ui/dist/assets/structtrace-wordmark.svg");
const DESIGN_TOKENS: &[u8] =
    include_bytes!("../../../ui/dist/assets/structtrace-design-tokens.json");

#[derive(Debug)]
struct AppState {
    token: String,
    expected_host: String,
    project_root: PathBuf,
    runs: Mutex<HashMap<String, PathBuf>>,
    pin_lock: Mutex<()>,
    last_activity: AtomicU64,
    active_runs: AtomicUsize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemResponse {
    product: &'static str,
    version: &'static str,
    local_only: bool,
    telemetry: bool,
    max_upload_bytes: usize,
    api_version: &'static str,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserFile {
    name: String,
    format: InputFormat,
    content: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum InputFormat {
    Json,
    Jsonl,
    Csv,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonFiles {
    dataset: BrowserFile,
    baseline: BrowserFile,
    candidate: BrowserFile,
    schema: Option<BrowserFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MappingRequest {
    dataset_id: String,
    dataset_input: String,
    dataset_expected: String,
    baseline_id: String,
    baseline_output: String,
    candidate_id: String,
    candidate_output: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RuleKind {
    Exact,
    NormalizedString,
    CanonicalDate,
    ExactInteger,
    DecimalExact,
    DecimalTolerance,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuleRequest {
    pointer: String,
    kind: RuleKind,
    tolerance: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonRequest {
    name: String,
    baseline_name: String,
    candidate_name: String,
    files: ComparisonFiles,
    mapping: MappingRequest,
    rules: Vec<RuleRequest>,
    gate_mode: GateMode,
    min_cases: usize,
    financial_invariants: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    run_id: String,
    project_name: String,
    created_at: u64,
    summary: RunSummary,
    schema_provenance: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CasePageQuery {
    offset: usize,
    limit: usize,
    filter: String,
    search: String,
}

impl Default for CasePageQuery {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 200,
            filter: "all".to_owned(),
            search: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct CasePageResponse {
    items: Vec<PairedCaseRecord>,
    total: usize,
    offset: usize,
    limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CiRequest {
    mode: CiMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CiMode {
    Regression,
    Release,
}

#[derive(Debug, Serialize)]
struct CiResponse {
    config: String,
    workflow: String,
    command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PinnedCase {
    id: String,
    run_id: String,
    case_id: String,
    project_name: String,
    pinned_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PinRequest {
    run_id: String,
    case_id: String,
}

#[derive(Debug)]
struct AppError(anyhow::Error);

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"message": format!("{:#}", self.0)})),
        )
            .into_response()
    }
}

struct ActiveRunGuard(Arc<AppState>);

impl ActiveRunGuard {
    fn new(state: Arc<AppState>) -> Self {
        state.active_runs.fetch_add(1, Ordering::Relaxed);
        Self(state)
    }
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        self.0.active_runs.fetch_sub(1, Ordering::Relaxed);
        self.0.last_activity.store(now_seconds(), Ordering::Relaxed);
    }
}

/// Serve the local UI on a random loopback-only port until interrupted or inactive.
pub async fn serve(project_root: &Path, open_browser: bool) -> anyhow::Result<()> {
    let project_root = project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", project_root.display()))?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let token = capability_token();
    let expected_host = format!("127.0.0.1:{}", address.port());
    let url = format!("http://{expected_host}/{token}/");
    println!("StructTrace Local: {url}");
    println!("Local only · no account · no telemetry · Ctrl-C to stop");
    if open_browser {
        if let Err(error) = open::that(&url) {
            eprintln!(
                "warning: could not open the default browser ({error}); open this capability URL manually: {url}"
            );
        }
    }

    let known_runs = discover_ui_runs(&project_root)?;
    let state = Arc::new(AppState {
        token,
        expected_host,
        project_root,
        runs: Mutex::new(known_runs),
        pin_lock: Mutex::new(()),
        last_activity: AtomicU64::new(now_seconds()),
        active_runs: AtomicUsize::new(0),
    });
    let app = Router::new()
        .route("/{token}/api/v1/system", get(system))
        .route("/{token}/api/v1/demo", post(run_demo))
        .route("/{token}/api/v1/comparisons/run", post(run_comparison))
        .route(
            "/{token}/api/v1/comparisons/draft",
            get(get_draft).put(save_draft),
        )
        .route("/{token}/api/v1/runs/{run_id}", get(get_run))
        .route("/{token}/api/v1/runs/{run_id}/accept", post(accept_run))
        .route("/{token}/api/v1/runs", get(list_runs))
        .route("/{token}/api/v1/runs/{run_id}/cases", get(get_run_cases))
        .route("/{token}/api/v1/ci/generate", post(generate_ci))
        .route("/{token}/api/v1/regressions", get(list_regressions))
        .route("/{token}/api/v1/regressions/pin", post(pin_regression))
        .route(
            "/{token}/api/v1/regressions/{pin_id}",
            delete(delete_regression),
        )
        .route("/{token}/", get(index))
        .route("/{token}/{*path}", get(static_or_spa))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            validate_request,
        ))
        .with_state(Arc::clone(&state));

    let shutdown_state = Arc::clone(&state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => break,
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {
                        let idle = now_seconds().saturating_sub(shutdown_state.last_activity.load(Ordering::Relaxed));
                        if shutdown_state.active_runs.load(Ordering::Relaxed) == 0
                            && idle >= INACTIVITY_TIMEOUT.as_secs()
                        {
                            eprintln!("StructTrace Local stopped after 30 minutes of inactivity.");
                            break;
                        }
                    }
                }
            }
        })
        .await?;
    Ok(())
}

async fn validate_request(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if host != Some(state.expected_host.as_str()) {
        return StatusCode::MISDIRECTED_REQUEST.into_response();
    }
    let expected_origin = format!("http://{}", state.expected_host);
    for name in [header::ORIGIN, header::REFERER] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            let accepted = value == expected_origin
                || value
                    .strip_prefix(&expected_origin)
                    .is_some_and(|suffix| suffix.starts_with('/'));
            if !accepted {
                return StatusCode::FORBIDDEN.into_response();
            }
        }
    }
    let expected_prefix = format!("/{}/", state.token);
    if !request.uri().path().starts_with(&expected_prefix) {
        return StatusCode::NOT_FOUND.into_response();
    }
    state.last_activity.store(now_seconds(), Ordering::Relaxed);
    secure(next.run(request).await)
}

fn secure(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'none'; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; img-src 'self' data:; object-src 'none'; script-src 'self'; style-src 'self'",
        ),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    response
}

async fn system() -> Json<SystemResponse> {
    Json(SystemResponse {
        product: "StructTrace",
        version: env!("CARGO_PKG_VERSION"),
        local_only: true,
        telemetry: false,
        max_upload_bytes: MAX_REQUEST_BYTES,
        api_version: "v1",
    })
}

async fn index() -> Response {
    asset(INDEX_HTML, "text/html; charset=utf-8")
}

async fn static_or_spa(AxumPath(params): AxumPath<HashMap<String, String>>) -> Response {
    match params.get("path").map(String::as_str).unwrap_or_default() {
        "assets/app.js" => asset(APP_JS, "text/javascript; charset=utf-8"),
        "assets/app.css" => asset(APP_CSS, "text/css; charset=utf-8"),
        "assets/structtrace-logo-mark.svg" => asset(LOGO_MARK, "image/svg+xml"),
        "assets/structtrace-app-icon.svg" => asset(APP_ICON, "image/svg+xml"),
        "assets/structtrace-wordmark.svg" => asset(WORDMARK, "image/svg+xml"),
        "assets/structtrace-design-tokens.json" => {
            asset(DESIGN_TOKENS, "application/json; charset=utf-8")
        }
        path if path.starts_with("api/") => StatusCode::NOT_FOUND.into_response(),
        _ => asset(INDEX_HTML, "text/html; charset=utf-8"),
    }
}

fn asset(bytes: &'static [u8], content_type: &'static str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

async fn run_demo(State(state): State<Arc<AppState>>) -> Result<Json<RunResponse>, AppError> {
    let guard = ActiveRunGuard::new(Arc::clone(&state));
    let root = state.project_root.clone();
    let run =
        tokio::task::spawn_blocking(move || crate::bundled_demo::run_invoice(&root)).await??;
    state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .insert(run.run_id.clone(), run.run_dir.clone());
    let response = response_from_run(&run.run_dir)?;
    drop(guard);
    Ok(Json(response))
}

async fn run_comparison(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ComparisonRequest>,
) -> Result<Json<RunResponse>, AppError> {
    validate_comparison_request(&request)?;
    let guard = ActiveRunGuard::new(Arc::clone(&state));
    let root = state.project_root.clone();
    let (run, _schema_provenance) =
        tokio::task::spawn_blocking(move || materialize_and_run_comparison(&root, request))
            .await??;
    state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .insert(run.run_id.clone(), run.run_dir.clone());
    let response = response_from_run(&run.run_dir)?;
    drop(guard);
    Ok(Json(response))
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<RunResponse>, AppError> {
    let run_id = params.get("run_id").context("run ID is missing")?;
    let run_dir = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .get(run_id)
        .cloned()
        .context("this local UI session does not know that run")?;
    Ok(Json(response_from_run(&run_dir)?))
}

async fn accept_run(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let run_id = params.get("run_id").context("run ID is missing")?;
    let run_dir = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .get(run_id)
        .cloned()
        .context("run is not part of this local StructTrace workspace")?;
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    if summary.gate.status != structtrace_core::gate::GateStatus::Passed {
        return Err(AppError(anyhow::anyhow!(
            "only a comparison with a passed configured gate can become an accepted baseline"
        )));
    }
    let manifest: structtrace_core::artifact::RunManifest =
        read_json(&run_dir.join("manifest.json"))?;
    let accepted = serde_json::json!({
        "runId": run_id,
        "projectName": manifest.project_name,
        "acceptedAt": now_seconds(),
        "candidateArtifactHash": manifest.input_artifacts.get("candidate")
    });
    let directory = state.project_root.join(".structtrace/ui");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join("accepted-baseline.json"),
        serde_json::to_vec_pretty(&accepted)?,
    )?;
    Ok(Json(accepted))
}

async fn list_runs(State(state): State<Arc<AppState>>) -> Result<Json<Vec<RunResponse>>, AppError> {
    let ui_projects = state.project_root.join(".structtrace/ui/projects");
    let directories = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .values()
        .filter(|directory| directory.starts_with(&ui_projects))
        .cloned()
        .collect::<Vec<_>>();
    let mut runs = directories
        .iter()
        .map(|directory| response_from_run(directory))
        .collect::<anyhow::Result<Vec<_>>>()?;
    runs.sort_by_key(|run| std::cmp::Reverse(run.created_at));
    Ok(Json(runs))
}

async fn get_run_cases(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
    Query(query): Query<CasePageQuery>,
) -> Result<Json<CasePageResponse>, AppError> {
    if query.limit == 0 || query.limit > 500 {
        return Err(AppError(anyhow::anyhow!("case page limit must be 1..=500")));
    }
    if query.offset > 100_000 {
        return Err(AppError(anyhow::anyhow!(
            "case page offset exceeds the hard case limit"
        )));
    }
    if query.search.len() > 256 {
        return Err(AppError(anyhow::anyhow!(
            "case search exceeds 256 characters"
        )));
    }
    let run_id = params.get("run_id").context("run ID is missing")?;
    let run_dir = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .get(run_id)
        .cloned()
        .context("this local UI session does not know that run")?;
    let bytes = structtrace_core::hashing::read_bounded(
        &run_dir.join("cases.jsonl"),
        64 * 1024 * 1024,
        "case evidence",
    )?;
    let text = std::str::from_utf8(&bytes)?;
    let search = query.search.to_lowercase();
    let pinned_case_ids = if query.filter == "pinned" {
        let _guard = state
            .pin_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("regression suite is unavailable"))?;
        read_pins(&state.project_root)?
            .into_iter()
            .filter(|pin| pin.run_id == *run_id)
            .map(|pin| pin.case_id)
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };
    let mut total = 0usize;
    let mut items = Vec::with_capacity(query.limit);
    for (index, line) in text.lines().enumerate() {
        let record: PairedCaseRecord = structtrace_core::strict_json::from_str(line)
            .with_context(|| format!("invalid case artifact line {}", index + 1))?;
        if !case_matches_filter(&record, &query.filter, &pinned_case_ids)
            || (!search.is_empty() && !line.to_lowercase().contains(&search))
        {
            continue;
        }
        if total >= query.offset && items.len() < query.limit {
            items.push(record);
        }
        total += 1;
    }
    Ok(Json(CasePageResponse {
        items,
        total,
        offset: query.offset,
        limit: query.limit,
    }))
}

fn case_matches_filter(
    record: &PairedCaseRecord,
    filter: &str,
    pinned_case_ids: &HashSet<String>,
) -> bool {
    match filter {
        "all" => true,
        "regressions" => record.transition == "baseline_only_pass",
        "improvements" => record.transition == "candidate_only_pass",
        "both_wrong" => record.transition == "both_fail",
        "valid_but_wrong" => {
            record.baseline_evaluation.valid_but_wrong
                || record.candidate_evaluation.valid_but_wrong
        }
        "parse_failures" => {
            !record.baseline_evaluation.parse_valid || !record.candidate_evaluation.parse_valid
        }
        "schema_failures" => {
            !record.baseline_evaluation.schema_valid || !record.candidate_evaluation.schema_valid
        }
        "evaluator_errors" => record
            .baseline_evaluation
            .evaluators
            .values()
            .chain(record.candidate_evaluation.evaluators.values())
            .any(|result| result.status == structtrace_core::evaluation::EvaluationStatus::Error),
        "pinned" => pinned_case_ids.contains(&record.case.id),
        _ => false,
    }
}

async fn save_draft(
    State(state): State<Arc<AppState>>,
    Json(value): Json<Value>,
) -> Result<StatusCode, AppError> {
    let bytes = serde_json::to_vec_pretty(&value)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(AppError(anyhow::anyhow!("draft exceeds the 64 MiB limit")));
    }
    let directory = state.project_root.join(".structtrace/ui");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("draft.json");
    let temporary = directory.join(format!("draft.{}.tmp", Ulid::new()));
    std::fs::write(&temporary, bytes)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_draft(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let path = state.project_root.join(".structtrace/ui/draft.json");
    if !path.exists() {
        return Ok(Json(serde_json::json!({"draft": null})));
    }
    let bytes = structtrace_core::hashing::read_bounded(
        &path,
        MAX_REQUEST_BYTES,
        "local comparison draft",
    )?;
    let draft = structtrace_core::strict_json::value_from_slice(&bytes)?;
    Ok(Json(serde_json::json!({"draft": draft})))
}

async fn list_regressions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PinnedCase>>, AppError> {
    let _guard = state
        .pin_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("regression suite is unavailable"))?;
    Ok(Json(read_pins(&state.project_root)?))
}

async fn pin_regression(
    State(state): State<Arc<AppState>>,
    Json(request): Json<PinRequest>,
) -> Result<Json<PinnedCase>, AppError> {
    if request.case_id.trim().is_empty() {
        return Err(AppError(anyhow::anyhow!("case ID is required")));
    }
    let run_dir = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .get(&request.run_id)
        .cloned()
        .context("run is not part of this local StructTrace workspace")?;
    let manifest: structtrace_core::artifact::RunManifest =
        read_json(&run_dir.join("manifest.json"))?;
    let bytes = structtrace_core::hashing::read_bounded(
        &run_dir.join("cases.jsonl"),
        64 * 1024 * 1024,
        "case evidence",
    )?;
    let mut found = false;
    for line in std::str::from_utf8(&bytes)?.lines() {
        let record = structtrace_core::strict_json::from_str::<PairedCaseRecord>(line)?;
        if record.case.id == request.case_id {
            found = true;
            break;
        }
    }
    if !found {
        return Err(AppError(anyhow::anyhow!(
            "case is not present in the immutable run evidence"
        )));
    }
    let _guard = state
        .pin_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("regression suite is unavailable"))?;
    let mut pins = read_pins(&state.project_root)?;
    if let Some(existing) = pins
        .iter()
        .find(|pin| pin.run_id == request.run_id && pin.case_id == request.case_id)
    {
        return Ok(Json(existing.clone()));
    }
    let pin = PinnedCase {
        id: Ulid::new().to_string(),
        run_id: request.run_id,
        case_id: request.case_id,
        project_name: manifest.project_name,
        pinned_at: now_seconds(),
    };
    pins.push(pin.clone());
    write_pins(&state.project_root, &pins)?;
    Ok(Json(pin))
}

async fn delete_regression(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<StatusCode, AppError> {
    let pin_id = params.get("pin_id").context("pin ID is missing")?;
    let _guard = state
        .pin_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("regression suite is unavailable"))?;
    let mut pins = read_pins(&state.project_root)?;
    let before = pins.len();
    pins.retain(|pin| pin.id != *pin_id);
    if pins.len() == before {
        return Err(AppError(anyhow::anyhow!("pinned case was not found")));
    }
    write_pins(&state.project_root, &pins)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn generate_ci(Json(request): Json<CiRequest>) -> Json<CiResponse> {
    let (mode, command) = match request.mode {
        CiMode::Regression => ("regression", "structtrace gate latest"),
        CiMode::Release => ("release", "structtrace release-check latest"),
    };
    let config = format!(
        "# Generated by StructTrace Local. Review before committing.\nversion: 3\nproject:\n  name: structured-output-comparison\ngate:\n  mode: {mode}\n  min_cases: 100\n  min_unique_cases: 100\n  min_primary_fully_evaluated_rate: 0.99\n  max_deployment_regression_pp: 0.0\n"
    );
    let workflow = format!(
        "# Starter template: pin the StructTrace installation before use.\nname: StructTrace\n\non:\n  pull_request:\n\npermissions:\n  contents: read\n\njobs:\n  structured-output-regression:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0\n      - name: Run comparison\n        run: structtrace run\n      - name: Verify decision\n        run: {command}\n"
    );
    Json(CiResponse {
        config,
        workflow,
        command: command.to_owned(),
    })
}

fn materialize_and_run_comparison(
    root: &Path,
    request: ComparisonRequest,
) -> anyhow::Result<(structtrace_engine::CompletedRun, &'static str)> {
    let project_id = Ulid::new().to_string();
    let projects_root = root.join(".structtrace/ui/projects");
    std::fs::create_dir_all(&projects_root)?;
    let destination = projects_root.join(&project_id);
    let staging = projects_root.join(format!("{project_id}.inputs"));
    std::fs::create_dir(&staging)?;

    let dataset_bytes = normalized_jsonl(&request.files.dataset)?;
    let baseline_bytes = normalize_simple_outputs(
        &normalized_jsonl(&request.files.baseline)?,
        &request.mapping.baseline_id,
        &request.mapping.baseline_output,
    )?;
    let candidate_bytes = normalize_simple_outputs(
        &normalized_jsonl(&request.files.candidate)?,
        &request.mapping.candidate_id,
        &request.mapping.candidate_output,
    )?;
    let dataset_path = staging.join("dataset.jsonl");
    let baseline_path = staging.join("baseline.jsonl");
    let candidate_path = staging.join("candidate.jsonl");
    std::fs::write(&dataset_path, &dataset_bytes)?;
    std::fs::write(&baseline_path, &baseline_bytes)?;
    std::fs::write(&candidate_path, &candidate_bytes)?;
    let (schema_bytes, schema_provenance) = match request.files.schema.as_ref() {
        Some(schema) => (normalized_schema(schema)?, "caller_supplied"),
        None => (
            inferred_schema(&dataset_bytes, &request.mapping.dataset_expected)?,
            "inferred_from_expected_values",
        ),
    };
    let schema_path = staging.join("schema.json");
    std::fs::write(&schema_path, schema_bytes)?;

    let field_evaluators = request
        .rules
        .iter()
        .map(|rule| {
            let kind = match rule.kind {
                RuleKind::Exact => "exact".to_owned(),
                RuleKind::NormalizedString => "normalized_string".to_owned(),
                RuleKind::CanonicalDate => "canonical_date".to_owned(),
                RuleKind::ExactInteger => "exact_integer".to_owned(),
                RuleKind::DecimalExact => "decimal_exact".to_owned(),
                RuleKind::DecimalTolerance => format!(
                    "decimal_tolerance:{}",
                    rule.tolerance.as_deref().unwrap_or("0.01")
                ),
            };
            format!("{}={kind}", rule.pointer)
        })
        .collect::<Vec<_>>();
    initialize_from_outputs(FromOutputsOptions {
        destination: &destination,
        dataset: &dataset_path,
        baseline: &baseline_path,
        candidate: &candidate_path,
        schema: &schema_path,
        dataset_fields: DatasetFields {
            id: request.mapping.dataset_id,
            input: request.mapping.dataset_input,
            expected: request.mapping.dataset_expected,
            model_visible_metadata: "/model_visible_metadata".to_owned(),
            metadata: "/metadata".to_owned(),
        },
        output_fields: SimpleOutputFields {
            id: "/id".to_owned(),
            output: "/output".to_owned(),
        },
        correctness_pointers: &[],
        field_evaluators: &field_evaluators,
        keyed_arrays: &[],
        financial_invariants: request.financial_invariants,
        exact_json: false,
        gate_mode: request.gate_mode,
        min_cases: request.min_cases,
    })?;
    let config_path = destination.join("structtrace.yaml");
    let mut config = Config::load(&config_path)?;
    config.project.name = request.name;
    config.project.description = Some(format!(
        "{} compared with {} through StructTrace Local",
        request.baseline_name, request.candidate_name
    ));
    std::fs::write(&config_path, serde_yaml_ng::to_string(&config)?)?;
    let run = structtrace_engine::run_recorded(&destination, &config_path)?;
    std::fs::remove_dir_all(&staging)?;
    Ok((run, schema_provenance))
}

fn validate_comparison_request(request: &ComparisonRequest) -> anyhow::Result<()> {
    anyhow::ensure!(
        !request.name.trim().is_empty(),
        "comparison name is required"
    );
    anyhow::ensure!(
        !request.rules.is_empty(),
        "select at least one correctness rule"
    );
    anyhow::ensure!(request.min_cases > 0, "minimum cases must be at least one");
    for file in [
        &request.files.dataset,
        &request.files.baseline,
        &request.files.candidate,
    ] {
        anyhow::ensure!(!file.name.trim().is_empty(), "source file name is missing");
        anyhow::ensure!(
            file.content.len() <= MAX_FILE_BYTES,
            "{} exceeds 32 MiB",
            file.name
        );
        anyhow::ensure!(!file.content.trim().is_empty(), "{} is empty", file.name);
    }
    if let Some(schema) = &request.files.schema {
        anyhow::ensure!(
            schema.content.len() <= MAX_FILE_BYTES,
            "schema exceeds 32 MiB"
        );
    }
    Ok(())
}

fn normalized_jsonl(file: &BrowserFile) -> anyhow::Result<Vec<u8>> {
    let rows = match file.format {
        InputFormat::Json => {
            let value = structtrace_core::strict_json::value_from_str(&file.content)
                .with_context(|| format!("{} is not strict JSON", file.name))?;
            match value {
                Value::Array(rows) => rows,
                row => vec![row],
            }
        }
        InputFormat::Jsonl => file
            .content
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| {
                structtrace_core::strict_json::value_from_str(line)
                    .with_context(|| format!("{} line {} is not strict JSON", file.name, index + 1))
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
        InputFormat::Csv => csv_rows(file)?,
    };
    anyhow::ensure!(!rows.is_empty(), "{} contains no records", file.name);
    let mut output = Vec::new();
    for row in rows {
        serde_json::to_writer(&mut output, &row)?;
        output.push(b'\n');
    }
    Ok(output)
}

fn csv_rows(file: &BrowserFile) -> anyhow::Result<Vec<Value>> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(file.content.as_bytes());
    let headers = reader.headers()?.clone();
    let mut rows = Vec::new();
    for (index, row) in reader.records().enumerate() {
        let row = row.with_context(|| format!("invalid CSV row {} in {}", index + 2, file.name))?;
        let values = headers
            .iter()
            .zip(row.iter())
            .map(|(key, value)| {
                let parsed = structtrace_core::strict_json::value_from_str(value)
                    .unwrap_or_else(|_| Value::String(value.to_owned()));
                (key.to_owned(), parsed)
            })
            .collect::<serde_json::Map<_, _>>();
        rows.push(Value::Object(values));
    }
    Ok(rows)
}

fn normalized_schema(file: &BrowserFile) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        matches!(file.format, InputFormat::Json),
        "schema must be a JSON document"
    );
    let value = structtrace_core::strict_json::value_from_str(&file.content)
        .with_context(|| format!("{} is not a strict JSON Schema document", file.name))?;
    Ok(serde_json::to_vec_pretty(&value)?)
}

fn normalize_simple_outputs(
    bytes: &[u8],
    id_pointer: &str,
    output_pointer: &str,
) -> anyhow::Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes)?;
    let mut output = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let value = structtrace_core::strict_json::value_from_str(line)?;
        let id = value
            .pointer(id_pointer)
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .with_context(|| {
                format!(
                    "output row {}: {id_pointer} must resolve to a non-empty string",
                    index + 1
                )
            })?;
        let selected = value.pointer(output_pointer).cloned();
        let status = value
            .pointer("/status")
            .and_then(Value::as_str)
            .unwrap_or("ok");
        anyhow::ensure!(
            status == "error" || selected.is_some(),
            "output row {}: {output_pointer} did not resolve",
            index + 1
        );
        let mut normalized = serde_json::Map::new();
        normalized.insert("id".to_owned(), Value::String(id.to_owned()));
        normalized.insert("status".to_owned(), Value::String(status.to_owned()));
        if let Some(selected) = selected {
            normalized.insert("output".to_owned(), selected);
        }
        for key in ["error", "latency_ms", "usage", "cost", "metadata"] {
            if let Some(item) = value.get(key) {
                normalized.insert(key.to_owned(), item.clone());
            }
        }
        serde_json::to_writer(&mut output, &Value::Object(normalized))?;
        output.push(b'\n');
    }
    Ok(output)
}

fn inferred_schema(dataset: &[u8], expected_pointer: &str) -> anyhow::Result<Vec<u8>> {
    let first = std::str::from_utf8(dataset)?
        .lines()
        .next()
        .context("dataset contains no rows")?;
    let row = structtrace_core::strict_json::value_from_str(first)?;
    let expected = row
        .pointer(expected_pointer)
        .context("expected output mapping did not resolve in the first row")?;
    let mut schema = schema_for_value(expected);
    if let Value::Object(object) = &mut schema {
        object.insert(
            "$schema".to_owned(),
            Value::String("https://json-schema.org/draft/2020-12/schema".to_owned()),
        );
        object.insert(
            "title".to_owned(),
            Value::String("StructTrace inferred expected-output shape".to_owned()),
        );
    }
    Ok(serde_json::to_vec_pretty(&schema)?)
}

fn schema_for_value(value: &Value) -> Value {
    match value {
        Value::Null => serde_json::json!({"type": "null"}),
        Value::Bool(_) => serde_json::json!({"type": "boolean"}),
        Value::Number(number) if number.is_i64() || number.is_u64() => {
            serde_json::json!({"type": "integer"})
        }
        Value::Number(_) => serde_json::json!({"type": "number"}),
        Value::String(_) => serde_json::json!({"type": "string"}),
        Value::Array(values) => serde_json::json!({
            "type": "array",
            "items": values.first().map(schema_for_value).unwrap_or_else(|| serde_json::json!({}))
        }),
        Value::Object(values) => {
            let properties = values
                .iter()
                .map(|(key, value)| (key.clone(), schema_for_value(value)))
                .collect::<serde_json::Map<_, _>>();
            serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": values.keys().collect::<Vec<_>>(),
                "additionalProperties": false
            })
        }
    }
}

fn response_from_run(run_dir: &Path) -> anyhow::Result<RunResponse> {
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    let manifest: structtrace_core::artifact::RunManifest =
        read_json(&run_dir.join("manifest.json"))?;
    Ok(RunResponse {
        run_id: manifest.run_id,
        project_name: manifest.project_name,
        created_at: u64::try_from(manifest.started_at_unix_ms).unwrap_or(u64::MAX),
        summary,
        schema_provenance: schema_provenance(run_dir),
    })
}

fn schema_provenance(run_dir: &Path) -> &'static str {
    let inferred = read_json::<Value>(&run_dir.join("inputs/schema.json"))
        .ok()
        .and_then(|schema| {
            schema
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|title| title == "StructTrace inferred expected-output shape");
    if inferred {
        "inferred_from_expected_values"
    } else {
        "caller_supplied"
    }
}

fn discover_ui_runs(project_root: &Path) -> anyhow::Result<HashMap<String, PathBuf>> {
    let projects = project_root.join(".structtrace/ui/projects");
    let mut runs = HashMap::new();
    let mut run_roots = vec![project_root.join(".structtrace/runs")];
    if projects.is_dir() {
        for project in std::fs::read_dir(projects)? {
            let project = project?;
            if project.file_type()?.is_dir() && !project.file_type()?.is_symlink() {
                run_roots.push(project.path().join(".structtrace/runs"));
            }
        }
    }
    for run_root in run_roots {
        if run_root.is_dir() {
            for run in std::fs::read_dir(run_root)? {
                let run = run?;
                if !run.file_type()?.is_dir() || run.file_type()?.is_symlink() {
                    continue;
                }
                let manifest_path = run.path().join("manifest.json");
                let Ok(manifest) =
                    read_json::<structtrace_core::artifact::RunManifest>(&manifest_path)
                else {
                    continue;
                };
                if manifest.status == structtrace_core::artifact::RunStatus::Complete {
                    runs.insert(manifest.run_id, run.path());
                }
            }
        }
    }
    Ok(runs)
}

fn read_pins(project_root: &Path) -> anyhow::Result<Vec<PinnedCase>> {
    let path = project_root.join(".structtrace/ui/regressions.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes =
        structtrace_core::hashing::read_bounded(&path, 4 * 1024 * 1024, "pinned regression cases")?;
    structtrace_core::strict_json::from_slice(&bytes).map_err(Into::into)
}

fn write_pins(project_root: &Path, pins: &[PinnedCase]) -> anyhow::Result<()> {
    let directory = project_root.join(".structtrace/ui");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("regressions.json");
    let temporary = directory.join(format!("regressions.{}.tmp", Ulid::new()));
    std::fs::write(&temporary, serde_json::to_vec_pretty(pins)?)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = structtrace_core::hashing::read_bounded(path, 64 * 1024 * 1024, "run artifact")?;
    structtrace_core::strict_json::from_slice(&bytes).map_err(Into::into)
}

fn capability_token() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_assets_have_no_runtime_network_dependency() {
        for asset in [INDEX_HTML, APP_JS, APP_CSS] {
            let text = std::str::from_utf8(asset).unwrap();
            assert!(!text.contains("https://fonts."));
            assert!(!text.contains("cdn.jsdelivr"));
            assert!(!text.contains("unpkg.com"));
        }
    }

    #[test]
    fn csv_normalization_preserves_nested_json_cells() {
        let file = BrowserFile {
            name: "cases.csv".to_owned(),
            format: InputFormat::Csv,
            content: "id,output\na,\"{\"\"answer\"\":4}\"\n".to_owned(),
        };
        let normalized = normalized_jsonl(&file).unwrap();
        let row: Value =
            structtrace_core::strict_json::from_slice(normalized.trim_ascii()).unwrap();
        assert_eq!(row.pointer("/output/answer"), Some(&Value::from(4)));
    }

    #[test]
    fn inferred_schema_is_closed_and_required() {
        let source = b"{\"id\":\"a\",\"expected\":{\"answer\":4}}\n";
        let schema: Value =
            serde_json::from_slice(&inferred_schema(source, "/expected").unwrap()).unwrap();
        assert_eq!(
            schema.pointer("/additionalProperties"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            schema.pointer("/required/0"),
            Some(&Value::String("answer".to_owned()))
        );
    }
}
