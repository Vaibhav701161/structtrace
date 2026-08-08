//! StructTrace command-line entry point.

#![forbid(unsafe_code)]

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use structtrace_core::{
    CoreError,
    artifact::{RunManifest, RunStatus, RunSummary},
    config::{Config, VariantConfig},
    dataset::Dataset,
    evaluation::compile_schema,
};

mod bundled_demo;
mod initialize;

const EXIT_INVALID_INPUT: u8 = 2;
const EXIT_RUN_FAILED: u8 = 3;
const EXIT_ARTIFACT_FAILURE: u8 = 4;
const EXIT_PROTOCOL_FAILURE: u8 = 5;
const EXIT_GATE_FAILED: u8 = 10;

#[derive(Debug, Parser)]
#[command(
    name = "structtrace",
    version,
    about = "Paired regression testing for structured LLM outputs",
    long_about = "Your schema passed. Did the answer?\n\nStructTrace compares matched baseline and candidate outputs without confusing structural validity with semantic correctness."
)]
struct Cli {
    /// Project configuration path.
    #[arg(long, global = true, default_value = "structtrace.yaml")]
    config: PathBuf,
    /// Project root used to resolve relative paths.
    #[arg(long, global = true, default_value = ".")]
    project_root: PathBuf,
    /// Human, JSON, or GitHub Actions output.
    #[arg(long, global = true, value_enum, default_value = "human")]
    format: OutputFormat,
    /// Suppress nonessential output.
    #[arg(long, global = true)]
    quiet: bool,
    /// Emit detailed diagnostics without secrets.
    #[arg(long, global = true)]
    verbose: bool,
    /// Disable ANSI colors.
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
    Github,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create a complete StructTrace project without overwriting existing files.
    Init {
        /// New project directory. Defaults to the current project root.
        path: Option<PathBuf>,
        /// Integration template.
        #[arg(long, value_enum, default_value = "recorded")]
        template: InitTemplate,
    },
    /// Run a complete deterministic demo without network access or credentials.
    Demo {
        /// Bundled scenario.
        #[arg(value_enum, default_value = "support-ticket")]
        demo: DemoKind,
        /// Open the generated report through a loopback-only server.
        #[arg(long)]
        open: bool,
    },
    /// Run the comparison defined by structtrace.yaml.
    Run {
        /// Optional configuration path overriding the global default.
        config_file: Option<PathBuf>,
        /// Open the generated report through a loopback-only server.
        #[arg(long)]
        open: bool,
        /// Resume a hash-compatible interrupted run without repeating completed variants.
        #[arg(long, value_name = "RUN_ID")]
        resume: Option<String>,
    },
    /// Compare existing baseline and candidate JSONL outputs.
    Compare {
        /// Matched golden dataset.
        #[arg(long)]
        dataset: PathBuf,
        /// Baseline recorded-output JSONL.
        #[arg(long)]
        baseline: PathBuf,
        /// Candidate recorded-output JSONL.
        #[arg(long)]
        candidate: PathBuf,
        /// External JSON Schema.
        #[arg(long)]
        schema: PathBuf,
        /// Open the generated report through a loopback-only server.
        #[arg(long)]
        open: bool,
    },
    /// Generate, export, or serve a completed local report.
    Report {
        /// Run ULID or `latest`.
        #[arg(default_value = "latest")]
        run: String,
        /// Open the loopback report URL in the default browser.
        #[arg(long)]
        open: bool,
        /// Serve until interrupted without opening a browser.
        #[arg(long)]
        serve: bool,
        /// Export one self-contained HTML file.
        #[arg(long)]
        export: Option<PathBuf>,
    },
    /// Apply configured deployment thresholds to a completed run.
    Gate {
        /// Run ULID or `latest`.
        #[arg(default_value = "latest")]
        run: String,
    },
    /// Recompute retained scores, summaries, intervals, and gate results.
    Replay {
        /// Run ULID or `latest`.
        #[arg(default_value = "latest")]
        run: String,
        /// Generate and verify the bundled accepted-research fixture.
        #[arg(long)]
        accepted_research: bool,
    },
    /// Report schema facts and potential sensitivity boundaries without rewriting it.
    Inspect {
        /// JSON Schema file.
        schema: PathBuf,
    },
    /// Validate the local environment without making network requests.
    Doctor,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DemoKind {
    SupportTicket,
    Research,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InitTemplate {
    Recorded,
    Python,
    Command,
    OpenaiCompatible,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let _color_enabled = !cli.no_color && std::env::var_os("NO_COLOR").is_none();
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("structtrace=debug")
            .with_writer(std::io::stderr)
            .init();
    }
    let result = dispatch(&cli).await;
    match result {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            if !cli.quiet {
                match cli.format {
                    OutputFormat::Json => println!(
                        "{}",
                        serde_json::json!({"status": "error", "message": format!("{error:#}")})
                    ),
                    OutputFormat::Github => {
                        println!(
                            "::error title=StructTrace failed::{}",
                            github_escape(&format!("{error:#}"))
                        );
                    }
                    OutputFormat::Human => eprintln!("StructTrace failed: {error:#}"),
                }
            }
            std::process::ExitCode::from(exit_code_for_error(&error))
        }
    }
}

