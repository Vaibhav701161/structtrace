//! Fail-closed project initialization.

use std::{
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde_json::json;
use structtrace_core::{
    config::{Config, DatasetFields, GateMode, LimitsConfig},
    dataset::Dataset,
    evaluation::compile_schema,
    output::{OutputError, OutputStatus, RecordedOutputs, VariantOutput},
};

use crate::InitTemplate;

const EXTRACTION_CONFIG: &str =
    include_str!("../../../examples/document-extraction/structtrace.yaml");
const EXTRACTION_SCHEMA: &str =
    include_str!("../../../examples/document-extraction/schemas/output.schema.json");
const EXTRACTION_DATASET: &str =
    include_str!("../../../examples/document-extraction/data/golden.jsonl");
const EXTRACTION_BASELINE: &str =
    include_str!("../../../examples/document-extraction/outputs/baseline.jsonl");
const EXTRACTION_CANDIDATE: &str =
    include_str!("../../../examples/document-extraction/outputs/candidate.jsonl");

/// Explicit inputs for guided recorded-output onboarding.
pub struct FromOutputsOptions<'a> {
    pub destination: &'a Path,
    pub dataset: &'a Path,
    pub baseline: &'a Path,
    pub candidate: &'a Path,
    pub schema: &'a Path,
    pub dataset_fields: DatasetFields,
    pub output_fields: SimpleOutputFields,
    pub correctness_pointers: &'a [String],
    pub field_evaluators: &'a [String],
    pub keyed_arrays: &'a [String],
    pub financial_invariants: bool,
    pub exact_json: bool,
    pub gate_mode: GateMode,
    pub min_cases: usize,
}

/// Pointer mapping for ordinary `{id, output}` JSONL exports.
#[derive(Debug, Clone)]
pub struct SimpleOutputFields {
    pub id: String,
    pub output: String,
}

fn read_or_normalize_outputs(
    path: &Path,
    dataset: &Dataset,
    fields: &SimpleOutputFields,
    limits: &LimitsConfig,
) -> anyhow::Result<RecordedOutputs> {
    let bytes = structtrace_core::hashing::read_bounded(
        path,
        limits.max_recorded_output_bytes,
        "simple recorded output",
    )?;
    if let Ok(canonical) = RecordedOutputs::from_bytes_bounded(&bytes, dataset, limits) {
        return Ok(canonical);
    }
    let fields = infer_simple_output_fields(&bytes, fields.clone())?;
    let text = std::str::from_utf8(&bytes).context("simple recorded output is not UTF-8")?;
    let mut normalized = Vec::new();
    for (index, line) in text.lines().enumerate() {
        anyhow::ensure!(
            line.len() <= limits.max_jsonl_line_bytes,
            "{}:{} exceeds the {}-byte JSONL line limit",
            path.display(),
            index + 1,
            limits.max_jsonl_line_bytes
        );
        anyhow::ensure!(
            !line.trim().is_empty(),
            "{}:{} is blank",
            path.display(),
            index + 1
        );
        let value = structtrace_core::strict_json::value_from_str(line)
            .with_context(|| format!("invalid JSON at {}:{}", path.display(), index + 1))?;
        let case_id = value
            .pointer(&fields.id)
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .with_context(|| {
                format!(
                    "{}:{}: {} must resolve to a non-empty string",
                    path.display(),
                    index + 1,
                    fields.id
                )
            })?
            .to_owned();
        let status = match value.pointer("/status").and_then(serde_json::Value::as_str) {
            None | Some("ok" | "success") => OutputStatus::Ok,
            Some("error" | "failed") => OutputStatus::Error,
            Some(other) => anyhow::bail!(
                "{}:{}: unsupported simple output status `{other}`",
                path.display(),
                index + 1
            ),
        };
        let output = value.pointer(&fields.output).cloned();
        anyhow::ensure!(
            status != OutputStatus::Ok || output.is_some(),
            "{}:{}: {} did not resolve for a successful output",
            path.display(),
            index + 1,
            fields.output
        );
        let (raw_output, parsed_output) = match output {
            Some(serde_json::Value::String(raw)) => (Some(raw), None),
            Some(value) => (None, Some(value)),
            None => (None, None),
        };
        let error = if status == OutputStatus::Error {
            Some(
                value
                    .pointer("/error")
                    .cloned()
                    .and_then(|item| serde_json::from_value::<OutputError>(item).ok())
                    .unwrap_or(OutputError {
                        kind: "imported_error".to_owned(),
                        message: "Imported output row reported an error.".to_owned(),
                        fingerprint: None,
                    }),
            )
        } else {
            None
        };
        let row = VariantOutput {
            case_id,
            status,
            raw_output,
            parsed_output,
            error,
            latency_ms: value
                .pointer("/latency_ms")
                .and_then(serde_json::Value::as_u64),
            usage: value
                .pointer("/usage")
                .cloned()
                .and_then(|item| serde_json::from_value(item).ok()),
            cost: value
                .pointer("/cost")
                .cloned()
                .and_then(|item| serde_json::from_value(item).ok()),
            metadata: value
                .pointer("/metadata")
                .cloned()
                .unwrap_or_else(|| json!({"imported": true})),
            retries: Vec::new(),
        };
        serde_json::to_writer(&mut normalized, &row)?;
        normalized.push(b'\n');
    }
    RecordedOutputs::from_bytes_bounded(&normalized, dataset, limits).map_err(Into::into)
}

fn first_jsonl_value(bytes: &[u8], label: &str) -> anyhow::Result<serde_json::Value> {
    let text = std::str::from_utf8(bytes).with_context(|| format!("{label} is not UTF-8"))?;
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .with_context(|| format!("{label} contains no records"))?;
    structtrace_core::strict_json::value_from_str(first)
        .with_context(|| format!("first {label} record is invalid JSON"))
}

