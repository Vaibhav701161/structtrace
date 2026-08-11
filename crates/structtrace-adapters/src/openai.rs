//! Focused OpenAI-compatible chat-completions adapter.

use std::{str::FromStr, time::Duration};

use futures::{StreamExt, stream};
use minijinja::{Environment, UndefinedBehavior, context};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use structtrace_core::{
    config::OpenAiCompatibleConfig,
    dataset::VariantCase,
    output::{Cost, OutputError, OutputStatus, Usage, VariantOutput},
};
use tokio::time::{Instant, sleep, timeout};
use url::Url;

use crate::command::AdapterRun;

const PROVIDER_ENVELOPE_OVERHEAD_BYTES: usize = 1024 * 1024;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

/// Execute matched cases against an explicitly configured endpoint.
pub async fn run_openai_compatible(
    config: &OpenAiCompatibleConfig,
    cases: &[VariantCase],
    output_schema: Option<&Value>,
    max_output_bytes: usize,
) -> AdapterRun {
    let api_key = match config.api_key_env.as_deref() {
        Some(name) => match std::env::var(name) {
            Ok(value) if !value.is_empty() => Some(value),
            _ => {
                return AdapterRun {
                    rows: cases
                        .iter()
                        .map(|case| {
                            error_output(
                                &case.id,
                                "missing_secret",
                                &format!("required environment variable {name} is not set"),
                                None,
                                Vec::new(),
                            )
                        })
                        .collect(),
                    stderr: Vec::new(),
                    protocol_errors: Vec::new(),
                };
            }
        },
        None => None,
    };
    let endpoint = match endpoint_url(&config.base_url) {
        Ok(value) => value,
        Err(error) => {
            return AdapterRun {
                rows: cases
                    .iter()
                    .map(|case| {
                        error_output(
                            &case.id,
                            "invalid_endpoint",
                            &error.to_string(),
                            None,
                            Vec::new(),
                        )
                    })
                    .collect(),
                stderr: Vec::new(),
                protocol_errors: vec![error.to_string()],
            };
        }
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return AdapterRun {
                rows: cases
                    .iter()
                    .map(|case| {
                        error_output(
                            &case.id,
                            "http_client",
                            &error.to_string(),
                            None,
                            Vec::new(),
                        )
                    })
                    .collect(),
                stderr: Vec::new(),
                protocol_errors: vec![error.to_string()],
            };
        }
    };
    let concurrency = config.concurrency.max(1);
    let rows = stream::iter(cases.iter().cloned().map(|case| {
        run_case(
            client.clone(),
            endpoint.clone(),
            api_key.clone(),
            config.clone(),
            output_schema.cloned(),
            case,
            max_output_bytes,
        )
    }))
    .buffered(concurrency)
    .collect()
    .await;
    AdapterRun {
        rows,
        stderr: Vec::new(),
        protocol_errors: Vec::new(),
    }
}