async fn dispatch(cli: &Cli) -> anyhow::Result<u8> {
    match &cli.command {
        Commands::Init { path, template } => {
            let destination = path.as_ref().map_or_else(
                || cli.project_root.clone(),
                |path| {
                    if path.is_absolute() {
                        path.clone()
                    } else {
                        cli.project_root.join(path)
                    }
                },
            );
            let created = initialize::initialize(&destination, *template)?;
            if !cli.quiet {
                match cli.format {
                    OutputFormat::Json => println!(
                        "{}",
                        serde_json::json!({"status": "initialized", "project_root": created})
                    ),
                    OutputFormat::Github => {
                        println!("StructTrace project initialized at `{}`", created.display())
                    }
                    OutputFormat::Human => {
                        println!("STRUCTTRACE PROJECT INITIALIZED");
                        println!();
                        println!("Project: {}", created.display());
                        println!("Next:    cd {}", created.display());
                        println!("         structtrace doctor");
                        println!("         structtrace run");
                    }
                }
            }
            Ok(0)
        }
        Commands::Demo { demo, open } => {
            let run = match demo {
                DemoKind::SupportTicket => bundled_demo::run_support_ticket(&cli.project_root)?,
                DemoKind::Research => bundled_demo::run_research(&cli.project_root)?,
            };
            print_completed(cli, &run)?;
            if *open {
                structtrace_report::serve(&run.run_dir, true).await?;
            }
            Ok(0)
        }
        Commands::Run {
            config_file,
            open,
            resume,
        } => {
            let config_path = config_file.as_ref().unwrap_or(&cli.config);
            let run = if let Some(run_id) = resume {
                structtrace_engine::resume_configured(&cli.project_root, config_path, run_id)
                    .await?
            } else {
                structtrace_engine::run_configured(&cli.project_root, config_path).await?
            };
            print_completed(cli, &run)?;
            if *open {
                structtrace_report::serve(&run.run_dir, true).await?;
            }
            Ok(0)
        }
        Commands::Compare {
            dataset,
            baseline,
            candidate,
            schema,
            open,
        } => {
            let project_root = cli.project_root.canonicalize().with_context(|| {
                format!("project root {} does not exist", cli.project_root.display())
            })?;
            let config_path = resolve(&project_root, &cli.config);
            let mut config = Config::load(&config_path)?;
            config.dataset.path = dataset.clone();
            config.schema.path = schema.clone();
            config.variants.insert(
                "baseline".to_owned(),
                VariantConfig::Recorded {
                    path: baseline.clone(),
                },
            );
            config.variants.insert(
                "candidate".to_owned(),
                VariantConfig::Recorded {
                    path: candidate.clone(),
                },
            );
            let run =
                structtrace_engine::run_recorded_with_config(&project_root, &config_path, config)?;
            print_completed(cli, &run)?;
            if *open {
                structtrace_report::serve(&run.run_dir, true).await?;
            }
            Ok(0)
        }
        Commands::Report {
            run,
            open,
            serve,
            export,
        } => {
            let run_dir = resolve_run(cli, run)?;
            ensure_complete(&run_dir)?;
            if let Some(destination) = export {
                structtrace_report::export_single_file(&run_dir, destination)?;
                if !cli.quiet {
                    println!("Exported {}", destination.display());
                }
            }
            if *open || *serve {
                structtrace_report::serve(&run_dir, *open).await?;
            } else if export.is_none() {
                let report = structtrace_report::generate(&run_dir)?;
                if !cli.quiet {
                    match cli.format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::json!({"report": report.index_path}))
                        }
                        _ => println!("Report: {}", report.index_path.display()),
                    }
                }
            }
            Ok(0)
        }
        Commands::Gate { run } => gate(cli, &resolve_run(cli, run)?),
        Commands::Replay {
            run,
            accepted_research,
        } => {
            let run_dir = if *accepted_research {
                bundled_demo::run_research(&cli.project_root)?.run_dir
            } else {
                resolve_run(cli, run)?
            };
            ensure_complete(&run_dir)?;
            let report = structtrace_engine::replay_run(&run_dir)?;
            if !cli.quiet {
                match cli.format {
                    OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&report)?)
                    }
                    OutputFormat::Github => {
                        println!("## StructTrace replay `{}`", report.run_id);
                        println!();
                        println!("- Cases replayed: {}", report.cases_replayed);
                        println!(
                            "- Artifact hash mismatches: {}",
                            report.artifact_hash_mismatches.len()
                        );
                        println!(
                            "- Row-score mismatches: {}",
                            report.row_score_mismatches.len()
                        );
                        println!("- Summary mismatches: {}", report.summary_mismatches.len());
                    }
                    OutputFormat::Human => {
                        println!("STRUCTTRACE REPLAY");
                        println!();
                        println!("Run:                          {}", report.run_id);
                        println!("Cases replayed:               {}", report.cases_replayed);
                        println!(
                            "Variant outputs replayed:     {}",
                            report.variant_outputs_replayed
                        );
                        println!(
                            "Evaluator results recomputed: {}",
                            report.evaluator_results_recomputed
                        );
                        println!();
                        println!(
                            "Artifact hash mismatches:     {}",
                            report.artifact_hash_mismatches.len()
                        );
                        println!(
                            "Row-score mismatches:         {}",
                            report.row_score_mismatches.len()
                        );
                        println!(
                            "Summary mismatches:           {}",
                            report.summary_mismatches.len()
                        );
                        println!();
                        println!(
                            "{}",
                            if report.verified {
                                "REPLAY VERIFIED"
                            } else {
                                "REPLAY FAILED"
                            }
                        );
                    }
                }
            }
            Ok(if report.verified {
                0
            } else {
                EXIT_ARTIFACT_FAILURE
            })
        }
        Commands::Inspect { schema } => inspect_schema(cli, schema),
        Commands::Doctor => doctor(cli),
    }
}