fn first_existing_pointer(
    value: &serde_json::Value,
    configured: &str,
    candidates: &[&str],
) -> String {
    if value.pointer(configured).is_some() {
        return configured.to_owned();
    }
    candidates
        .iter()
        .find(|pointer| value.pointer(pointer).is_some())
        .map_or_else(|| configured.to_owned(), |pointer| (*pointer).to_owned())
}

fn infer_dataset_fields(bytes: &[u8], mut fields: DatasetFields) -> anyhow::Result<DatasetFields> {
    let first = first_jsonl_value(bytes, "dataset")?;
    fields.id = first_existing_pointer(
        &first,
        &fields.id,
        &[
            "/id",
            "/case_id",
            "/invoice_id",
            "/document_id",
            "/record_id",
        ],
    );
    fields.input = first_existing_pointer(
        &first,
        &fields.input,
        &["/input", "/payload", "/document", "/request", "/text"],
    );
    fields.expected = first_existing_pointer(
        &first,
        &fields.expected,
        &[
            "/expected",
            "/ground_truth",
            "/reference",
            "/target",
            "/label",
        ],
    );
    fields.model_visible_metadata = first_existing_pointer(
        &first,
        &fields.model_visible_metadata,
        &["/model_visible_metadata", "/visible_metadata"],
    );
    fields.metadata = first_existing_pointer(
        &first,
        &fields.metadata,
        &["/metadata", "/evaluation_metadata", "/tags"],
    );
    Ok(fields)
}

fn infer_simple_output_fields(
    bytes: &[u8],
    mut fields: SimpleOutputFields,
) -> anyhow::Result<SimpleOutputFields> {
    let first = first_jsonl_value(bytes, "output")?;
    fields.id = first_existing_pointer(
        &first,
        &fields.id,
        &[
            "/id",
            "/case_id",
            "/record_id",
            "/document_id",
            "/invoice_id",
        ],
    );
    fields.output = first_existing_pointer(
        &first,
        &fields.output,
        &[
            "/output",
            "/result",
            "/response",
            "/prediction",
            "/parsed_output",
        ],
    );
    Ok(fields)
}

