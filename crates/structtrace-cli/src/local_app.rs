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
    config::{Config, DatasetFields, EvaluatorKind, GateMode},
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
    comparison_lock: Mutex<()>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserFile {
    name: String,
    format: InputFormat,
    content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum InputFormat {
    Json,
    Jsonl,
    Csv,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonFiles {
    dataset: SourceReference,
    baseline: SourceReference,
    candidate: SourceReference,
    schema: Option<SourceReference>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceReference {
    source_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StageSourceRequest {
    kind: String,
    file: BrowserFile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedSource {
    source_id: String,
    kind: String,
    name: String,
    format: InputFormat,
    hash: String,
    bytes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptedBaseline {
    run_id: String,
    project_id: String,
    accepted_at: u64,
    candidate_artifact_hash: String,
    source_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedBaselineResponse {
    accepted: AcceptedBaseline,
    source: AcceptedSourceResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedSourceResponse {
    source_id: String,
    hash: String,
    name: String,
    format: InputFormat,
    content: String,
    bytes: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    project_id: String,
    name: String,
    run_count: usize,
    updated_at: u64,
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
    baseline_status: Option<String>,
    baseline_error: Option<String>,
    baseline_latency: Option<String>,
    baseline_usage: Option<String>,
    baseline_cost: Option<String>,
    baseline_metadata: Option<String>,
    candidate_status: Option<String>,
    candidate_error: Option<String>,
    candidate_latency: Option<String>,
    candidate_usage: Option<String>,
    candidate_cost: Option<String>,
    candidate_metadata: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RuleKind {
    Exact,
    NormalizedString,
    CanonicalDate,
    ExactInteger,
    DecimalExact,
    DecimalTolerance,
    KeyedArray,
    RequiredFields,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuleRequest {
    pointer: String,
    kind: RuleKind,
    tolerance: Option<String>,
    keys: Option<String>,
    fields: Option<String>,
    formats: Option<String>,
    case_insensitive: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComparisonRequest {
    project_id: String,
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
    project_id: Option<String>,
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
    #[serde(default)]
    note: String,
    #[serde(default = "default_saved_case_status")]
    status: String,
}

fn default_saved_case_status() -> String {
    "open".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PinRequest {
    run_id: String,
    case_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdatePinnedCaseRequest {
    note: String,
    status: String,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AppError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            message: message.into(),
        }
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        let error = error.into();
        tracing::error!(error = ?error, "local API request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "StructTrace could not complete the local operation. Check the terminal for details.".to_owned(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({"code": self.code, "message": self.message})),
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
        comparison_lock: Mutex::new(()),
        last_activity: AtomicU64::new(now_seconds()),
        active_runs: AtomicUsize::new(0),
    });
    let app = Router::new()
        .route("/{token}/api/v1/system", get(system))
        .route("/{token}/api/v1/demo", post(run_demo))
        .route("/{token}/api/v1/comparisons/run", post(run_comparison))
        .route("/{token}/api/v1/sources", post(stage_source))
        .route(
            "/{token}/api/v1/comparisons/draft",
            get(get_draft).put(save_draft).delete(delete_draft),
        )
        .route("/{token}/api/v1/runs/{run_id}", get(get_run))
        .route("/{token}/api/v1/runs/{run_id}/accept", post(accept_run))
        .route(
            "/{token}/api/v1/projects/{project_id}/accepted-baseline",
            get(get_accepted_baseline),
        )
        .route("/{token}/api/v1/runs", get(list_runs))
        .route("/{token}/api/v1/projects", get(list_projects))
        .route(
            "/{token}/api/v1/projects/{project_id}",
            get(get_project).delete(archive_project),
        )
        .route(
            "/{token}/api/v1/projects/{project_id}/duplicate",
            post(duplicate_project),
        )
        .route("/{token}/api/v1/runs/{run_id}/cases", get(get_run_cases))
        .route("/{token}/api/v1/ci/generate", post(generate_ci))
        .route("/{token}/api/v1/regressions", get(list_regressions))
        .route("/{token}/api/v1/regressions/pin", post(pin_regression))
        .route(
            "/{token}/api/v1/regressions/{pin_id}",
            delete(delete_regression).put(update_regression),
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

async fn index(State(state): State<Arc<AppState>>) -> Response {
    index_response(&state.token)
}

async fn static_or_spa(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Response {
    match params.get("path").map(String::as_str).unwrap_or_default() {
        "assets/app.js" => asset(APP_JS, "text/javascript; charset=utf-8"),
        "assets/app.css" => asset(APP_CSS, "text/css; charset=utf-8"),
        "assets/structtrace-logo-mark.svg" => asset(LOGO_MARK, "image/svg+xml"),
        "assets/structtrace-app-icon.svg" => asset(APP_ICON, "image/svg+xml"),
        "assets/structtrace-wordmark.svg" => asset(WORDMARK, "image/svg+xml"),
        "assets/structtrace-design-tokens.json" => {
            asset(DESIGN_TOKENS, "application/json; charset=utf-8")
        }
        path if path.starts_with("assets/") => StatusCode::NOT_FOUND.into_response(),
        path if path.starts_with("api/") => StatusCode::NOT_FOUND.into_response(),
        _ => index_response(&state.token),
    }
}

fn index_response(token: &str) -> Response {
    // Vite emits relative asset paths so a standalone static build remains portable.
    // Capability URLs need absolute, token-scoped paths or a deep-link refresh would
    // request `/token/runs/assets/app.js` and receive HTML instead of JavaScript.
    let html = render_index(token);
    let mut response = Response::new(Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn render_index(token: &str) -> String {
    let template = std::str::from_utf8(INDEX_HTML).expect("embedded index is UTF-8");
    template.replace("./assets/", &format!("/{token}/assets/"))
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

async fn stage_source(
    State(state): State<Arc<AppState>>,
    Json(request): Json<StageSourceRequest>,
) -> Result<Json<StagedSource>, AppError> {
    if !matches!(
        request.kind.as_str(),
        "dataset" | "baseline" | "candidate" | "schema"
    ) {
        return Err(AppError::bad_request(
            "invalid_source_kind",
            "Source kind is not supported.",
        ));
    }
    if request.file.name.trim().is_empty() || request.file.name.len() > 255 {
        return Err(AppError::bad_request(
            "invalid_source_name",
            "Source name must contain 1 to 255 characters.",
        ));
    }
    let bytes = request.file.content.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(
            "invalid_source_size",
            "Source must contain 1 byte to 32 MiB.",
        ));
    }
    let staged = stage_browser_file(&state.project_root, request.kind, request.file)?;
    Ok(Json(staged))
}

async fn run_comparison(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ComparisonRequest>,
) -> Result<Json<RunResponse>, AppError> {
    validate_comparison_request(&request)
        .map_err(|error| AppError::bad_request("invalid_comparison", error.to_string()))?;
    let guard = ActiveRunGuard::new(Arc::clone(&state));
    let root = state.project_root.clone();
    let run_state = Arc::clone(&state);
    let (run, _schema_provenance) = tokio::task::spawn_blocking(move || {
        let _lock = run_state
            .comparison_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("comparison coordinator is unavailable"))?;
        materialize_and_run_comparison(&root, request)
    })
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
    let run_dir = run_dir_for(&state, run_id)?;
    Ok(Json(response_from_run(&run_dir)?))
}

async fn accept_run(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<AcceptedBaselineResponse>, AppError> {
    let run_id = params.get("run_id").context("run ID is missing")?;
    let run_dir = run_dir_for(&state, run_id)?;
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    if !summary.gate.deployment_authorized {
        return Err(AppError::bad_request(
            "acceptance_not_authorized",
            "Only a release comparison with deployment authorization can become the next baseline.",
        ));
    }
    let project_id =
        ui_project_id(&run_dir).context("only a persistent UI project can promote a baseline")?;
    let project_dir = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(&project_id);
    let canonical_candidate = structtrace_core::hashing::read_bounded(
        &run_dir.join("inputs/candidate.jsonl"),
        MAX_FILE_BYTES,
        "accepted candidate input",
    )?;
    let candidate = BrowserFile {
        name: format!("accepted-{run_id}.jsonl"),
        format: InputFormat::Jsonl,
        content: String::from_utf8(canonical_candidate)
            .context("accepted candidate is not UTF-8")?,
    };
    let staged = stage_browser_file(&state.project_root, "baseline".to_owned(), candidate)?;
    let manifest: structtrace_core::artifact::RunManifest =
        read_json(&run_dir.join("manifest.json"))?;
    let accepted = AcceptedBaseline {
        run_id: run_id.clone(),
        project_id,
        accepted_at: now_seconds(),
        candidate_artifact_hash: manifest
            .input_artifacts
            .get("inputs/candidate.jsonl")
            .cloned()
            .context("candidate hash is missing from the run manifest")?,
        source_id: staged.source_id.clone(),
    };
    atomic_write(
        &project_dir.join("accepted-baseline.json"),
        &serde_json::to_vec_pretty(&accepted)?,
    )?;
    Ok(Json(accepted_response(&state.project_root, accepted)?))
}

async fn get_accepted_baseline(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<AcceptedBaselineResponse>, AppError> {
    let project_id = params.get("project_id").context("project ID is missing")?;
    validate_project_id(project_id)
        .map_err(|error| AppError::bad_request("invalid_project_id", error.to_string()))?;
    let path = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(project_id)
        .join("accepted-baseline.json");
    if !path.exists() {
        return Err(AppError::not_found(
            "accepted_baseline_not_found",
            "This project has no accepted baseline.",
        ));
    }
    let accepted: AcceptedBaseline = read_json(&path)?;
    Ok(Json(accepted_response(&state.project_root, accepted)?))
}

async fn list_runs(State(state): State<Arc<AppState>>) -> Result<Json<Vec<RunResponse>>, AppError> {
    let directories = state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .values()
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
        return Err(AppError::bad_request(
            "invalid_page_limit",
            "Case page limit must be 1..=500.",
        ));
    }
    if query.offset > 100_000 {
        return Err(AppError::bad_request(
            "invalid_page_offset",
            "Case page offset exceeds the hard limit.",
        ));
    }
    if query.search.len() > 256 {
        return Err(AppError::bad_request(
            "invalid_case_search",
            "Case search must be at most 256 characters.",
        ));
    }
    let run_id = params.get("run_id").context("run ID is missing")?;
    let run_dir = run_dir_for(&state, run_id)?;
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

fn run_dir_for(state: &AppState, run_id: &str) -> Result<PathBuf, AppError> {
    state
        .runs
        .lock()
        .map_err(|error| {
            AppError::from(anyhow::anyhow!("local run index is unavailable: {error}"))
        })?
        .get(run_id)
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(
                "run_not_found",
                "Run is not part of this local StructTrace workspace.",
            )
        })
}

async fn save_draft(
    State(state): State<Arc<AppState>>,
    Json(value): Json<Value>,
) -> Result<StatusCode, AppError> {
    let mut value = value;
    strip_draft_source_contents(&mut value);
    let bytes = serde_json::to_vec_pretty(&value)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(AppError::bad_request(
            "draft_too_large",
            "Draft exceeds the 64 MiB request limit.",
        ));
    }
    let directory = state.project_root.join(".structtrace/ui");
    std::fs::create_dir_all(&directory)?;
    make_owner_only_directory(&directory)?;
    let path = directory.join("draft.json");
    atomic_write(&path, &bytes)?;
    if let Some(project_id) = value.get("projectId").and_then(Value::as_str) {
        validate_project_id(project_id)
            .map_err(|error| AppError::bad_request("invalid_project_id", error.to_string()))?;
        let project_dir = state
            .project_root
            .join(".structtrace/ui/projects")
            .join(project_id);
        if project_dir.exists()
            && std::fs::symlink_metadata(&project_dir)?
                .file_type()
                .is_symlink()
        {
            return Err(AppError::bad_request(
                "invalid_project",
                "Project directory must not be a symbolic link.",
            ));
        }
        std::fs::create_dir_all(&project_dir)?;
        make_owner_only_directory(&project_dir)?;
        atomic_write(&project_dir.join("ui-draft.json"), &bytes)?;
    }
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
    let mut draft = structtrace_core::strict_json::value_from_slice(&bytes)?;
    hydrate_draft_source_contents(&state.project_root, &mut draft)?;
    Ok(Json(serde_json::json!({"draft": draft})))
}

async fn delete_draft(State(state): State<Arc<AppState>>) -> Result<StatusCode, AppError> {
    let draft = state.project_root.join(".structtrace/ui/draft.json");
    let project_id = if draft.exists() {
        read_json::<Value>(&draft).ok().and_then(|value| {
            value
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    } else {
        None
    };
    if draft.exists() {
        std::fs::remove_file(draft)?;
    }
    if let Some(project_id) = project_id {
        let project = state
            .project_root
            .join(".structtrace/ui/projects")
            .join(project_id);
        let has_runs = project.join(".structtrace/runs").is_dir();
        let initialized = project.join("structtrace.yaml").exists();
        if project.is_dir() && !has_runs && !initialized {
            std::fs::remove_dir_all(project)?;
        }
    }
    garbage_collect_staged_sources(&state.project_root)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProjectSummary>>, AppError> {
    let root = state.project_root.join(".structtrace/ui/projects");
    if !root.exists() {
        return Ok(Json(Vec::new()));
    }
    let mut projects = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || entry.file_type()?.is_symlink() {
            continue;
        }
        let project_id = entry.file_name().to_string_lossy().to_string();
        if validate_project_id(&project_id).is_err() {
            continue;
        }
        let draft_path = entry.path().join("ui-draft.json");
        if !draft_path.exists() {
            continue;
        }
        let draft: Value = read_json(&draft_path)?;
        let name = draft
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unnamed comparison")
            .to_owned();
        let runs = entry.path().join(".structtrace/runs");
        let run_count = if runs.is_dir() {
            std::fs::read_dir(runs)?
                .filter_map(Result::ok)
                .filter(|item| item.file_type().is_ok_and(|kind| kind.is_dir()))
                .count()
        } else {
            0
        };
        let updated_at = std::fs::metadata(&draft_path)?
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        projects.push(ProjectSummary {
            project_id,
            name,
            run_count,
            updated_at,
        });
    }
    projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at));
    Ok(Json(projects))
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let project_id = params.get("project_id").context("project ID is missing")?;
    validate_project_id(project_id)
        .map_err(|error| AppError::bad_request("invalid_project_id", error.to_string()))?;
    let path = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(project_id)
        .join("ui-draft.json");
    if !path.exists() {
        return Err(AppError::not_found(
            "project_not_found",
            "Project was not found.",
        ));
    }
    let mut draft: Value = read_json(&path)?;
    hydrate_draft_source_contents(&state.project_root, &mut draft)?;
    Ok(Json(serde_json::json!({"draft": draft})))
}

async fn duplicate_project(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let project_id = params.get("project_id").context("project ID is missing")?;
    validate_project_id(project_id)
        .map_err(|error| AppError::bad_request("invalid_project_id", error.to_string()))?;
    let source = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(project_id)
        .join("ui-draft.json");
    if !source.exists() {
        return Err(AppError::not_found(
            "project_not_found",
            "Project was not found.",
        ));
    }
    let mut draft: Value = read_json(&source)?;
    let new_id = Ulid::new().to_string();
    draft["projectId"] = Value::String(new_id.clone());
    if let Some(name) = draft.get("name").and_then(Value::as_str).map(str::to_owned) {
        draft["name"] = Value::String(format!("{name} copy"));
    }
    let directory = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(&new_id);
    std::fs::create_dir_all(&directory)?;
    make_owner_only_directory(&directory)?;
    atomic_write(
        &directory.join("ui-draft.json"),
        &serde_json::to_vec_pretty(&draft)?,
    )?;
    atomic_write(
        &state.project_root.join(".structtrace/ui/draft.json"),
        &serde_json::to_vec_pretty(&draft)?,
    )?;
    hydrate_draft_source_contents(&state.project_root, &mut draft)?;
    Ok(Json(serde_json::json!({"draft": draft})))
}

async fn archive_project(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
) -> Result<StatusCode, AppError> {
    let project_id = params.get("project_id").context("project ID is missing")?;
    validate_project_id(project_id)
        .map_err(|error| AppError::bad_request("invalid_project_id", error.to_string()))?;
    let _lock = state
        .comparison_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("comparison coordinator is unavailable"))?;
    let source = state
        .project_root
        .join(".structtrace/ui/projects")
        .join(project_id);
    if !source.exists() {
        return Err(AppError::not_found(
            "project_not_found",
            "Project was not found.",
        ));
    }
    if std::fs::symlink_metadata(&source)?.file_type().is_symlink() {
        return Err(AppError::bad_request(
            "invalid_project",
            "Project directory must not be a symbolic link.",
        ));
    }
    let archive_root = state.project_root.join(".structtrace/ui/archived-projects");
    std::fs::create_dir_all(&archive_root)?;
    make_owner_only_directory(&archive_root)?;
    let destination = archive_root.join(format!("{project_id}-{}", now_seconds()));
    std::fs::rename(&source, &destination)?;
    state
        .runs
        .lock()
        .map_err(|_| anyhow::anyhow!("local run index is unavailable"))?
        .retain(|_, path| !path.starts_with(&source));
    let current = state.project_root.join(".structtrace/ui/draft.json");
    if current.exists() {
        let draft: Value = read_json(&current)?;
        if draft.get("projectId").and_then(Value::as_str) == Some(project_id) {
            std::fs::remove_file(current)?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
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
        return Err(AppError::bad_request(
            "missing_case_id",
            "Case ID is required.",
        ));
    }
    let run_dir = run_dir_for(&state, &request.run_id)?;
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
        return Err(AppError::not_found(
            "case_not_found",
            "Case is not present in the immutable run evidence.",
        ));
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
        note: String::new(),
        status: "open".to_owned(),
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
        return Err(AppError::not_found(
            "saved_case_not_found",
            "Saved case was not found.",
        ));
    }
    write_pins(&state.project_root, &pins)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_regression(
    State(state): State<Arc<AppState>>,
    AxumPath(params): AxumPath<HashMap<String, String>>,
    Json(request): Json<UpdatePinnedCaseRequest>,
) -> Result<Json<PinnedCase>, AppError> {
    if request.note.len() > 2_000 || !matches!(request.status.as_str(), "open" | "fixed") {
        return Err(AppError::bad_request(
            "invalid_saved_case_update",
            "Saved-case note must be at most 2,000 characters and status must be open or fixed.",
        ));
    }
    let pin_id = params.get("pin_id").context("saved case ID is missing")?;
    let _guard = state
        .pin_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("saved cases are unavailable"))?;
    let mut pins = read_pins(&state.project_root)?;
    let pin = pins
        .iter_mut()
        .find(|pin| pin.id == *pin_id)
        .ok_or_else(|| AppError::not_found("saved_case_not_found", "Saved case was not found."))?;
    pin.note = request.note;
    pin.status = request.status;
    let response = pin.clone();
    write_pins(&state.project_root, &pins)?;
    Ok(Json(response))
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
    let dataset_file = load_staged_source(root, &request.files.dataset)?;
    let baseline_file = load_staged_source(root, &request.files.baseline)?;
    let candidate_file = load_staged_source(root, &request.files.candidate)?;
    let schema_file = request
        .files
        .schema
        .as_ref()
        .map(|source| load_staged_source(root, source))
        .transpose()?;
    for file in [&dataset_file, &baseline_file, &candidate_file] {
        validate_source_file(file)?;
    }
    let project_id = request.project_id.clone();
    let projects_root = root.join(".structtrace/ui/projects");
    std::fs::create_dir_all(&projects_root)?;
    let destination = projects_root.join(&project_id);
    if destination.exists() {
        anyhow::ensure!(
            !std::fs::symlink_metadata(&destination)?
                .file_type()
                .is_symlink(),
            "project destination must not be a symbolic link"
        );
        anyhow::ensure!(
            destination.is_dir(),
            "project destination must be a directory"
        );
    }
    let scratch = tempfile::Builder::new()
        .prefix(".structtrace-ui-")
        .tempdir_in(&projects_root)?;
    let build = scratch.path().join("build");
    let staging = scratch.path().join("inputs");
    std::fs::create_dir(&staging)?;

    let dataset_bytes = normalized_jsonl(&dataset_file)?;
    let baseline_bytes = normalize_simple_outputs(
        &normalized_jsonl(&baseline_file)?,
        &request.mapping.baseline_id,
        &request.mapping.baseline_output,
        envelope_mappings(&request.mapping, false),
    )?;
    let candidate_bytes = normalize_simple_outputs(
        &normalized_jsonl(&candidate_file)?,
        &request.mapping.candidate_id,
        &request.mapping.candidate_output,
        envelope_mappings(&request.mapping, true),
    )?;
    let dataset_path = staging.join("dataset.jsonl");
    let baseline_path = staging.join("baseline.jsonl");
    let candidate_path = staging.join("candidate.jsonl");
    std::fs::write(&dataset_path, &dataset_bytes)?;
    std::fs::write(&baseline_path, &baseline_bytes)?;
    std::fs::write(&candidate_path, &candidate_bytes)?;
    let (schema_bytes, schema_provenance) = match schema_file.as_ref() {
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
        .filter(|rule| !matches!(rule.kind, RuleKind::KeyedArray))
        .map(|rule| {
            let kind = match rule.kind {
                RuleKind::Exact => "exact".to_owned(),
                RuleKind::NormalizedString => "normalized_string".to_owned(),
                RuleKind::CanonicalDate => format!(
                    "canonical_date:{}",
                    rule.formats.as_deref().unwrap_or("iso")
                ),
                RuleKind::ExactInteger => "exact_integer".to_owned(),
                RuleKind::DecimalExact => "decimal_exact".to_owned(),
                RuleKind::DecimalTolerance => format!(
                    "decimal_tolerance:{}",
                    rule.tolerance.as_deref().unwrap_or("0.01")
                ),
                RuleKind::KeyedArray => unreachable!("keyed arrays are handled separately"),
                RuleKind::RequiredFields => "exact".to_owned(),
            };
            format!("{}={kind}", rule.pointer)
        })
        .collect::<Vec<_>>();
    let keyed_arrays = request
        .rules
        .iter()
        .filter(|rule| matches!(rule.kind, RuleKind::KeyedArray))
        .map(|rule| {
            let keys = rule.keys.as_deref().unwrap_or("").trim();
            anyhow::ensure!(
                !keys.is_empty(),
                "keyed array {} requires at least one item key",
                rule.pointer
            );
            let fields = rule.fields.as_deref().unwrap_or("").trim();
            Ok(if fields.is_empty() {
                format!("{}={keys}", rule.pointer)
            } else {
                format!("{}={keys};{fields}", rule.pointer)
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    initialize_from_outputs(FromOutputsOptions {
        destination: &build,
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
        keyed_arrays: &keyed_arrays,
        financial_invariants: request.financial_invariants,
        exact_json: false,
        gate_mode: request.gate_mode,
        min_cases: request.min_cases,
    })?;
    let config_path = build.join("structtrace.yaml");
    let mut config = Config::load(&config_path)?;
    let ordinary_rules = request
        .rules
        .iter()
        .filter(|rule| !matches!(rule.kind, RuleKind::KeyedArray));
    for (rule, evaluator) in ordinary_rules.zip(config.evaluators.iter_mut()) {
        if matches!(rule.kind, RuleKind::RequiredFields) {
            evaluator.kind = EvaluatorKind::RequiredFields {
                pointers: vec![rule.pointer.clone()],
            };
        }
        if matches!(rule.kind, RuleKind::NormalizedString) && rule.case_insensitive == Some(false) {
            evaluator.kind = EvaluatorKind::NormalizedString {
                pointer: rule.pointer.clone(),
                expected_pointer: rule.pointer.clone(),
                case_insensitive: false,
            };
        }
    }
    config.project.name = request.name;
    config.project.description = Some(format!(
        "{} compared with {} through StructTrace Local",
        request.baseline_name, request.candidate_name
    ));
    std::fs::write(&config_path, serde_yaml_ng::to_string(&config)?)?;
    if destination.exists() {
        for relative in [
            "structtrace.yaml",
            "data/golden.jsonl",
            "schemas/output.schema.json",
            "outputs/baseline.jsonl",
            "outputs/candidate.jsonl",
            "README.md",
            "ONBOARDING.md",
            ".gitignore",
        ] {
            let source = build.join(relative);
            let target = destination.join(relative);
            let bytes = structtrace_core::hashing::read_bounded(
                &source,
                MAX_FILE_BYTES,
                "prepared project artifact",
            )?;
            atomic_write(&target, &bytes)?;
        }
        std::fs::remove_dir_all(&build)?;
    } else {
        std::fs::rename(&build, &destination)?;
    }
    let run =
        structtrace_engine::run_recorded(&destination, &destination.join("structtrace.yaml"))?;
    Ok((run, schema_provenance))
}

fn validate_comparison_request(request: &ComparisonRequest) -> anyhow::Result<()> {
    validate_project_id(&request.project_id)?;
    anyhow::ensure!(
        !request.name.trim().is_empty(),
        "comparison name is required"
    );
    anyhow::ensure!(
        !request.rules.is_empty(),
        "select at least one correctness rule"
    );
    anyhow::ensure!(request.min_cases > 0, "minimum cases must be at least one");
    if request.gate_mode == GateMode::Release {
        anyhow::ensure!(
            request.min_cases >= 100,
            "release decisions require at least 100 independent cases"
        );
    }
    for source in [
        &request.files.dataset,
        &request.files.baseline,
        &request.files.candidate,
    ] {
        validate_source_id(&source.source_id)?;
    }
    if let Some(source) = &request.files.schema {
        validate_source_id(&source.source_id)?;
    }
    Ok(())
}

fn validate_source_file(file: &BrowserFile) -> anyhow::Result<()> {
    anyhow::ensure!(!file.name.trim().is_empty(), "source file name is missing");
    anyhow::ensure!(
        file.content.len() <= MAX_FILE_BYTES,
        "{} exceeds 32 MiB",
        file.name
    );
    anyhow::ensure!(!file.content.trim().is_empty(), "{} is empty", file.name);
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
    envelope_mappings: [(&str, Option<&str>); 6],
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
        for (key, pointer) in envelope_mappings {
            if let Some(item) = pointer
                .filter(|pointer| !pointer.is_empty())
                .and_then(|pointer| value.pointer(pointer))
            {
                normalized.insert(key.to_owned(), item.clone());
            }
        }
        serde_json::to_writer(&mut output, &Value::Object(normalized))?;
        output.push(b'\n');
    }
    Ok(output)
}

fn envelope_mappings(mapping: &MappingRequest, candidate: bool) -> [(&str, Option<&str>); 6] {
    if candidate {
        [
            ("status", mapping.candidate_status.as_deref()),
            ("error", mapping.candidate_error.as_deref()),
            ("latency_ms", mapping.candidate_latency.as_deref()),
            ("usage", mapping.candidate_usage.as_deref()),
            ("cost", mapping.candidate_cost.as_deref()),
            ("metadata", mapping.candidate_metadata.as_deref()),
        ]
    } else {
        [
            ("status", mapping.baseline_status.as_deref()),
            ("error", mapping.baseline_error.as_deref()),
            ("latency_ms", mapping.baseline_latency.as_deref()),
            ("usage", mapping.baseline_usage.as_deref()),
            ("cost", mapping.baseline_cost.as_deref()),
            ("metadata", mapping.baseline_metadata.as_deref()),
        ]
    }
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
        project_id: ui_project_id(run_dir),
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
    make_owner_only_directory(&directory)?;
    let path = directory.join("regressions.json");
    atomic_write(&path, &serde_json::to_vec_pretty(pins)?)?;
    Ok(())
}

fn staged_sources_dir(project_root: &Path) -> PathBuf {
    project_root.join(".structtrace/ui/staged-sources")
}

fn stage_browser_file(
    root: &Path,
    kind: String,
    file: BrowserFile,
) -> anyhow::Result<StagedSource> {
    let source_id = Ulid::new().to_string();
    let bytes = file.content.as_bytes();
    let staged = StagedSource {
        source_id: source_id.clone(),
        kind,
        name: file.name,
        format: file.format,
        hash: structtrace_core::hashing::hash_bytes(bytes),
        bytes: bytes.len(),
    };
    let directory = staged_sources_dir(root);
    std::fs::create_dir_all(&directory)?;
    make_owner_only_directory(&directory)?;
    atomic_write(&directory.join(format!("{source_id}.data")), bytes)?;
    atomic_write(
        &directory.join(format!("{source_id}.json")),
        &serde_json::to_vec_pretty(&staged)?,
    )?;
    Ok(staged)
}

fn accepted_response(
    root: &Path,
    accepted: AcceptedBaseline,
) -> anyhow::Result<AcceptedBaselineResponse> {
    let reference = SourceReference {
        source_id: accepted.source_id.clone(),
    };
    let staged: StagedSource =
        read_json(&staged_sources_dir(root).join(format!("{}.json", reference.source_id)))?;
    let file = load_staged_source(root, &reference)?;
    Ok(AcceptedBaselineResponse {
        accepted,
        source: AcceptedSourceResponse {
            source_id: staged.source_id,
            hash: staged.hash,
            name: file.name,
            format: file.format,
            bytes: file.content.len(),
            content: file.content,
        },
    })
}

fn ui_project_id(run_dir: &Path) -> Option<String> {
    let project = run_dir.ancestors().nth(3)?;
    let parent = project.parent()?;
    (parent.file_name()?.to_str()? == "projects")
        .then(|| project.file_name()?.to_str().map(str::to_owned))
        .flatten()
}

fn validate_project_id(project_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        project_id.len() >= 8
            && project_id.len() <= 64
            && project_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-'),
        "project ID is invalid"
    );
    Ok(())
}

fn validate_source_id(source_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        source_id.len() == 26 && source_id.parse::<Ulid>().is_ok(),
        "source reference is invalid"
    );
    Ok(())
}

fn load_staged_source(root: &Path, reference: &SourceReference) -> anyhow::Result<BrowserFile> {
    validate_source_id(&reference.source_id)?;
    let directory = staged_sources_dir(root);
    let metadata: StagedSource =
        read_json(&directory.join(format!("{}.json", reference.source_id)))?;
    anyhow::ensure!(
        metadata.source_id == reference.source_id,
        "source metadata does not match its reference"
    );
    let bytes = structtrace_core::hashing::read_bounded(
        &directory.join(format!("{}.data", reference.source_id)),
        MAX_FILE_BYTES,
        "staged comparison source",
    )?;
    anyhow::ensure!(
        structtrace_core::hashing::hash_bytes(&bytes) == metadata.hash,
        "staged source hash does not match its recorded digest"
    );
    Ok(BrowserFile {
        name: metadata.name,
        format: metadata.format,
        content: String::from_utf8(bytes).context("staged source is not UTF-8")?,
    })
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .context("atomic write destination has no parent")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        Ulid::new()
    ));
    std::fs::write(&temporary, bytes)?;
    make_owner_only_file(&temporary)?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(unix)]
fn make_owner_only_directory(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_owner_only_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_owner_only_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_owner_only_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn strip_draft_source_contents(value: &mut Value) {
    let Some(sources) = value.get_mut("sources").and_then(Value::as_object_mut) else {
        return;
    };
    for source in sources.values_mut().filter_map(Value::as_object_mut) {
        source.remove("content");
    }
}

fn hydrate_draft_source_contents(root: &Path, value: &mut Value) -> anyhow::Result<()> {
    let Some(sources) = value.get_mut("sources").and_then(Value::as_object_mut) else {
        return Ok(());
    };
    for source in sources.values_mut().filter_map(Value::as_object_mut) {
        let source_id = source
            .get("sourceId")
            .and_then(Value::as_str)
            .context("saved source reference is missing sourceId")?;
        let file = load_staged_source(
            root,
            &SourceReference {
                source_id: source_id.to_owned(),
            },
        )?;
        source.insert("content".to_owned(), Value::String(file.content));
    }
    Ok(())
}

fn garbage_collect_staged_sources(root: &Path) -> anyhow::Result<()> {
    let mut retained = HashSet::new();
    for collection in ["projects", "archived-projects"] {
        let collection_root = root.join(".structtrace/ui").join(collection);
        if !collection_root.is_dir() {
            continue;
        }
        for project in std::fs::read_dir(collection_root)? {
            let project = project?;
            if !project.file_type()?.is_dir() || project.file_type()?.is_symlink() {
                continue;
            }
            for name in ["ui-draft.json", "accepted-baseline.json"] {
                let path = project.path().join(name);
                if !path.exists() {
                    continue;
                }
                let value: Value = read_json(&path)?;
                collect_source_ids(&value, &mut retained);
            }
        }
    }
    let staged = staged_sources_dir(root);
    if !staged.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&staged)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() || entry.file_type()?.is_symlink() {
            continue;
        }
        let Some(stem) = entry
            .path()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if validate_source_id(&stem).is_ok() && !retained.contains(&stem) {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn collect_source_ids(value: &Value, output: &mut HashSet<String>) {
    match value {
        Value::Object(object) => {
            if let Some(source_id) = object.get("sourceId").and_then(Value::as_str) {
                output.insert(source_id.to_owned());
            }
            object
                .values()
                .for_each(|child| collect_source_ids(child, output));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_source_ids(item, output)),
        _ => {}
    }
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
    fn capability_index_uses_rooted_assets_on_deep_links() {
        let html = render_index("test-capability");
        assert!(html.contains("/test-capability/assets/app.js"));
        assert!(html.contains("/test-capability/assets/app.css"));
        assert!(!html.contains("./assets/"));
    }

    #[test]
    fn persisted_draft_does_not_contain_source_bytes() {
        let mut draft = serde_json::json!({"sources": {"dataset": {"sourceId": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "content": "sensitive"}}});
        strip_draft_source_contents(&mut draft);
        assert_eq!(draft.pointer("/sources/dataset/content"), None);
        assert_eq!(
            draft
                .pointer("/sources/dataset/sourceId")
                .and_then(Value::as_str),
            Some("01ARZ3NDEKTSV4RRFFQ69G5FAV")
        );
    }

    #[test]
    fn staged_source_hash_tampering_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let source_id = Ulid::new().to_string();
        let directory = staged_sources_dir(root.path());
        std::fs::create_dir_all(&directory).unwrap();
        let metadata = StagedSource {
            source_id: source_id.clone(),
            kind: "dataset".to_owned(),
            name: "data.jsonl".to_owned(),
            format: InputFormat::Jsonl,
            hash: structtrace_core::hashing::hash_bytes(b"original"),
            bytes: 8,
        };
        std::fs::write(
            directory.join(format!("{source_id}.json")),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();
        std::fs::write(directory.join(format!("{source_id}.data")), b"tampered").unwrap();
        let error = load_staged_source(root.path(), &SourceReference { source_id }).unwrap_err();
        assert!(error.to_string().contains("hash"));
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