fn inspect_schema(cli: &Cli, schema_path: &Path) -> anyhow::Result<u8> {
    let root = cli
        .project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", cli.project_root.display()))?;
    let path = resolve(&root, schema_path);
    let schema: serde_json::Value = read_json(&path)?;
    let inspection = structtrace_core::inspection::inspect_schema(&schema);
    if !cli.quiet {
        match cli.format {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&inspection)?),
            OutputFormat::Github => {
                println!("## StructTrace schema inspection");
                println!();
                println!("- Fields: {}", inspection.fields.len());
                println!("- Warnings: {}", inspection.warnings.len());
            }
            OutputFormat::Human => {
                println!("STRUCTTRACE SCHEMA INSPECTION");
                println!();
                println!("Schema: {}", path.display());
                println!(
                    "Draft:  {}",
                    inspection.draft.as_deref().unwrap_or("not declared")
                );
                println!();
                println!("Fields");
                for field in &inspection.fields {
                    let required = if field.required {
                        "required"
                    } else {
                        "optional"
                    };
                    println!(
                        "  {}  {}  {}",
                        field.pointer,
                        field.types.join(" | "),
                        required
                    );
                    if let Some(pattern) = &field.pattern {
                        println!("    Pattern: {pattern}");
                    }
                    if !field.enum_values.is_empty() {
                        println!("    Enum values: {}", field.enum_values.len());
                    }
                }
                if inspection.warnings.is_empty() {
                    println!();
                    println!("Potential sensitivity boundaries: none detected by static checks");
                } else {
                    println!();
                    println!("Potential sensitivity boundaries");
                    for warning in &inspection.warnings {
                        println!("  {}", warning.pointer);
                        println!("    {}", warning.observation);
                        println!("    {}", warning.recommendation);
                    }
                }
            }
        }
    }
    Ok(0)
}