fn prompt_for_field_evaluators(
    discovered: &std::collections::BTreeSet<String>,
) -> anyhow::Result<Vec<String>> {
    let pointers = discovered.iter().cloned().collect::<Vec<_>>();
    eprintln!("\nDiscovered output fields from schema, references, baseline, and candidate:");
    for (index, pointer) in pointers.iter().enumerate() {
        eprintln!("  {:>3}. {pointer}", index + 1);
    }
    eprint!("Select correctness fields by number (comma-separated): ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let indexes = answer
        .trim()
        .split(',')
        .map(|value| value.trim().parse::<usize>())
        .collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(!indexes.is_empty(), "select at least one correctness field");
    let mut selections = Vec::new();
    for index in indexes {
        let pointer = pointers
            .get(index.saturating_sub(1))
            .with_context(|| format!("field selection {index} is out of range"))?;
        let lower = pointer.to_ascii_lowercase();
        let suggested = if lower.contains("date") {
            "canonical_date"
        } else if ["total", "amount", "price", "tax", "subtotal"]
            .iter()
            .any(|term| lower.contains(term))
        {
            "decimal_exact"
        } else if ["count", "quantity", "number"]
            .iter()
            .any(|term| lower.contains(term))
        {
            "exact_integer"
        } else {
            "normalized_string"
        };
        eprint!(
            "Comparator for {pointer} [exact, normalized_string, canonical_date, exact_integer, decimal_exact, decimal_tolerance:VALUE] ({suggested}): "
        );
        std::io::stderr().flush()?;
        let mut kind = String::new();
        std::io::stdin().read_line(&mut kind)?;
        let kind = if kind.trim().is_empty() {
            suggested
        } else {
            kind.trim()
        };
        selections.push(format!("{pointer}={kind}"));
    }
    Ok(selections)
}

/// Validate existing artifacts and create a complete recorded-output project.
pub fn initialize_from_outputs(options: FromOutputsOptions<'_>) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(options.min_cases > 0, "--min-cases must be at least one");
    let limits = LimitsConfig::default();
    let dataset_bytes = structtrace_core::hashing::read_bounded(
        options.dataset,
        limits.max_dataset_bytes,
        "dataset",
    )?;
    let fields = infer_dataset_fields(&dataset_bytes, options.dataset_fields.clone())?;
    let dataset = Dataset::from_bytes_bounded(
        &dataset_bytes,
        &fields,
        limits.max_jsonl_line_bytes,
        limits.max_cases,
    )?;
    let baseline =
        read_or_normalize_outputs(options.baseline, &dataset, &options.output_fields, &limits)?;
    let candidate =
        read_or_normalize_outputs(options.candidate, &dataset, &options.output_fields, &limits)?;
    let schema_bytes =
        structtrace_core::hashing::read_bounded(options.schema, limits.max_schema_bytes, "schema")?;
    let schema = structtrace_core::strict_json::value_from_slice(&schema_bytes)?;
    compile_schema(&schema)?;

    let discovered = selectable_pointer_union(&schema, &dataset, &baseline, &candidate)?;
    let discovery_markdown = discovery_markdown(&discovered, &dataset, &baseline, &candidate);
    let correctness_pointers = options.correctness_pointers.to_vec();
    let mut field_evaluators = options.field_evaluators.to_vec();
    if !options.exact_json
        && correctness_pointers.is_empty()
        && field_evaluators.is_empty()
        && options.keyed_arrays.is_empty()
        && !options.financial_invariants
    {
        if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
            field_evaluators = prompt_for_field_evaluators(&discovered)?;
        } else {
            anyhow::bail!(
                "choose semantics explicitly with --exact-json, --correctness-pointer, --field-evaluator, --keyed-array, or --financial-invariants"
            );
        }
    }
    for pointer in &correctness_pointers {
        anyhow::ensure!(
            discovered.contains(pointer),
            "correctness pointer `{pointer}` was not present in the schema, expected values, baseline outputs, or candidate outputs; available pointers: {}",
            discovered.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    let project_name = options
        .destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("recorded-output-comparison");
    let mut evaluators = Vec::new();
    if options.exact_json {
        evaluators.push(json!({"id": "semantic_correctness", "kind": "exact_json"}));
    }
    if !correctness_pointers.is_empty() {
        evaluators.push(json!({
            "id": "selected_fields_exact",
            "kind": "json_pointers_exact",
            "pointers": correctness_pointers.iter().map(|pointer| {
                json!({"pointer": pointer, "expected_pointer": pointer})
            }).collect::<Vec<_>>()
        }));
    }
    for (index, specification) in field_evaluators.iter().enumerate() {
        let (pointer, kind) = specification
            .split_once('=')
            .with_context(|| format!("field evaluator `{specification}` must use POINTER=KIND"))?;
        anyhow::ensure!(
            discovered.contains(pointer),
            "field evaluator pointer `{pointer}` was not discovered"
        );
        let id = format!("field_{:03}", index + 1);
        let evaluator = match kind {
            "exact" => {
                json!({"id": id, "kind": "json_pointer_exact", "pointer": pointer, "expected_pointer": pointer})
            }
            "normalized_string" => {
                json!({"id": id, "kind": "normalized_string", "pointer": pointer, "expected_pointer": pointer, "case_insensitive": true})
            }
            "canonical_date" => {
                json!({"id": id, "kind": "canonical_date", "pointer": pointer, "expected_pointer": pointer, "formats": ["iso"]})
            }
            value if value.starts_with("canonical_date:") => {
                let formats = value
                    .trim_start_matches("canonical_date:")
                    .split(',')
                    .map(str::trim)
                    .filter(|format| !format.is_empty())
                    .collect::<Vec<_>>();
                anyhow::ensure!(
                    !formats.is_empty(),
                    "canonical date formats must not be empty"
                );
                json!({"id": id, "kind": "canonical_date", "pointer": pointer, "expected_pointer": pointer, "formats": formats})
            }
            "exact_integer" => {
                json!({"id": id, "kind": "numeric_tolerance", "pointer": pointer, "expected_pointer": pointer, "exact_integer": true})
            }
            "decimal_exact" => {
                json!({"id": id, "kind": "numeric_tolerance", "pointer": pointer, "expected_pointer": pointer, "absolute": "0"})
            }
            value if value.starts_with("decimal_tolerance:") => {
                let tolerance = value.trim_start_matches("decimal_tolerance:");
                anyhow::ensure!(!tolerance.is_empty(), "decimal tolerance must not be empty");
                json!({"id": id, "kind": "numeric_tolerance", "pointer": pointer, "expected_pointer": pointer, "absolute": tolerance})
            }
            _ => anyhow::bail!("unsupported field evaluator kind `{kind}`"),
        };
        evaluators.push(evaluator);
    }
    for (index, specification) in options.keyed_arrays.iter().enumerate() {
        let (pointer, semantics) = specification.split_once('=').with_context(|| {
            format!("keyed array `{specification}` must use ARRAY_POINTER=KEY[,KEY...]")
        })?;
        anyhow::ensure!(
            discovered.contains(pointer),
            "keyed-array pointer `{pointer}` was not discovered"
        );
        let (keys, field_specs) = semantics
            .split_once(';')
            .map_or((semantics, ""), |(keys, fields)| (keys, fields));
        let keys = keys
            .split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .collect::<Vec<_>>();
        anyhow::ensure!(
            !keys.is_empty(),
            "keyed array requires at least one item key"
        );
        let mut fields = if field_specs.is_empty() {
            keys.iter()
                .map(|key| json!({"pointer": key, "evaluator": "exact"}))
                .collect::<Vec<_>>()
        } else {
            field_specs
                .split(',')
                .map(|field| {
                    let (field_pointer, evaluator) = field.split_once(':').with_context(|| {
                        format!("keyed-array field `{field}` must use POINTER:EVALUATOR")
                    })?;
                    let value = match evaluator {
                        "exact" | "normalized_string" | "exact_integer" | "decimal_exact"
                        | "canonical_date" => {
                            json!({"pointer": field_pointer, "evaluator": evaluator})
                        }
                        value if value.starts_with("decimal_tolerance:") => json!({
                            "pointer": field_pointer,
                            "evaluator": "decimal_tolerance",
                            "absolute": value.trim_start_matches("decimal_tolerance:")
                        }),
                        _ => anyhow::bail!("unsupported keyed-array field evaluator `{evaluator}`"),
                    };
                    Ok(value)
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };
        fields.sort_by_key(|field| {
            field
                .get("pointer")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        });
        evaluators.push(json!({
            "id": format!("keyed_array_{:03}", index + 1),
            "kind": "keyed_array",
            "pointer": pointer,
            "expected_pointer": pointer,
            "keys": keys,
            "fields": fields
        }));
    }
    if options.financial_invariants {
        evaluators.push(json!({
            "id": "financial_invariants",
            "kind": "financial_invariants",
            "line_items_pointer": "/line_items",
            "subtotal_pointer": "/subtotal",
            "tax_pointer": "/tax",
            "total_pointer": "/total",
            "absolute": "0.01"
        }));
    }
    let outcome_evaluators = evaluators
        .iter()
        .filter_map(|evaluator| evaluator.get("id").cloned())
        .collect::<Vec<_>>();
    let gate = match options.gate_mode {
        GateMode::Advisory => json!({"mode": "advisory", "min_cases": options.min_cases}),
        GateMode::Regression | GateMode::Release => {
            let mut gate = json!({
                "mode": if options.gate_mode == GateMode::Release {"release"} else {"regression"},
                "min_cases": options.min_cases,
                "min_unique_cases": options.min_cases,
                "max_duplicate_case_rate": 0.01,
                "min_primary_fully_evaluated_rate": 0.99,
                "max_primary_component_error_rate": 0.01,
                "max_primary_component_not_applicable_rate": 0.0,
                "max_primary_component_unscored_rate": 0.0,
                "max_deployment_regression_pp": 0.0
            });
            if options.gate_mode == GateMode::Release {
                gate["min_candidate_deployment_success_rate"] = json!(0.95);
                gate["min_candidate_parse_validity"] = json!(1.0);
                gate["min_candidate_schema_validity"] = json!(1.0);
                gate["max_candidate_valid_but_wrong_rate"] = json!(0.02);
            }
            gate
        }
    };
    let value = json!({
        "version": 3,
        "project": {"name": project_name, "description": "Recorded baseline and candidate comparison with user-selected correctness semantics"},
        "storage": {"root": ".structtrace", "process_logs": {"mode": "off"}},
        "dataset": {"path": "data/golden.jsonl", "format": "jsonl", "fields": fields},
        "schema": {"path": "schemas/output.schema.json"},
        "variants": {
            "baseline": {"kind": "recorded", "path": "outputs/baseline.jsonl"},
            "candidate": {"kind": "recorded", "path": "outputs/candidate.jsonl"}
        },
        "evaluators": evaluators,
        "outcomes": {"correct": {"all_of": outcome_evaluators}},
        "analysis": {"primary_outcome": "correct"},
        "gate": gate
    });
    let config: Config = serde_json::from_value(value)?;
    Config::validate(config.clone())?;
    let yaml = serde_yaml_ng::to_string(&config)?;

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
        anyhow::ensure!(
            !options.destination.join(relative).exists(),
            "refusing to overwrite {}",
            options.destination.join(relative).display()
        );
    }
    write_new_bytes(
        &options.destination.join("data/golden.jsonl"),
        &dataset.source_bytes,
    )?;
    write_new_bytes(
        &options.destination.join("outputs/baseline.jsonl"),
        &baseline.source_bytes,
    )?;
    write_new_bytes(
        &options.destination.join("outputs/candidate.jsonl"),
        &candidate.source_bytes,
    )?;
    write_new_bytes(
        &options.destination.join("schemas/output.schema.json"),
        &schema_bytes,
    )?;
    write_new(&options.destination.join("structtrace.yaml"), &yaml)?;
    write_new(
        &options.destination.join("README.md"),
        &format!(
            "# {project_name}\n\nRecorded StructTrace comparison generated from validated artifacts. Correctness semantics were selected explicitly; inspect `structtrace.yaml` before using the gate. Field coverage and import decisions are recorded in `ONBOARDING.md`.\n\n```bash\nstructtrace doctor --strict\nstructtrace run\nstructtrace report latest --open\n{}\nstructtrace replay latest\n```\n",
            if options.gate_mode == GateMode::Release {
                "structtrace release-check latest"
            } else {
                "structtrace gate latest"
            }
        ),
    )?;
    write_new(
        &options.destination.join("ONBOARDING.md"),
        &discovery_markdown,
    )?;
    write_new(&options.destination.join(".gitignore"), ".structtrace/\n")?;
    Config::load(&options.destination.join("structtrace.yaml"))?;
    options.destination.canonicalize().with_context(|| {
        format!(
            "could not resolve initialized project {}",
            options.destination.display()
        )
    })
}

fn discovery_markdown(
    pointers: &std::collections::BTreeSet<String>,
    dataset: &Dataset,
    baseline: &RecordedOutputs,
    candidate: &RecordedOutputs,
) -> String {
    fn parsed(rows: &RecordedOutputs) -> Vec<Option<serde_json::Value>> {
        rows.rows
            .iter()
            .map(|row| {
                row.parse_source()
                    .and_then(|source| structtrace_core::strict_json::value_from_str(&source).ok())
            })
            .collect()
    }
    fn count(values: &[Option<serde_json::Value>], pointer: &str) -> usize {
        values
            .iter()
            .filter(|value| {
                value
                    .as_ref()
                    .and_then(|value| value.pointer(pointer))
                    .is_some()
            })
            .count()
    }
    fn observed_types(groups: [&[Option<serde_json::Value>]; 3], pointer: &str) -> String {
        let mut types = std::collections::BTreeSet::new();
        for value in groups.into_iter().flatten().filter_map(Option::as_ref) {
            if let Some(value) = value.pointer(pointer) {
                types.insert(match value {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => "object",
                });
            }
        }
        if types.is_empty() {
            "unobserved".to_owned()
        } else {
            types.into_iter().collect::<Vec<_>>().join(", ")
        }
    }
    let expected = dataset
        .cases
        .iter()
        .map(|case| case.expected.clone())
        .collect::<Vec<_>>();
    let baseline = parsed(baseline);
    let candidate = parsed(candidate);
    let mut markdown = String::from(
        "# Imported field discovery\n\nThis report is generated from the caller schema, expected references, baseline outputs, and candidate outputs. Missing candidate fields remain visible. Coverage is descriptive; StructTrace never activates semantic correctness automatically.\n\n| Field | Types | Expected | Baseline | Candidate |\n|---|---|---:|---:|---:|\n",
    );
    for pointer in pointers {
        markdown.push_str(&format!(
            "| `{pointer}` | {} | {}/{} | {}/{} | {}/{} |\n",
            observed_types([&expected, &baseline, &candidate], pointer),
            count(&expected, pointer),
            expected.len(),
            count(&baseline, pointer),
            baseline.len(),
            count(&candidate, pointer),
            candidate.len()
        ));
    }
    markdown.push_str(
        "\n## Deterministic evaluator choices\n\nUse exact values, normalized text, canonical dates, exact integers, decimal equality/tolerance, keyed arrays, or explicitly enabled financial invariants. Review `structtrace.yaml`; these choices define the primary correctness outcome.\n",
    );
    markdown
}

fn selectable_pointer_union(
    schema: &serde_json::Value,
    dataset: &Dataset,
    baseline: &RecordedOutputs,
    candidate: &RecordedOutputs,
) -> anyhow::Result<std::collections::BTreeSet<String>> {
    fn visit(
        value: &serde_json::Value,
        path: &str,
        output: &mut std::collections::BTreeSet<String>,
    ) {
        match value {
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    visit(value, &format!("{path}/{key}"), output);
                }
            }
            serde_json::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    visit(value, &format!("{path}/{index}"), output);
                }
            }
            _ => {
                output.insert(path.to_owned());
            }
        }
    }
    let mut pointers = std::collections::BTreeSet::new();
    fn visit_schema(
        schema: &serde_json::Value,
        path: &str,
        output: &mut std::collections::BTreeSet<String>,
    ) {
        if let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            for (key, child) in properties {
                let key = key.replace('~', "~0").replace('/', "~1");
                let pointer = format!("{path}/{key}");
                output.insert(pointer.clone());
                visit_schema(child, &pointer, output);
            }
        }
    }
    visit_schema(schema, "", &mut pointers);
    for case in &dataset.cases {
        if let Some(expected) = &case.expected {
            visit(expected, "", &mut pointers);
        }
    }
    for outputs in [baseline, candidate] {
        for row in &outputs.rows {
            if let Some(source) = row.parse_source() {
                if let Ok(value) = structtrace_core::strict_json::value_from_str(&source) {
                    visit(&value, "", &mut pointers);
                }
            }
        }
    }
    Ok(pointers)
}