async fn run_case(
    client: reqwest::Client,
    endpoint: Url,
    api_key: Option<String>,
    config: OpenAiCompatibleConfig,
    output_schema: Option<Value>,
    case: VariantCase,
    max_output_bytes: usize,
) -> VariantOutput {
    let case_id = case.id.clone();
    let total_deadline = Duration::from_millis(config.timeout_ms);
    match timeout(
        total_deadline,
        run_case_with_retries(
            client,
            endpoint,
            api_key,
            config,
            output_schema,
            case,
            max_output_bytes,
        ),
    )
    .await
    {
        Ok(output) => output,
        Err(_) => error_output(
            &case_id,
            "timeout",
            "provider case exceeded the configured total deadline",
            Some(u64::try_from(total_deadline.as_millis()).unwrap_or(u64::MAX)),
            Vec::new(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_case_with_retries(
    client: reqwest::Client,
    endpoint: Url,
    api_key: Option<String>,
    config: OpenAiCompatibleConfig,
    output_schema: Option<Value>,
    case: VariantCase,
    max_output_bytes: usize,
) -> VariantOutput {
    let prompt = match render_prompt(&config.request.user_template, &case) {
        Ok(value) => value,
        Err(error) => {
            return error_output(
                &case.id,
                "template_error",
                &error.to_string(),
                None,
                Vec::new(),
            );
        }
    };
    let mut messages = Vec::new();
    if let Some(system) = &config.request.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.push(json!({"role": "user", "content": prompt.clone()}));
    let mut body = json!({
        "model": config.model,
        "messages": messages,
        "temperature": config.request.temperature,
        "max_tokens": config.request.max_output_tokens,
    });
    if let Some(structured) = &config.structured_output {
        let response_format = match structured.mode.as_str() {
            "json_schema" => match output_schema {
                Some(schema) => json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": "structtrace_output",
                        "strict": true,
                        "schema": schema,
                    }
                }),
                None => {
                    return error_output(
                        &case.id,
                        "missing_schema",
                        "json_schema mode requires a loaded schema",
                        None,
                        Vec::new(),
                    );
                }
            },
            "json_object" => json!({"type": "json_object"}),
            other => {
                return error_output(
                    &case.id,
                    "unsupported_structured_output",
                    &format!("unsupported structured-output mode `{other}`"),
                    None,
                    Vec::new(),
                );
            }
        };
        body.as_object_mut()
            .expect("request body is an object")
            .insert("response_format".to_owned(), response_format);
    }
    let started = Instant::now();
    let mut retries = Vec::new();
    for attempt in 0..=config.retries {
        let attempt_started = Instant::now();
        let mut request = client
            .post(endpoint.clone())
            .header("content-type", "application/json")
            .json(&body);
        if let Some(api_key) = &api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await;
        match response {
            Err(error) => {
                if attempt < config.retries {
                    retries.push(json!({
                        "attempt": attempt + 1,
                        "kind": "transport_error",
                        "message": error.to_string(),
                        "latency_ms": elapsed_ms(attempt_started),
                    }));
                    sleep(retry_delay(attempt, None)).await;
                    continue;
                }
                return error_output(
                    &case.id,
                    if error.is_timeout() {
                        "timeout"
                    } else {
                        "provider_transport"
                    },
                    &error.to_string(),
                    Some(elapsed_ms(started)),
                    retries,
                );
            }
            Ok(response) => {
                let status = response.status();
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(Duration::from_secs);
                let response_limit =
                    max_output_bytes.saturating_add(PROVIDER_ENVELOPE_OVERHEAD_BYTES);
                let payload = match read_limited_response(response, response_limit).await {
                    Ok(bytes) => bytes,
                    Err(message) => {
                        return error_output(
                            &case.id,
                            "provider_response_limit",
                            &message,
                            Some(elapsed_ms(started)),
                            retries,
                        );
                    }
                };
                let value = match structtrace_core::strict_json::value_from_slice(&payload) {
                    Ok(value) => value,
                    Err(error) => {
                        return error_output(
                            &case.id,
                            "malformed_provider_response",
                            &format!("HTTP {status}: {error}"),
                            Some(elapsed_ms(started)),
                            retries,
                        );
                    }
                };
                if !status.is_success() {
                    if attempt < config.retries
                        && (status.as_u16() == 429 || status.is_server_error())
                    {
                        retries.push(json!({
                            "attempt": attempt + 1,
                            "kind": "provider_error",
                            "status": status.as_u16(),
                            "response": value,
                            "latency_ms": elapsed_ms(attempt_started),
                        }));
                        sleep(retry_delay(attempt, retry_after)).await;
                        continue;
                    }
                    return provider_error_output(
                        &case.id,
                        status.as_u16(),
                        value,
                        Some(elapsed_ms(started)),
                        retries,
                    );
                }
                return success_output(
                    &case.id,
                    &config,
                    value,
                    elapsed_ms(started),
                    retries,
                    &prompt,
                    max_output_bytes,
                );
            }
        }
    }
    error_output(
        &case.id,
        "provider_error",
        "retry loop ended without a response",
        Some(elapsed_ms(started)),
        retries,
    )
}

fn retry_delay(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| {
            let exponent = attempt.min(8);
            Duration::from_millis(100_u64.saturating_mul(1_u64 << exponent))
        })
        .min(MAX_RETRY_DELAY)
}