fn print_completed(cli: &Cli, run: &structtrace_engine::CompletedRun) -> anyhow::Result<()> {
    if cli.quiet {
        return Ok(());
    }
    match cli.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "complete",
                "run_id": run.run_id,
                "run_dir": run.run_dir,
                "summary": run.summary,
            }))?
        ),
        OutputFormat::Github => {
            println!("## StructTrace run `{}`", run.run_id);
            println!();
            println!("| Metric | Baseline | Candidate |");
            println!("|---|---:|---:|");
            println!(
                "| Primary outcome | {}/{} | {}/{} |",
                run.summary.baseline.primary_pass,
                run.summary.baseline.total,
                run.summary.candidate.primary_pass,
                run.summary.candidate.total
            );
            println!(
                "| Schema valid | {}/{} | {}/{} |",
                run.summary.baseline.schema_valid,
                run.summary.baseline.total,
                run.summary.candidate.schema_valid,
                run.summary.candidate.total
            );
            println!();
            println!("Report: `{}/report/index.html`", run.run_dir.display());
        }
        OutputFormat::Human => {
            println!("STRUCTTRACE RUN COMPLETE");
            println!();
            println!("Run:          {}", run.run_id);
            println!(
                "Baseline:     {}/{} ({:.1}%)",
                run.summary.baseline.primary_pass,
                run.summary.baseline.total,
                percent(
                    run.summary.baseline.primary_pass,
                    run.summary.baseline.total
                )
            );
            println!(
                "Candidate:    {}/{} ({:.1}%)",
                run.summary.candidate.primary_pass,
                run.summary.candidate.total,
                percent(
                    run.summary.candidate.primary_pass,
                    run.summary.candidate.total
                )
            );
            println!(
                "Difference:   {:+.2} percentage points",
                run.summary.paired.difference_pp
            );
            println!(
                "Transitions:  {} candidate-only, {} baseline-only",
                run.summary.paired.candidate_only_pass, run.summary.paired.baseline_only_pass
            );
            println!(
                "Gate:         {}",
                if run.summary.gate.passed {
                    "PASSED"
                } else {
                    "FAILED"
                }
            );
            println!("Report:       {}/report/index.html", run.run_dir.display());
        }
    }
    Ok(())
}

fn gate(cli: &Cli, run_dir: &Path) -> anyhow::Result<u8> {
    ensure_complete(run_dir)?;
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    if !cli.quiet {
        match cli.format {
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "run_id": summary.run_id,
                    "passed": summary.gate.passed,
                    "exit_code": if summary.gate.passed { 0 } else { EXIT_GATE_FAILED },
                    "primary": summary.paired,
                    "rules": summary.gate.rules,
                }))?
            ),
            OutputFormat::Github => print_github_gate(&summary)?,
            OutputFormat::Human => print_human_gate(&summary, run_dir),
        }
    }
    Ok(if summary.gate.passed {
        0
    } else {
        EXIT_GATE_FAILED
    })
}