fn write_new_bytes(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    anyhow::ensure!(!path.exists(), "refusing to overwrite {}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents).with_context(|| format!("could not create {}", path.display()))
}

/// Materialize one complete integration template.
pub fn initialize(destination: &Path, template: InitTemplate) -> anyhow::Result<PathBuf> {
    let mut protected = vec![
        "structtrace.yaml",
        "schemas/output.schema.json",
        "data/golden.jsonl",
        "README.md",
        ".gitignore",
    ];
    match template {
        InitTemplate::Recorded => {
            protected.extend(["outputs/baseline.jsonl", "outputs/candidate.jsonl"]);
        }
        InitTemplate::Python => protected.push("variants/app.py"),
        InitTemplate::Command => protected.push("variants/adapter.py"),
        InitTemplate::OpenaiCompatible => protected.push("variants/README.md"),
    }
    let conflicts = protected
        .iter()
        .filter(|relative| destination.join(relative).exists())
        .copied()
        .collect::<Vec<_>>();
    anyhow::ensure!(
        conflicts.is_empty(),
        "refusing to overwrite existing StructTrace files: {}",
        conflicts.join(", ")
    );
    for directory in [
        "schemas",
        "data",
        "evaluators",
        "variants",
        "outputs",
        ".structtrace",
    ] {
        std::fs::create_dir_all(destination.join(directory))?;
    }
    structtrace_core::filesystem::make_private_directory(&destination.join(".structtrace"))?;
    let project_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("structured-output-project");
    write_new(
        &destination.join("structtrace.yaml"),
        &configuration(project_name, template),
    )?;
    write_new(&destination.join("schemas/output.schema.json"), SCHEMA)?;
    write_new(&destination.join("data/golden.jsonl"), DATASET)?;
    write_new(
        &destination.join("README.md"),
        &readme(project_name, template),
    )?;
    write_new(&destination.join(".gitignore"), ".structtrace/\n")?;
    match template {
        InitTemplate::Recorded => {
            write_new(&destination.join("outputs/baseline.jsonl"), BASELINE)?;
            write_new(&destination.join("outputs/candidate.jsonl"), CANDIDATE)?;
        }
        InitTemplate::Python => {
            write_new(&destination.join("variants/app.py"), PYTHON_VARIANTS)?;
        }
        InitTemplate::Command => {
            write_new(&destination.join("variants/adapter.py"), COMMAND_VARIANT)?;
        }
        InitTemplate::OpenaiCompatible => {
            write_new(&destination.join("variants/README.md"), OPENAI_NOTES)?;
        }
    }
    destination
        .canonicalize()
        .with_context(|| format!("could not resolve {}", destination.display()))
}

/// Materialize the production-shaped invoice extraction preset.
pub fn initialize_extraction(destination: &Path) -> anyhow::Result<PathBuf> {
    let project_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("invoice-extraction-project");
    let extraction_config = EXTRACTION_CONFIG.replacen(
        "name: invoice-extraction-migration",
        &format!("name: {project_name}"),
        1,
    );
    let extraction_readme = extraction_readme(project_name);
    let files = [
        ("structtrace.yaml", extraction_config.as_str()),
        ("schemas/output.schema.json", EXTRACTION_SCHEMA),
        ("data/golden.jsonl", EXTRACTION_DATASET),
        ("outputs/baseline.jsonl", EXTRACTION_BASELINE),
        ("outputs/candidate.jsonl", EXTRACTION_CANDIDATE),
        ("README.md", extraction_readme.as_str()),
        (".gitignore", ".structtrace/\n"),
    ];
    let conflicts = files
        .iter()
        .filter(|(relative, _)| destination.join(relative).exists())
        .map(|(relative, _)| *relative)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        conflicts.is_empty(),
        "refusing to overwrite existing StructTrace files: {}",
        conflicts.join(", ")
    );
    for (relative, contents) in files {
        let path = destination.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_new(&path, contents)?;
    }
    destination
        .canonicalize()
        .with_context(|| format!("could not resolve {}", destination.display()))
}