async fn read_limited_response(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(format!(
            "provider response Content-Length exceeded the {max_bytes}-byte safety limit"
        ));
    }
    let mut payload = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("could not read provider response: {error}"))?;
        if payload.len().saturating_add(chunk.len()) > max_bytes {
            return Err(format!(
                "provider response exceeded the {max_bytes}-byte safety limit"
            ));
        }
        payload.extend_from_slice(&chunk);
    }
    Ok(payload)
}

fn success_output(
    case_id: &str,
    config: &OpenAiCompatibleConfig,
    response: Value,
    latency_ms: u64,
    retries: Vec<Value>,
    rendered_prompt: &str,
    max_output_bytes: usize,
) -> VariantOutput {
    let Some(content) = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
    else {
        return error_output(
            case_id,
            "malformed_provider_response",
            "response did not contain choices[0].message.content as text",
            Some(latency_ms),
            retries,
        );
    };
    if content.len() > max_output_bytes {
        return error_output(
            case_id,
            "output_limit",
            &format!(
                "provider output was {} bytes; configured limit is {}",
                content.len(),
                max_output_bytes
            ),
            Some(latency_ms),
            retries,
        );
    }
    let usage = Usage {
        input_tokens: response
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64),
        output_tokens: response
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64),
    };
    let cost = calculate_cost(config, &usage);
    let metadata = json!({
        "request_model": config.model,
        "provider_model": response.get("model"),
        "response_id": response.get("id"),
        "finish_reason": response.pointer("/choices/0/finish_reason"),
        "rendered_prompt": rendered_prompt,
        "provider_response": response,
    });
    VariantOutput {
        case_id: case_id.to_owned(),
        status: OutputStatus::Ok,
        raw_output: Some(content.to_owned()),
        parsed_output: None,
        error: None,
        latency_ms: Some(latency_ms),
        usage: Some(usage),
        cost,
        metadata,
        retries,
    }
}

fn calculate_cost(config: &OpenAiCompatibleConfig, usage: &Usage) -> Option<Cost> {
    let pricing = config.pricing.as_ref()?;
    let input_price = Decimal::from_str(&pricing.input_per_million).ok()?;
    let output_price = Decimal::from_str(&pricing.output_per_million).ok()?;
    let input = Decimal::from(usage.input_tokens?);
    let output = Decimal::from(usage.output_tokens?);
    let amount = input
        .checked_mul(input_price)?
        .checked_add(output.checked_mul(output_price)?)?
        .checked_div(Decimal::from(1_000_000_u64))?;
    Some(Cost {
        amount: amount.normalize().to_string(),
        currency: pricing.currency.clone(),
    })
}

fn render_prompt(template: &str, case: &VariantCase) -> anyhow::Result<String> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    Ok(environment.render_str(
        template,
        context! {
            input => case.input,
            metadata => case.metadata,
        },
    )?)
}

fn endpoint_url(base_url: &str) -> anyhow::Result<Url> {
    let base = format!("{}/", base_url.trim_end_matches('/'));
    Ok(Url::parse(&base)?.join("chat/completions")?)
}

fn error_output(
    case_id: &str,
    kind: &str,
    message: &str,
    latency_ms: Option<u64>,
    retries: Vec<Value>,
) -> VariantOutput {
    VariantOutput {
        case_id: case_id.to_owned(),
        status: OutputStatus::Error,
        raw_output: None,
        parsed_output: None,
        error: Some(OutputError {
            kind: kind.to_owned(),
            message: message.to_owned(),
            fingerprint: None,
        }),
        latency_ms,
        usage: None,
        cost: None,
        metadata: Value::Object(serde_json::Map::new()),
        retries,
    }
}