fn print_human_gate(summary: &RunSummary, run_dir: &Path) {
    println!(
        "STRUCTTRACE RELEASE GATE: {}",
        if summary.gate.passed {
            "PASSED"
        } else {
            "FAILED"
        }
    );
    println!();
    println!("Primary outcome");
    println!(
        "  Baseline:   {:.1}%",
        percent(summary.baseline.primary_pass, summary.baseline.total)
    );
    println!(
        "  Candidate:  {:.1}%",
        percent(summary.candidate.primary_pass, summary.candidate.total)
    );
    println!(
        "  Difference: {:+.2} percentage points",
        summary.paired.difference_pp
    );
    println!();
    println!("Paired transitions");
    println!(
        "  Candidate-only wins: {}",
        summary.paired.candidate_only_pass
    );
    println!(
        "  Baseline-only wins:  {}",
        summary.paired.baseline_only_pass
    );
    println!();
    println!("Rules");
    for rule in &summary.gate.rules {
        let state = match rule.passed {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "NOT EVALUATED",
        };
        println!("  {state:<13} {}", rule.message);
    }
    println!();
    println!("Report");
    println!("  {}/report/index.html", run_dir.display());
}

fn print_github_gate(summary: &RunSummary) -> anyhow::Result<()> {
    let mut markdown = format!(
        "## StructTrace release gate: {}",
        if summary.gate.passed {
            "passed"
        } else {
            "failed"
        }
    );
    markdown.push_str("\n\n| Metric | Baseline | Candidate |\n|---|---:|---:|\n");
    markdown.push_str(&format!(
        "| Primary outcome | {:.1}% | {:.1}% |\n",
        percent(summary.baseline.primary_pass, summary.baseline.total),
        percent(summary.candidate.primary_pass, summary.candidate.total)
    ));
    markdown.push_str(&format!(
        "| Schema validity | {:.1}% | {:.1}% |\n",
        percent(summary.baseline.schema_valid, summary.baseline.total),
        percent(summary.candidate.schema_valid, summary.candidate.total)
    ));
    markdown.push_str(&format!(
        "| Valid but wrong | {:.1}% | {:.1}% |\n",
        percent(summary.baseline.valid_but_wrong, summary.baseline.total),
        percent(summary.candidate.valid_but_wrong, summary.candidate.total)
    ));
    println!("{markdown}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "could not append GitHub step summary {}",
                    PathBuf::from(&path).display()
                )
            })?;
        writeln!(file, "{markdown}")?;
    }
    if !summary.gate.passed {
        for rule in summary
            .gate
            .rules
            .iter()
            .filter(|rule| rule.passed == Some(false))
        {
            println!(
                "::error title=StructTrace gate failed::{}",
                github_escape(&rule.message)
            );
        }
    }
    Ok(())
}