fn write_new(path: &Path, contents: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!path.exists(), "refusing to overwrite {}", path.display());
    std::fs::write(path, contents)
        .with_context(|| format!("could not create {}", path.display()))?;
    Ok(())
}

fn configuration(project_name: &str, template: InitTemplate) -> String {
    let project_name = serde_json::to_string(project_name).expect("project name is serializable");
    let python = if cfg!(windows) { "python" } else { "python3" };
    let variants = match template {
        InitTemplate::Recorded => r#"  baseline:
    kind: recorded
    path: outputs/baseline.jsonl
  candidate:
    kind: recorded
    path: outputs/candidate.jsonl"#
            .to_owned(),
        InitTemplate::Python => {
            format!(
                r#"  baseline:
    kind: python
    interpreter: {python}
    callable: variants.app:baseline
  candidate:
    kind: python
    interpreter: {python}
    callable: variants.app:candidate"#
            )
        }
        InitTemplate::Command => {
            format!(
                r#"  baseline:
    kind: command
    command:
      program: {python}
      args: [variants/adapter.py, --variant, baseline]
    process_mode: persistent
    timeout_ms: 60000
  candidate:
    kind: command
    command:
      program: {python}
      args: [variants/adapter.py, --variant, candidate]
    process_mode: persistent
    timeout_ms: 60000"#
            )
        }
        InitTemplate::OpenaiCompatible => r#"  baseline:
    kind: openai_compatible
    base_url: http://127.0.0.1:8000/v1
    model: baseline-model
    request:
      system: Return only the required structured object.
      user_template: "{{ input.text }}"
      temperature: 0
      max_output_tokens: 200
    structured_output:
      mode: json_schema
      schema: schemas/output.schema.json
    timeout_ms: 120000
    concurrency: 4
  candidate:
    kind: openai_compatible
    base_url: http://127.0.0.1:8000/v1
    model: candidate-model
    request:
      system: Return only the required structured object.
      user_template: "{{ input.text }}"
      temperature: 0
      max_output_tokens: 200
    structured_output:
      mode: json_schema
      schema: schemas/output.schema.json
    timeout_ms: 120000
    concurrency: 4"#
            .to_owned(),
    };
    format!(
        r#"version: 3

project:
  name: {project_name}
  description: Paired regression testing for a structured-output change

storage:
  root: .structtrace
  retain_raw_outputs: true
  retain_provider_responses: false

limits:
  max_config_bytes: 1048576
  max_dataset_bytes: 33554432
  max_recorded_output_bytes: 33554432
  max_schema_bytes: 16777216
  max_cases: 10000
  max_jsonl_line_bytes: 16777216
  max_output_bytes_per_case: 4194304
  max_stderr_bytes_per_process: 1048576
  max_report_raw_bytes_per_case: 262144

dataset:
  path: data/golden.jsonl
  format: jsonl

schema:
  path: schemas/output.schema.json

variants:
{variants}

evaluators:
  - id: exact_label
    kind: json_pointer_exact
    pointer: /label
    expected_pointer: /label

outcomes:
  semantic_correct:
    all_of: [exact_label]

analysis:
  primary_outcome: semantic_correct
  bootstrap:
    samples: 10000
    confidence: 0.95
    seed: 17

gate:
  # The generated two-case fixture is a demonstration, not release evidence.
  mode: regression
  min_cases: 100
  min_unique_cases: 100
  max_duplicate_case_rate: 0.01
  min_primary_fully_evaluated_rate: 0.99
  max_primary_component_error_rate: 0.01
  max_primary_component_not_applicable_rate: 0.0
  max_primary_component_unscored_rate: 0.0
  max_primary_regression_pp: 1.0
  max_valid_but_wrong_increase_pp: 0.5
  min_candidate_schema_validity: 1.0
"#
    )
}