fn provider_error_output(
    case_id: &str,
    http_status: u16,
    provider_response: Value,
    latency_ms: Option<u64>,
    retries: Vec<Value>,
) -> VariantOutput {
    let provider_code = provider_response
        .pointer("/error/code")
        .or_else(|| provider_response.pointer("/code"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    VariantOutput {
        case_id: case_id.to_owned(),
        status: OutputStatus::Error,
        raw_output: None,
        parsed_output: None,
        error: Some(OutputError {
            kind: "provider_error".to_owned(),
            message: format!("Provider rejected the request with HTTP status {http_status}."),
            fingerprint: None,
        }),
        latency_ms,
        usage: None,
        cost: None,
        metadata: json!({
            "provider_error": {
                "http_status": http_status,
                "provider_code": provider_code,
                "message": "Provider rejected the request."
            },
            "provider_response": provider_response
        }),
        retries,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
    use serde_json::json;

    use structtrace_core::config::{OpenAiRequestConfig, PricingConfig};

    use super::*;

    #[derive(Clone)]
    struct MockState {
        attempts: Arc<Mutex<usize>>,
        mode: &'static str,
    }

    async fn handler(
        State(state): State<MockState>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let attempt = {
            let mut attempts = state.attempts.lock().unwrap();
            *attempts += 1;
            *attempts
        };
        if state.mode == "retry" && attempt == 1 {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "retry"})),
            );
        }
        if state.mode == "error" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "bad request"})),
            );
        }
        if state.mode == "secret-error" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "invalid_request_error",
                        "message": "echoed SECRET_DOCUMENT_91f2"
                    }
                })),
            );
        }
        if state.mode == "slow" {
            sleep(Duration::from_millis(100)).await;
        }
        if state.mode == "malformed" {
            return (StatusCode::OK, Json(json!({"choices": []})));
        }
        let user = body
            .pointer("/messages/1/content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        (
            StatusCode::OK,
            Json(json!({
                "id": "response-1",
                "model": "mock-model",
                "choices": [{
                    "message": {"content": serde_json::json!({"label": user}).to_string()},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 100, "completion_tokens": 20}
            })),
        )
    }

    async fn server(mode: &'static str) -> (String, Arc<Mutex<usize>>) {
        let attempts = Arc::new(Mutex::new(0));
        let state = MockState {
            attempts: attempts.clone(),
            mode,
        };
        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{address}/v1"), attempts)
    }

    fn config(base_url: String, retries: u32) -> OpenAiCompatibleConfig {
        OpenAiCompatibleConfig {
            base_url,
            api_key_env: Some("PATH".to_owned()),
            model: "mock-model".to_owned(),
            request: OpenAiRequestConfig {
                system: Some("Classify".to_owned()),
                user_template: "{{ input.text }}".to_owned(),
                temperature: 0.0,
                max_output_tokens: 100,
            },
            structured_output: None,
            timeout_ms: 1_000,
            concurrency: 2,
            retries,
            pricing: Some(PricingConfig {
                input_per_million: "0.50".to_owned(),
                output_per_million: "1.50".to_owned(),
                currency: "USD".to_owned(),
            }),
        }
    }

    fn case() -> VariantCase {
        VariantCase::from_parts(
            structtrace_core::dataset::ExecutionToken::new("openai-test", 0),
            json!({"text": "accepted"}),
            None,
        )
    }

    #[test]
    fn openai_template_cannot_access_expected() {
        assert!(render_prompt("{{ expected }}", &case()).is_err());
        assert!(render_prompt("{{ id }}", &case()).is_err());
    }

    #[tokio::test]
    async fn captures_content_usage_cost_and_provider_envelope() {
        let (base_url, attempts) = server("success").await;
        let run = run_openai_compatible(&config(base_url, 0), &[case()], None, 1024).await;
        assert_eq!(run.rows[0].status, OutputStatus::Ok);
        assert_eq!(run.rows[0].usage.as_ref().unwrap().input_tokens, Some(100));
        assert_eq!(run.rows[0].cost.as_ref().unwrap().amount, "0.00008");
        assert!(run.rows[0].metadata.get("provider_response").is_some());
        assert_eq!(*attempts.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn unauthenticated_local_endpoint_is_supported() {
        let (base_url, attempts) = server("success").await;
        let mut local = config(base_url, 0);
        local.api_key_env = None;
        let run = run_openai_compatible(&local, &[case()], None, 1024).await;
        assert_eq!(run.rows[0].status, OutputStatus::Ok);
        assert_eq!(*attempts.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn retries_only_when_explicitly_configured() {
        let (base_url, attempts) = server("retry").await;
        let run = run_openai_compatible(&config(base_url, 1), &[case()], None, 1024).await;
        assert_eq!(run.rows[0].status, OutputStatus::Ok);
        assert_eq!(run.rows[0].retries.len(), 1);
        assert_eq!(*attempts.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn provider_and_malformed_responses_remain_failures() {
        for mode in ["error", "malformed"] {
            let (base_url, _) = server(mode).await;
            let run = run_openai_compatible(&config(base_url, 0), &[case()], None, 1024).await;
            assert_eq!(run.rows[0].status, OutputStatus::Error);
        }
    }

    #[tokio::test]
    async fn provider_error_body_is_not_embedded_in_error_message() {
        let (base_url, _) = server("secret-error").await;
        let run = run_openai_compatible(&config(base_url, 0), &[case()], None, 1024).await;
        let row = &run.rows[0];
        assert_eq!(row.status, OutputStatus::Error);
        assert!(
            !row.error
                .as_ref()
                .unwrap()
                .message
                .contains("SECRET_DOCUMENT")
        );
        assert_eq!(
            row.metadata.pointer("/provider_error/provider_code"),
            Some(&json!("invalid_request_error"))
        );
    }

    #[tokio::test]
    async fn total_case_deadline_bounds_all_provider_work() {
        let (base_url, _) = server("slow").await;
        let mut deadline = config(base_url, 3);
        deadline.timeout_ms = 20;
        let started = Instant::now();
        let run = run_openai_compatible(&deadline, &[case()], None, 1024).await;
        assert_eq!(
            run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
            Some("timeout")
        );
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn retry_delay_is_capped() {
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(86_400))),
            MAX_RETRY_DELAY
        );
    }

    #[test]
    fn cost_overflow_is_omitted_instead_of_panicking_or_wrapping() {
        let mut pricing = config("http://127.0.0.1:1/v1".to_owned(), 0);
        pricing.pricing.as_mut().unwrap().input_per_million =
            "79228162514264337593543950335".to_owned();
        let usage = Usage {
            input_tokens: Some(u64::MAX),
            output_tokens: Some(0),
        };
        assert_eq!(calculate_cost(&pricing, &usage), None);
    }

    #[tokio::test]
    async fn oversized_provider_content_fails_closed() {
        let (base_url, _) = server("success").await;
        let run = run_openai_compatible(&config(base_url, 0), &[case()], None, 8).await;
        assert_eq!(run.rows[0].status, OutputStatus::Error);
        assert_eq!(
            run.rows[0].error.as_ref().map(|error| error.kind.as_str()),
            Some("output_limit")
        );
    }

    #[tokio::test]
    async fn oversized_provider_envelope_is_rejected_while_streaming() {
        let (base_url, _) = server("success").await;
        let response = reqwest::Client::new()
            .post(endpoint_url(&base_url).unwrap())
            .json(&json!({"messages": [{}, {"content": "accepted"}]}))
            .send()
            .await
            .unwrap();
        let error = read_limited_response(response, 8).await.unwrap_err();
        assert!(error.contains("safety limit"));
    }
}