fn doctor(cli: &Cli) -> anyhow::Result<u8> {
    let root = cli
        .project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", cli.project_root.display()))?;
    let config_path = resolve(&root, &cli.config);
    let config_present = config_path.is_file();
    let mut checks = vec![serde_json::json!({
        "check": "project_root",
        "passed": true,
        "detail": root,
    })];
    let mut passed = true;
    if config_present {
        match Config::load(&config_path) {
            Ok(config) => {
                checks.push(serde_json::json!({"check": "configuration", "passed": true, "detail": config_path}));
                let dataset_path = resolve(&root, &config.dataset.path);
                match Dataset::read(&dataset_path, &config.dataset.fields) {
                    Ok(dataset) => checks.push(serde_json::json!({
                        "check": "dataset",
                        "passed": true,
                        "detail": {"path": dataset_path, "cases": dataset.cases.len(), "blake3": dataset.source_hash}
                    })),
                    Err(error) => {
                        passed = false;
                        checks.push(serde_json::json!({"check": "dataset", "passed": false, "detail": error.to_string()}));
                    }
                }
                let schema_path = resolve(&root, &config.schema.path);
                match std::fs::read(&schema_path)
                    .with_context(|| format!("could not read {}", schema_path.display()))
                    .and_then(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
                    .and_then(|schema| {
                        compile_schema(&schema)
                            .map(|_| schema)
                            .map_err(Into::into)
                    })
                {
                    Ok(schema) => checks.push(serde_json::json!({
                        "check": "schema",
                        "passed": true,
                        "detail": {"path": schema_path, "compiled": true, "draft": schema.get("$schema")}
                    })),
                    Err(error) => {
                        passed = false;
                        checks.push(serde_json::json!({"check": "schema", "passed": false, "detail": format!("{error:#}")}));
                    }
                }
                let storage_root = resolve(&root, &config.storage.root);
                match writable_directory(&storage_root) {
                    Ok(()) => checks.push(serde_json::json!({"check": "storage", "passed": true, "detail": storage_root})),
                    Err(error) => {
                        passed = false;
                        checks.push(serde_json::json!({"check": "storage", "passed": false, "detail": format!("{error:#}")}));
                    }
                }
                for (name, variant) in &config.variants {
                    match variant {
                        VariantConfig::Recorded { path } => {
                            let path = resolve(&root, path);
                            let exists = path.is_file();
                            passed &= exists;
                            checks.push(serde_json::json!({"check": format!("variant.{name}.recorded_output"), "passed": exists, "detail": path}));
                        }
                        VariantConfig::Command { command, .. } => {
                            let exists = executable_exists(&root, &command.program);
                            passed &= exists;
                            checks.push(serde_json::json!({"check": format!("variant.{name}.executable"), "passed": exists, "detail": command.program}));
                        }
                        VariantConfig::Python { interpreter, .. } => {
                            let exists = executable_exists(&root, interpreter);
                            passed &= exists;
                            checks.push(serde_json::json!({"check": format!("variant.{name}.python"), "passed": exists, "detail": interpreter}));
                        }
                        VariantConfig::OpenaiCompatible(adapter) => {
                            let present = std::env::var_os(&adapter.api_key_env).is_some();
                            passed &= present;
                            checks.push(serde_json::json!({
                                "check": format!("variant.{name}.credential"),
                                "passed": present,
                                "detail": {"environment_variable": adapter.api_key_env, "present": present, "network_checked": false},
                            }));
                            if let Some(path) = adapter
                                .structured_output
                                .as_ref()
                                .and_then(|structured| structured.schema.as_ref())
                            {
                                let path = resolve(&root, path);
                                let exists = path.is_file();
                                passed &= exists;
                                checks.push(serde_json::json!({"check": format!("variant.{name}.structured_schema"), "passed": exists, "detail": path}));
                            }
                        }
                    }
                }
                for evaluator in &config.evaluators {
                    let (program, kind) = match &evaluator.kind {
                        structtrace_core::config::EvaluatorKind::Command { command, .. } => {
                            (Some(command.program.as_str()), "executable")
                        }
                        structtrace_core::config::EvaluatorKind::Python { interpreter, .. } => {
                            (Some(interpreter.as_str()), "python")
                        }
                        _ => (None, "builtin"),
                    };
                    if let Some(program) = program {
                        let exists = executable_exists(&root, program);
                        passed &= exists;
                        checks.push(serde_json::json!({"check": format!("evaluator.{}.{}", evaluator.id, kind), "passed": exists, "detail": program}));
                    }
                }
                checks.push(serde_json::json!({
                    "check": "report_browser",
                    "passed": true,
                    "required": false,
                    "detail": "Browser launch is optional; reports can always be opened or exported by path."
                }));
            }
            Err(error) => {
                passed = false;
                checks.push(serde_json::json!({"check": "configuration", "passed": false, "detail": error.to_string()}));
            }
        }
    } else {
        checks.push(serde_json::json!({
            "check": "configuration",
            "passed": false,
            "required": false,
            "detail": format!("{} not found; run `structtrace init` before a project comparison", config_path.display()),
        }));
    }
    let payload = serde_json::json!({
        "product": "StructTrace",
        "version": env!("CARGO_PKG_VERSION"),
        "binary_target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "network_checked": false,
        "passed": passed,
        "checks": checks,
    });
    if !cli.quiet {
        match cli.format {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&payload)?),
            OutputFormat::Github => {
                println!("```json\n{}\n```", serde_json::to_string_pretty(&payload)?)
            }
            OutputFormat::Human => {
                println!("STRUCTTRACE DOCTOR");
                println!();
                println!("Binary:   structtrace {}", env!("CARGO_PKG_VERSION"));
                println!(
                    "Target:   {}-{}",
                    std::env::consts::ARCH,
                    std::env::consts::OS
                );
                println!("Network:  not checked");
                println!(
                    "Project:  {}",
                    if passed {
                        "ready"
                    } else if config_present {
                        "needs attention"
                    } else {
                        "not initialized"
                    }
                );
                println!();
                for check in &checks {
                    let state = if check["passed"].as_bool() == Some(true) {
                        "PASS"
                    } else if check["required"].as_bool() == Some(false) {
                        "INFO"
                    } else {
                        "FAIL"
                    };
                    println!(
                        "  {state:<4}  {}",
                        check["check"].as_str().unwrap_or("unknown")
                    );
                }
            }
        }
    }
    Ok(if passed || !config_present {
        0
    } else {
        EXIT_INVALID_INPUT
    })
}