fn readme(project_name: &str, template: InitTemplate) -> String {
    format!(
        "# {project_name}\n\nStructTrace paired regression project using the `{}` integration. The generated fixture contains two cases and is not release evidence.\n\n```bash\nstructtrace doctor --strict\nstructtrace run\nstructtrace report latest --open\nstructtrace gate latest\nstructtrace replay latest\n```\n",
        match template {
            InitTemplate::Recorded => "recorded-output",
            InitTemplate::Python => "Python-callable",
            InitTemplate::Command => "command",
            InitTemplate::OpenaiCompatible => "OpenAI-compatible",
        }
    )
}

fn extraction_readme(project_name: &str) -> String {
    format!(
        r#"# {project_name}

This initialized project is a 12-case invoice-extraction regression fixture. It uses the installed
`structtrace` binary and deterministic built-in evaluators.

```bash
structtrace doctor --strict
structtrace run
structtrace report latest --open
structtrace gate latest
structtrace replay latest
```

Expected fixture result:

- 12 matched cases
- baseline primary success: 9/12
- candidate primary success: 9/12
- six discordant cases
- baseline schema validity: 10/12
- candidate schema validity: 12/12
- gate: `INSUFFICIENT EVIDENCE`, because 12 cases do not meet the configured 100-case evidence floor

The fixture demonstrates diagnosis and replay. It does not authorize deployment.
"#
    )
}

const SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["label", "reason"],
  "properties": {
    "label": {"type": "string", "enum": ["accepted", "rejected"]},
    "reason": {"type": "string", "minLength": 1}
  },
  "additionalProperties": false
}
"#;

const DATASET: &str = r#"{"id":"case-001","input":{"text":"A clear positive example."},"expected":{"label":"accepted"},"metadata":{"split":"golden"}}
{"id":"case-002","input":{"text":"A clear negative example."},"expected":{"label":"rejected"},"metadata":{"split":"golden"}}
"#;

const BASELINE: &str = r#"{"case_id":"case-001","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Matched the positive rule.\"}"}
{"case_id":"case-002","status":"ok","raw_output":"{\"label\":\"rejected\",\"reason\":\"Matched the negative rule.\"}"}
"#;

const CANDIDATE: &str = r#"{"case_id":"case-001","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Matched the positive rule.\"}"}
{"case_id":"case-002","status":"ok","raw_output":"{\"label\":\"accepted\",\"reason\":\"Candidate regression.\"}"}
"#;

const PYTHON_VARIANTS: &str = r#"def baseline(case: dict) -> dict:
    text = case["input"]["text"]
    label = "rejected" if "negative" in text else "accepted"
    return {"label": label, "reason": "Baseline deterministic example."}


def candidate(case: dict) -> dict:
    return {"label": "accepted", "reason": "Candidate deterministic example."}
"#;

const COMMAND_VARIANT: &str = r#"import argparse
import json
import sys

parser = argparse.ArgumentParser()
parser.add_argument("--variant", choices=("baseline", "candidate"), required=True)
args = parser.parse_args()

for line in sys.stdin:
    request = json.loads(line)
    text = request["input"]["text"]
    if args.variant == "baseline":
        label = "rejected" if "negative" in text else "accepted"
    else:
        label = "accepted"
    response = {
        "protocol": "structtrace.variant",
        "protocol_version": 3,
        "case_id": request["case_id"],
        "status": "ok",
        "output": {"label": label, "reason": f"{args.variant} deterministic example."},
    }
    print(json.dumps(response), flush=True)
"#;

const OPENAI_NOTES: &str = r#"# OpenAI-compatible example