fn writable_directory(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("could not create storage directory {}", path.display()))?;
    let marker = path.join(format!(
        ".doctor-write-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .with_context(|| format!("storage directory {} is not writable", path.display()))?;
    std::fs::remove_file(&marker)
        .with_context(|| format!("could not remove write check {}", marker.display()))?;
    Ok(())
}

fn executable_exists(project_root: &Path, program: &str) -> bool {
    let program_path = Path::new(program);
    if program_path.components().count() > 1 || program_path.is_absolute() {
        return resolve(project_root, program_path).is_file();
    }
    let extensions = if cfg!(windows) {
        std::env::var_os("PATHEXT")
            .map(|value| {
                value
                    .to_string_lossy()
                    .split(';')
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![".EXE".to_owned(), ".CMD".to_owned(), ".BAT".to_owned()])
    } else {
        vec![String::new()]
    };
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            extensions
                .iter()
                .any(|extension| directory.join(format!("{program}{extension}")).is_file())
        })
    })
}

fn resolve_run(cli: &Cli, selector: &str) -> anyhow::Result<PathBuf> {
    let root = cli
        .project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", cli.project_root.display()))?;
    let config_path = resolve(&root, &cli.config);
    let storage = if config_path.is_file() {
        resolve(&root, &Config::load(&config_path)?.storage.root)
    } else {
        root.join(".structtrace")
    };
    let runs = storage.join("runs");
    if selector == "latest" {
        let mut candidates = std::fs::read_dir(&runs)
            .with_context(|| format!("no StructTrace runs found under {}", runs.display()))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .collect::<Vec<_>>();
        candidates.sort_by_key(std::fs::DirEntry::file_name);
        return candidates
            .last()
            .map(std::fs::DirEntry::path)
            .context("no StructTrace run directories were found");
    }
    anyhow::ensure!(
        !selector.is_empty()
            && selector
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid run ID"
    );
    let path = runs.join(selector);
    anyhow::ensure!(path.is_dir(), "run `{selector}` does not exist");
    Ok(path)
}

fn ensure_complete(run_dir: &Path) -> anyhow::Result<()> {
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    anyhow::ensure!(
        manifest.status == RunStatus::Complete,
        "run `{}` is {:?}, not complete",
        manifest.run_id,
        manifest.status
    );
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn percent(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * count as f64 / total as f64
    }
}

fn github_escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn exit_code_for_error(error: &anyhow::Error) -> u8 {
    if let Some(core) = error.downcast_ref::<CoreError>() {
        match core {
            CoreError::Configuration(_)
            | CoreError::Dataset { .. }
            | CoreError::RecordedOutput { .. }
            | CoreError::Schema(_)
            | CoreError::Statistics(_) => EXIT_INVALID_INPUT,
            CoreError::Artifact(_) => EXIT_ARTIFACT_FAILURE,
            CoreError::Evaluator { .. } => EXIT_PROTOCOL_FAILURE,
            CoreError::Read { .. } | CoreError::Write { .. } => EXIT_RUN_FAILED,
        }
    } else {
        EXIT_RUN_FAILED
    }
}