Edit the localhost endpoint and model names in `structtrace.yaml`. The generated template is
unauthenticated; add `api_key_env` only when your endpoint requires it. StructTrace sends requests
only when `structtrace run` is explicitly invoked. `structtrace doctor` does not call the endpoint.
"#;

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn recorded_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("project");
        initialize(&project, InitTemplate::Recorded).unwrap();
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert!(!run.summary.gate.status.is_passed());
    }

    #[test]
    fn extraction_preset_runs_with_production_evaluators() {
        let root = tempdir().unwrap();
        let project = root.path().join("invoice-project");
        initialize_extraction(&project).unwrap();
        let config =
            structtrace_core::config::Config::load(&project.join("structtrace.yaml")).unwrap();
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::CanonicalDate { .. }
        )));
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::FinancialInvariants { .. }
        )));
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.total, 12);
        assert_eq!(run.summary.field_hotspots[0].pointer, "/total");
    }

    #[test]
    fn init_from_outputs_generates_valid_config() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        let project = root.path().join("guided");
        initialize(&source, InitTemplate::Recorded).unwrap();
        initialize_from_outputs(FromOutputsOptions {
            destination: &project,
            dataset: &source.join("data/golden.jsonl"),
            baseline: &source.join("outputs/baseline.jsonl"),
            candidate: &source.join("outputs/candidate.jsonl"),
            schema: &source.join("schemas/output.schema.json"),
            dataset_fields: DatasetFields::default(),
            output_fields: SimpleOutputFields {
                id: "/id".to_owned(),
                output: "/output".to_owned(),
            },
            correctness_pointers: &["/label".to_owned()],
            field_evaluators: &[],
            keyed_arrays: &[],
            financial_invariants: false,
            exact_json: false,
            gate_mode: GateMode::Regression,
            min_cases: 100,
        })
        .unwrap();
        Config::load(&project.join("structtrace.yaml")).unwrap();
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert_eq!(
            run.summary.gate.status,
            structtrace_core::gate::GateStatus::InsufficientEvidence
        );
    }

    #[test]
    fn candidate_missing_field_remains_selectable() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("data.jsonl"),
            "{\"id\":\"one\",\"input\":{},\"expected\":{\"tax\":\"10.00\"}}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("baseline.jsonl"),
            "{\"case_id\":\"one\",\"status\":\"ok\",\"raw_output\":\"{\\\"tax\\\":\\\"10.00\\\"}\"}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("candidate.jsonl"),
            "{\"case_id\":\"one\",\"status\":\"ok\",\"raw_output\":\"{}\"}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("schema.json"),
            r#"{"type":"object","properties":{"tax":{"type":"string"}}}"#,
        )
        .unwrap();
        initialize_from_outputs(FromOutputsOptions {
            destination: &root.path().join("guided"),
            dataset: &source.join("data.jsonl"),
            baseline: &source.join("baseline.jsonl"),
            candidate: &source.join("candidate.jsonl"),
            schema: &source.join("schema.json"),
            dataset_fields: DatasetFields::default(),
            output_fields: SimpleOutputFields {
                id: "/id".to_owned(),
                output: "/output".to_owned(),
            },
            correctness_pointers: &["/tax".to_owned()],
            field_evaluators: &[],
            keyed_arrays: &[],
            financial_invariants: false,
            exact_json: false,
            gate_mode: GateMode::Regression,
            min_cases: 100,
        })
        .unwrap();
        let onboarding = std::fs::read_to_string(root.path().join("guided/ONBOARDING.md")).unwrap();
        assert!(onboarding.contains("`/tax`"));
        assert!(onboarding.contains("0/1"));
    }

    #[test]
    fn arbitrary_dataset_and_simple_output_mappings_run_end_to_end() {
        let root = tempdir().unwrap();
        let source = root.path().join("ordinary-export");
        let project = root.path().join("imported-project");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("data.jsonl"),
            "{\"document_id\":\"one\",\"payload\":{\"text\":\"hello\"},\"ground_truth\":{\"vendor\":\"Acme\",\"date\":\"2026-08-10\",\"total\":\"10.00\"}}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("baseline.jsonl"),
            "{\"record_id\":\"one\",\"result\":{\"vendor\":\" ACME \",\"date\":\"10/08/2026\",\"total\":\"10.00\"},\"latency_ms\":12}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("candidate.jsonl"),
            "{\"record_id\":\"one\",\"result\":{\"vendor\":\"Acme\",\"date\":\"2026-08-10\",\"total\":\"10.00\"}}\n",
        )
        .unwrap();
        std::fs::write(
            source.join("schema.json"),
            r#"{"type":"object","required":["vendor","date","total"],"properties":{"vendor":{"type":"string"},"date":{"type":"string"},"total":{"type":"string"}}}"#,
        )
        .unwrap();
        initialize_from_outputs(FromOutputsOptions {
            destination: &project,
            dataset: &source.join("data.jsonl"),
            baseline: &source.join("baseline.jsonl"),
            candidate: &source.join("candidate.jsonl"),
            schema: &source.join("schema.json"),
            dataset_fields: DatasetFields {
                id: "/document_id".to_owned(),
                input: "/payload".to_owned(),
                expected: "/ground_truth".to_owned(),
                model_visible_metadata: "/model_visible_metadata".to_owned(),
                metadata: "/metadata".to_owned(),
            },
            output_fields: SimpleOutputFields {
                id: "/record_id".to_owned(),
                output: "/result".to_owned(),
            },
            correctness_pointers: &[],
            field_evaluators: &[
                "/vendor=normalized_string".to_owned(),
                "/date=canonical_date:iso,dmy_slash".to_owned(),
                "/total=decimal_exact".to_owned(),
            ],
            keyed_arrays: &[],
            financial_invariants: false,
            exact_json: false,
            gate_mode: GateMode::Regression,
            min_cases: 100,
        })
        .unwrap();
        let config = Config::load(&project.join("structtrace.yaml")).unwrap();
        assert_eq!(config.dataset.fields.id, "/document_id");
        assert_eq!(config.evaluators.len(), 3);
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 1);
        assert_eq!(run.summary.candidate.primary_pass, 1);
    }

    #[test]
    fn keyed_array_and_financial_evaluators_can_be_generated() {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        let project = root.path().join("guided");
        initialize_extraction(&source).unwrap();
        initialize_from_outputs(FromOutputsOptions {
            destination: &project,
            dataset: &source.join("data/golden.jsonl"),
            baseline: &source.join("outputs/baseline.jsonl"),
            candidate: &source.join("outputs/candidate.jsonl"),
            schema: &source.join("schemas/output.schema.json"),
            dataset_fields: DatasetFields::default(),
            output_fields: SimpleOutputFields {
                id: "/id".to_owned(),
                output: "/output".to_owned(),
            },
            correctness_pointers: &[],
            field_evaluators: &[],
            keyed_arrays: &["/line_items=/sku;/description:normalized_string,/quantity:exact_integer,/unit_price:decimal_exact,/amount:decimal_tolerance:0.01".to_owned()],
            financial_invariants: true,
            exact_json: false,
            gate_mode: GateMode::Regression,
            min_cases: 100,
        })
        .unwrap();
        let config = Config::load(&project.join("structtrace.yaml")).unwrap();
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::KeyedArray { .. }
        )));
        let keyed = config
            .evaluators
            .iter()
            .find_map(|evaluator| match &evaluator.kind {
                structtrace_core::config::EvaluatorKind::KeyedArray { fields, .. } => Some(fields),
                _ => None,
            })
            .unwrap();
        assert!(
            keyed.iter().any(|field| {
                field.pointer == "/unit_price" && field.evaluator == "decimal_exact"
            })
        );
        assert!(config.evaluators.iter().any(|evaluator| matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::FinancialInvariants { .. }
        )));
        let run =
            structtrace_engine::run_recorded(&project, Path::new("structtrace.yaml")).unwrap();
        assert_eq!(run.summary.baseline.total, 12);
        assert_eq!(run.summary.candidate.total, 12);
    }

    #[test]
    fn refuses_to_overwrite_an_existing_project() {
        let root = tempdir().unwrap();
        let project = root.path().join("project");
        initialize(&project, InitTemplate::Recorded).unwrap();
        assert!(initialize(&project, InitTemplate::Recorded).is_err());
    }

    #[tokio::test]
    async fn python_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("python-project");
        initialize(&project, InitTemplate::Python).unwrap();
        let run = structtrace_engine::run_configured(&project, Path::new("structtrace.yaml"))
            .await
            .unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert!(run.run_dir.join("report/index.html").is_file());
    }

    #[tokio::test]
    async fn command_template_runs_end_to_end() {
        let root = tempdir().unwrap();
        let project = root.path().join("command-project");
        initialize(&project, InitTemplate::Command).unwrap();
        let run = structtrace_engine::run_configured(&project, Path::new("structtrace.yaml"))
            .await
            .unwrap();
        assert_eq!(run.summary.baseline.primary_pass, 2);
        assert_eq!(run.summary.candidate.primary_pass, 1);
        assert_eq!(run.summary.baseline.operational.latency_observations, 2);
    }
}
