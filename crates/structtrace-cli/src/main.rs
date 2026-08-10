//! StructTrace command-line entry point.

#![forbid(unsafe_code)]

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use structtrace_adapters::{
    command::{CommandLimits, run_command},
    evaluator::{
        EVALUATOR_BRIDGE_SOURCE, EvaluatorInvocation, EvaluatorRuntime,
        run_external_evaluator_batch,
    },
    python::{BRIDGE_SOURCE, run_python},
};
use structtrace_core::{
    CoreError,
    artifact::{RunKind, RunManifest, RunStatus, RunSummary},
    config::{Config, VariantConfig},
    dataset::Dataset,
    evaluation::compile_schema,
    gate::{GateRuleStatus, GateStatus},
    hashing::hash_file,
};

mod bundled_demo;
mod initialize;

const EXIT_INVALID_INPUT: u8 = 2;
const EXIT_RUN_FAILED: u8 = 3;
const EXIT_ARTIFACT_FAILURE: u8 = 4;
const EXIT_PROTOCOL_FAILURE: u8 = 5;
const EXIT_GATE_FAILED: u8 = 10;
const EXIT_GATE_NOT_CONFIGURED: u8 = 11;
const EXIT_GATE_INSUFFICIENT_EVIDENCE: u8 = 12;
const EXIT_GATE_ERROR: u8 = 13;

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
    /// Override the configured StructTrace storage root.
    #[arg(long, global = true)]
    storage_root: Option<PathBuf>,
    /// Operate on one explicit run directory for report, gate, or replay commands.
    #[arg(long, global = true)]
    run_dir: Option<PathBuf>,
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
        #[arg(long, value_enum, conflicts_with_all = ["preset", "from_outputs"])]
        template: Option<InitTemplate>,
        /// Opinionated application preset.
        #[arg(long, value_enum, conflicts_with_all = ["template", "from_outputs"])]
        preset: Option<InitPreset>,
        /// Build a recorded comparison from existing matched artifacts.
        #[arg(long)]
        from_outputs: bool,
        /// Golden dataset used by --from-outputs.
        #[arg(long, requires = "from_outputs")]
        dataset: Option<PathBuf>,
        /// Baseline output JSONL used by --from-outputs.
        #[arg(long, requires = "from_outputs")]
        baseline: Option<PathBuf>,
        /// Candidate output JSONL used by --from-outputs.
        #[arg(long, requires = "from_outputs")]
        candidate: Option<PathBuf>,
        /// Caller-facing JSON Schema used by --from-outputs.
        #[arg(long, requires = "from_outputs")]
        schema: Option<PathBuf>,
        /// JSON Pointer whose exact value defines correctness. Repeat for multiple fields.
        #[arg(long, requires = "from_outputs")]
        correctness_pointer: Vec<String>,
        /// Compare the complete output object to expected instead of selected pointers.
        #[arg(
            long,
            requires = "from_outputs",
            conflicts_with = "correctness_pointer"
        )]
        exact_json: bool,
        /// Gate intent for the generated project.
        #[arg(
            long,
            value_enum,
            default_value = "regression",
            requires = "from_outputs"
        )]
        gate_mode: GuidedGateMode,
        /// Required independent cases for the generated evidence gate.
        #[arg(long, default_value_t = 100, requires = "from_outputs")]
        min_cases: usize,
    },
    /// Run a complete deterministic demo without network access or credentials.
    Demo {
        /// Bundled scenario.
        #[arg(value_enum, default_value = "invoice")]
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
    /// Compare recorded outputs using evaluators/outcomes from the project configuration.
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
        /// Run ULID, `latest`, `latest-any`, `latest-demo`, or `latest-research`.
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
        /// Export an aggregate-only share directory with all case content omitted.
        #[arg(long, value_name = "DIRECTORY")]
        export_share: Option<PathBuf>,
    },
    /// Apply configured deployment thresholds to a completed run.
    Gate {
        /// Run ULID, `latest`, `latest-any`, `latest-demo`, or `latest-research`.
        #[arg(default_value = "latest")]
        run: String,
        /// Integrity verification performed before applying the stored gate.
        #[arg(long, value_enum, default_value = "hash")]
        verify: GateVerification,
    },
    /// Recompute retained scores, summaries, intervals, and gate results.
    Replay {
        /// Run ULID, `latest`, `latest-any`, `latest-demo`, or `latest-research`.
        #[arg(default_value = "latest")]
        run: String,
        /// Verify normalized transition matrices, not original model artifacts.
        #[arg(long, alias = "accepted-research")]
        research_fixture: bool,
    },
    /// Report schema facts and potential sensitivity boundaries without rewriting it.
    Inspect {
        /// JSON Schema file.
        schema: PathBuf,
    },
    /// Validate the local environment without making network requests.
    Doctor {
        /// Fail on insecure storage, duplicate evidence, and leakage-risk values.
        #[arg(long)]
        strict: bool,
        /// Import configured Python workers and resolve callables without executing cases.
        #[arg(long, requires = "strict", conflicts_with = "execute_cases")]
        handshake: bool,
        /// Deliberately execute this many local business cases; configured code may have side effects.
        #[arg(
            long,
            value_name = "CASES",
            requires = "strict",
            conflicts_with = "handshake"
        )]
        execute_cases: Option<usize>,
    },
    /// Inspect, select, archive, or safely remove local runs.
    Runs {
        #[command(subcommand)]
        command: RunsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RunsCommand {
    /// List local runs without mixing demo and research fixtures into production selection.
    List,
    /// Show one run manifest after strict parsing.
    Show { run: String },
    /// Resolve the latest completed run of one kind.
    Latest {
        #[arg(long, value_enum, default_value = "production")]
        kind: RunKindArg,
    },
    /// Remove one inactive run beneath the configured storage root.
    Delete {
        run: String,
        /// Skip the interactive confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Copy one complete, hash-verified run into a self-verifying directory bundle.
    Archive { run: String, destination: PathBuf },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RunKindArg {
    Production,
    Demo,
    Research,
    Test,
}

impl RunKindArg {
    fn value(self) -> RunKind {
        match self {
            Self::Production => RunKind::Production,
            Self::Demo => RunKind::Demo,
            Self::Research => RunKind::ResearchFixture,
            Self::Test => RunKind::Test,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DemoKind {
    Invoice,
    SupportTicket,
    Research,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GateVerification {
    /// Verify the manifest-bound summary hash.
    Hash,
    /// Replay retained artifacts and require complete verification.
    Replay,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InitTemplate {
    Recorded,
    Python,
    Command,
    OpenaiCompatible,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InitPreset {
    Extraction,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum GuidedGateMode {
    Advisory,
    Regression,
    Release,
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
        Commands::Init {
            path,
            template,
            preset,
            from_outputs,
            dataset,
            baseline,
            candidate,
            schema,
            correctness_pointer,
            exact_json,
            gate_mode,
            min_cases,
        } => {
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
            let created = if *from_outputs {
                initialize::initialize_from_outputs(initialize::FromOutputsOptions {
                    destination: &destination,
                    dataset: &required_init_path(&cli.project_root, dataset, "--dataset")?,
                    baseline: &required_init_path(&cli.project_root, baseline, "--baseline")?,
                    candidate: &required_init_path(&cli.project_root, candidate, "--candidate")?,
                    schema: &required_init_path(&cli.project_root, schema, "--schema")?,
                    correctness_pointers: correctness_pointer,
                    exact_json: *exact_json,
                    gate_mode: match gate_mode {
                        GuidedGateMode::Advisory => structtrace_core::config::GateMode::Advisory,
                        GuidedGateMode::Regression => {
                            structtrace_core::config::GateMode::Regression
                        }
                        GuidedGateMode::Release => structtrace_core::config::GateMode::Release,
                    },
                    min_cases: *min_cases,
                })?
            } else {
                match preset {
                    Some(InitPreset::Extraction) => {
                        initialize::initialize_extraction(&destination)?
                    }
                    None => initialize::initialize(
                        &destination,
                        template.unwrap_or(InitTemplate::Recorded),
                    )?,
                }
            };
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
                        println!("         structtrace doctor --strict");
                        println!("         structtrace run");
                    }
                }
            }
            Ok(0)
        }
        Commands::Demo { demo, open } => {
            if matches!(demo, DemoKind::Research) {
                let research = bundled_demo::run_research(&cli.project_root)?;
                for run in &research.runs {
                    print_completed(cli, run)?;
                }
                if !cli.quiet {
                    println!(
                        "Research index (no pooled effect): {}",
                        research.index_path.display()
                    );
                }
                if *open {
                    let studies = research
                        .studies
                        .iter()
                        .zip(&research.runs)
                        .map(|((id, _), run)| (id.clone(), run.run_dir.clone()))
                        .collect::<Vec<_>>();
                    structtrace_report::serve_research(&research.index_path, &studies, true)
                        .await?;
                }
                return Ok(0);
            }
            let run = match demo {
                DemoKind::Invoice => bundled_demo::run_invoice(&cli.project_root)?,
                DemoKind::SupportTicket => bundled_demo::run_support_ticket(&cli.project_root)?,
                DemoKind::Research => unreachable!("handled above"),
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
            export_share,
        } => {
            let run_dir = resolve_run(cli, run)?;
            ensure_complete(&run_dir)?;
            if let Some(destination) = export {
                structtrace_report::export_single_file(&run_dir, destination)?;
                if !cli.quiet {
                    println!("Exported {}", destination.display());
                }
            }
            if let Some(destination) = export_share {
                structtrace_report::export_share_directory(&run_dir, destination)?;
                if !cli.quiet {
                    println!(
                        "Exported share-safe aggregate report {}",
                        destination.display()
                    );
                }
            }
            if *open || *serve {
                structtrace_report::serve(&run_dir, *open).await?;
            } else if export.is_none() && export_share.is_none() {
                let report = structtrace_report::finalized_report(&run_dir)?;
                if !cli.quiet {
                    match cli.format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::json!({"report": report.index_path}))
                        }
                        _ => {
                            println!("Report files: {}", report.index_path.display());
                            println!(
                                "Open safely with: structtrace --project-root {} report {} --open",
                                cli.project_root.display(),
                                run
                            );
                            println!(
                                "Do not open a chunked report through file://; browsers may block its case data."
                            );
                        }
                    }
                }
            }
            Ok(0)
        }
        Commands::Gate { run, verify } => gate(cli, &resolve_run(cli, run)?, *verify),
        Commands::Replay {
            run,
            research_fixture,
        } => {
            if *research_fixture {
                let research = bundled_demo::run_research(&cli.project_root)?;
                let reports = research
                    .runs
                    .iter()
                    .map(|run| structtrace_engine::replay_run(&run.run_dir))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                anyhow::ensure!(
                    reports.iter().all(|report| report.verified),
                    "one or more separate research fixtures failed replay"
                );
                if !cli.quiet {
                    println!("{}", serde_json::to_string_pretty(&reports)?);
                }
                return Ok(0);
            }
            let run_dir = resolve_run(cli, run)?;
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
                            "- Cross-artifact mismatches: {}",
                            report.cross_artifact_mismatches.len()
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
                            "Built-in evaluator results recomputed: {}",
                            report.built_in_evaluator_results_recomputed
                        );
                        println!(
                            "External evaluator receipts verified: {}",
                            report.external_evaluator_receipts_verified
                        );
                        println!(
                            "External evaluator programs re-executed: {}",
                            report.external_evaluator_programs_reexecuted
                        );
                        println!();
                        println!(
                            "Artifact hash mismatches:     {}",
                            report.artifact_hash_mismatches.len()
                        );
                        println!(
                            "Cross-artifact mismatches:  {}",
                            report.cross_artifact_mismatches.len()
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
        Commands::Doctor {
            strict,
            handshake,
            execute_cases,
        } => doctor(cli, *strict, *handshake, *execute_cases).await,
        Commands::Runs { command } => manage_runs(cli, command),
    }
}

fn manage_runs(cli: &Cli, command: &RunsCommand) -> anyhow::Result<u8> {
    match command {
        RunsCommand::List => {
            let manifests = local_run_manifests(cli)?;
            if !cli.quiet {
                match cli.format {
                    OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&manifests)?),
                    _ => {
                        println!(
                            "RUN ID                       KIND              STATUS       PROJECT"
                        );
                        for manifest in manifests {
                            println!(
                                "{:<28} {:<17} {:<12} {}",
                                manifest.run_id,
                                run_kind_label(manifest.run_kind),
                                format!("{:?}", manifest.status).to_ascii_lowercase(),
                                manifest.project_name
                            );
                        }
                    }
                }
            }
        }
        RunsCommand::Show { run } => {
            let run_dir = resolve_run(cli, run)?;
            let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
            if !cli.quiet {
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            }
        }
        RunsCommand::Latest { kind } => {
            let selector = match kind.value() {
                RunKind::Production => "latest",
                RunKind::Demo => "latest-demo",
                RunKind::ResearchFixture => "latest-research",
                RunKind::Test => "latest-test",
            };
            let run_dir = resolve_run(cli, selector)?;
            let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
            if !cli.quiet {
                match cli.format {
                    OutputFormat::Json => println!(
                        "{}",
                        serde_json::json!({"run_id": manifest.run_id, "run_dir": run_dir})
                    ),
                    _ => println!("{}\t{}", manifest.run_id, run_dir.display()),
                }
            }
        }
        RunsCommand::Delete { run, yes } => {
            let run_dir = resolve_run(cli, run)?;
            let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
            anyhow::ensure!(
                !matches!(
                    manifest.status,
                    RunStatus::Created
                        | RunStatus::Validating
                        | RunStatus::Running
                        | RunStatus::Analyzing
                ),
                "run `{}` is active ({:?}) and cannot be deleted",
                manifest.run_id,
                manifest.status
            );
            ensure_safe_run_tree(cli, &run_dir)?;
            if !yes {
                eprint!(
                    "Delete inactive run {} permanently? [y/N] ",
                    manifest.run_id
                );
                std::io::stderr().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                anyhow::ensure!(
                    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"),
                    "deletion cancelled"
                );
            }
            std::fs::remove_dir_all(&run_dir)?;
            if !cli.quiet {
                println!("Deleted inactive run {}", manifest.run_id);
            }
        }
        RunsCommand::Archive { run, destination } => {
            let run_dir = resolve_run(cli, run)?;
            ensure_complete(&run_dir)?;
            ensure_safe_run_tree(cli, &run_dir)?;
            verify_complete_manifest(&run_dir)?;
            let destination = if destination.is_absolute() {
                destination.clone()
            } else {
                cli.project_root.join(destination)
            };
            anyhow::ensure!(!destination.exists(), "archive destination already exists");
            std::fs::create_dir(&destination)?;
            let copied_root = destination.join("run");
            std::fs::create_dir(&copied_root)?;
            let mut hashes = std::collections::BTreeMap::new();
            copy_verified_tree(&run_dir, &run_dir, &copied_root, &mut hashes)?;
            let receipt = serde_json::json!({
                "format_version": 1,
                "run_id": run,
                "hash_algorithm": "blake3",
                "files": hashes,
            });
            std::fs::write(
                destination.join("archive-verification.json"),
                serde_json::to_vec_pretty(&receipt)?,
            )?;
            if !cli.quiet {
                println!("Archived verified run to {}", destination.display());
            }
        }
    }
    Ok(0)
}

fn local_run_manifests(cli: &Cli) -> anyhow::Result<Vec<RunManifest>> {
    let runs = storage_root(cli)?.join("runs");
    let mut manifests: Vec<RunManifest> = Vec::new();
    for entry in std::fs::read_dir(&runs)
        .with_context(|| format!("no StructTrace runs found under {}", runs.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let path = entry.path().join("manifest.json");
        if path.is_file() {
            manifests.push(read_json(&path)?);
        }
    }
    manifests.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    Ok(manifests)
}

fn run_kind_label(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Production => "production",
        RunKind::Demo => "demo",
        RunKind::ResearchFixture => "research",
        RunKind::Test => "test",
    }
}

fn ensure_safe_run_tree(cli: &Cli, run_dir: &Path) -> anyhow::Result<()> {
    let root = storage_root(cli)?.canonicalize()?;
    let runs = root.join("runs").canonicalize()?;
    let canonical = run_dir.canonicalize()?;
    anyhow::ensure!(
        canonical.parent() == Some(runs.as_path()),
        "run escaped storage root"
    );
    fn check(directory: &Path) -> anyhow::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            anyhow::ensure!(!file_type.is_symlink(), "run contains a symbolic link");
            if file_type.is_dir() {
                check(&entry.path())?;
            } else {
                anyhow::ensure!(file_type.is_file(), "run contains a non-regular entry");
            }
        }
        Ok(())
    }
    check(&canonical)
}

fn verify_complete_manifest(run_dir: &Path) -> anyhow::Result<()> {
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    for relative in manifest.artifacts.keys() {
        verify_manifest_artifact(run_dir, relative)?;
    }
    Ok(())
}

fn copy_verified_tree(
    source_root: &Path,
    source: &Path,
    destination_root: &Path,
    hashes: &mut std::collections::BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        anyhow::ensure!(
            !file_type.is_symlink(),
            "archive source contains a symbolic link"
        );
        let relative = entry.path().strip_prefix(source_root)?.to_owned();
        let destination = destination_root.join(&relative);
        if file_type.is_dir() {
            std::fs::create_dir(&destination)?;
            copy_verified_tree(source_root, &entry.path(), destination_root, hashes)?;
        } else {
            anyhow::ensure!(
                file_type.is_file(),
                "archive source contains a non-regular entry"
            );
            std::fs::copy(entry.path(), &destination)?;
            let relative = relative.to_string_lossy().replace('\\', "/");
            let source_hash = hash_file(&entry.path())?;
            anyhow::ensure!(
                source_hash == hash_file(&destination)?,
                "archive copy mismatch"
            );
            hashes.insert(relative, source_hash);
        }
    }
    Ok(())
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
            println!("Gate:         {}", run.summary.gate.status.label());
            println!("Report:       {}/report/index.html", run.run_dir.display());
            println!("Open with:    structtrace report {} --open", run.run_id);
            println!("              (chunked reports may not work through file://)");
        }
    }
    Ok(())
}

fn gate(cli: &Cli, run_dir: &Path, verify: GateVerification) -> anyhow::Result<u8> {
    ensure_complete(run_dir)?;
    verify_manifest_artifact(run_dir, "summary.json")?;
    if matches!(verify, GateVerification::Replay) {
        let replay = structtrace_engine::replay_run(run_dir)?;
        anyhow::ensure!(
            replay.verified,
            "replay verification failed: {} artifact hash, {} cross-artifact, {} row-score, and {} summary mismatch(es)",
            replay.artifact_hash_mismatches.len(),
            replay.cross_artifact_mismatches.len(),
            replay.row_score_mismatches.len(),
            replay.summary_mismatches.len()
        );
    }
    let summary: RunSummary = read_json(&run_dir.join("summary.json"))?;
    if !cli.quiet {
        match cli.format {
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "run_id": summary.run_id,
                    "status": summary.gate.status,
                    "deployment_authorized": summary.gate.deployment_authorized,
                    "quality_failures": summary.gate.quality_failures,
                    "evidence_failures": summary.gate.evidence_failures,
                    "runtime_errors": summary.gate.runtime_errors,
                    "exit_code": gate_exit_code(summary.gate.status),
                    "primary": summary.paired,
                    "rules": summary.gate.rules,
                }))?
            ),
            OutputFormat::Github => print_github_gate(&summary)?,
            OutputFormat::Human => print_human_gate(&summary, run_dir),
        }
    }
    Ok(gate_exit_code(summary.gate.status))
}

fn print_human_gate(summary: &RunSummary, run_dir: &Path) {
    println!("STRUCTTRACE RELEASE GATE: {}", gate_headline(summary));
    println!(
        "{}",
        if summary.gate.deployment_authorized {
            "DEPLOYMENT AUTHORIZED"
        } else {
            "DO NOT DEPLOY"
        }
    );
    if !summary.gate.quality_failures.is_empty() {
        println!("Quality threshold failed.");
    }
    if !summary.gate.evidence_failures.is_empty() {
        println!("Evidence requirements are also insufficient.");
    }
    if !summary.gate.runtime_errors.is_empty() {
        println!("One or more gate rules could not be evaluated safely.");
    }
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
    if summary.gate.rules.is_empty() {
        println!("  No release criteria were configured.");
        println!("  This run was analyzed, but StructTrace cannot make a deployment decision.");
    }
    for rule in &summary.gate.rules {
        let state = match rule.status {
            GateRuleStatus::Passed => "PASS",
            GateRuleStatus::Failed => "FAIL",
            GateRuleStatus::NotConfigured => "NOT CONFIGURED",
            GateRuleStatus::InsufficientEvidence => "INSUFFICIENT",
            GateRuleStatus::Error => "ERROR",
        };
        println!("  {state:<13} {}", rule.message);
    }
    println!();
    println!("Report");
    println!("  {}/report/index.html", run_dir.display());
}

fn print_github_gate(summary: &RunSummary) -> anyhow::Result<()> {
    let mut markdown = format!("## StructTrace release gate: {}", gate_headline(summary));
    if !summary.gate.quality_failures.is_empty() {
        markdown.push_str("\n\n**Quality thresholds failed.**");
    }
    if !summary.gate.evidence_failures.is_empty() {
        markdown.push_str("\n\n**Evidence requirements are also insufficient.**");
    }
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
    if !summary.gate.status.is_passed() {
        let annotation = match summary.gate.status {
            GateStatus::Failed => "StructTrace gate failed",
            GateStatus::NotConfigured => "StructTrace gate not configured",
            GateStatus::InsufficientEvidence => "StructTrace evidence insufficient",
            GateStatus::Error => "StructTrace gate error",
            GateStatus::Passed => "StructTrace gate passed",
        };
        for rule in summary.gate.rules.iter().filter(|rule| {
            matches!(
                rule.status,
                GateRuleStatus::Failed
                    | GateRuleStatus::InsufficientEvidence
                    | GateRuleStatus::Error
            )
        }) {
            println!(
                "::error title={}::{}",
                annotation,
                github_escape(&rule.message)
            );
        }
        if summary.gate.rules.is_empty() {
            println!(
                "::error title={}::{}",
                annotation,
                github_escape("no release criteria were configured")
            );
        }
    }
    Ok(())
}

fn gate_headline(summary: &RunSummary) -> String {
    if !summary.gate.quality_failures.is_empty() && !summary.gate.evidence_failures.is_empty() {
        "DO NOT DEPLOY: quality failed and evidence is insufficient".to_owned()
    } else {
        summary.gate.status.label().to_ascii_lowercase()
    }
}

const fn gate_exit_code(status: GateStatus) -> u8 {
    match status {
        GateStatus::Passed => 0,
        GateStatus::Failed => EXIT_GATE_FAILED,
        GateStatus::NotConfigured => EXIT_GATE_NOT_CONFIGURED,
        GateStatus::InsufficientEvidence => EXIT_GATE_INSUFFICIENT_EVIDENCE,
        GateStatus::Error => EXIT_GATE_ERROR,
    }
}

async fn doctor(
    cli: &Cli,
    strict: bool,
    handshake: bool,
    execute_cases: Option<usize>,
) -> anyhow::Result<u8> {
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
    let mut loaded_dataset = None;
    if config_present {
        match Config::load(&config_path) {
            Ok(config) => {
                checks.push(serde_json::json!({"check": "configuration", "passed": true, "detail": config_path}));
                let gate_configured = config.gate.is_configured();
                passed &= !strict || gate_configured;
                checks.push(serde_json::json!({
                    "check": "release_gate_configuration",
                    "passed": gate_configured,
                    "required": strict,
                    "detail": if gate_configured { "at least one release criterion is configured" } else { "no release criteria are configured; analysis cannot authorize deployment" }
                }));
                let dataset_path = resolve(&root, &config.dataset.path);
                match Dataset::read_bounded(&dataset_path, &config.dataset.fields, &config.limits) {
                    Ok(dataset) => {
                        loaded_dataset = Some(dataset.clone());
                        checks.push(serde_json::json!({
                            "check": "dataset",
                            "passed": true,
                            "detail": {"path": dataset_path, "cases": dataset.cases.len(), "blake3": dataset.source_hash}
                        }));
                        let mut fingerprints = std::collections::BTreeMap::<String, usize>::new();
                        let mut visible_gold_matches = 0usize;
                        let mut suspicious_case_ids = 0usize;
                        for case in &dataset.cases {
                            let semantic =
                                evidence_unit_value(case, &config.dataset.evidence_unit)?;
                            let fingerprint =
                                structtrace_core::hashing::hash_canonical_json(&semantic)?;
                            *fingerprints.entry(fingerprint).or_default() += 1;
                            if let Some(expected) = &case.expected {
                                let leaves = scalar_leaves(expected);
                                if leaves.iter().any(|leaf| {
                                    json_contains(&case.input, leaf)
                                        || case
                                            .model_visible_metadata
                                            .as_ref()
                                            .is_some_and(|metadata| json_contains(metadata, leaf))
                                }) {
                                    visible_gold_matches += 1;
                                }
                                if leaves
                                    .iter()
                                    .any(|leaf| case_id_contains_label(&case.id, leaf))
                                {
                                    suspicious_case_ids += 1;
                                }
                            }
                        }
                        let duplicate_rows = dataset.cases.len().saturating_sub(fingerprints.len());
                        let duplicate_passed = duplicate_rows == 0;
                        passed &= !strict || duplicate_passed;
                        checks.push(serde_json::json!({
                            "check": "independent_evidence",
                            "passed": duplicate_passed,
                            "required": strict,
                            "detail": {"total_rows": dataset.cases.len(), "unique_semantic_cases": fingerprints.len(), "duplicate_rows": duplicate_rows}
                        }));
                        let leakage_passed = visible_gold_matches == 0;
                        passed &= !strict || leakage_passed;
                        checks.push(serde_json::json!({
                            "check": "golden_value_isolation",
                            "passed": leakage_passed,
                            "required": strict,
                            "detail": {
                                "model_visible_expected_leaf_matches": visible_gold_matches,
                                "opaque_case_ids_with_label_like_text": suspicious_case_ids,
                                "case_id_note": "case IDs are never model-visible and do not create a strict leakage failure"
                            }
                        }));
                        let bootstrap_work = config
                            .analysis
                            .bootstrap
                            .samples
                            .checked_mul(fingerprints.len());
                        let bootstrap_safe = bootstrap_work.is_some_and(|work| {
                            work <= structtrace_core::config::HARD_MAX_BOOTSTRAP_WORK_UNITS
                        });
                        passed &= !strict || bootstrap_safe;
                        checks.push(serde_json::json!({
                            "check": "bootstrap_resource_budget",
                            "passed": bootstrap_safe,
                            "required": strict,
                            "detail": {
                                "samples": config.analysis.bootstrap.samples,
                                "evidence_units": fingerprints.len(),
                                "estimated_resampling_operations": bootstrap_work,
                                "hard_work_limit": structtrace_core::config::HARD_MAX_BOOTSTRAP_WORK_UNITS,
                                "estimated_result_bytes": config.analysis.bootstrap.samples.saturating_mul(std::mem::size_of::<f64>())
                            }
                        }));
                    }
                    Err(error) => {
                        passed = false;
                        checks.push(serde_json::json!({"check": "dataset", "passed": false, "detail": error.to_string()}));
                    }
                }
                let schema_path = resolve(&root, &config.schema.path);
                match structtrace_core::hashing::read_bounded(
                    &schema_path,
                    config.limits.max_schema_bytes,
                    "schema",
                )
                    .map_err(anyhow::Error::from)
                    .and_then(|bytes| {
                        structtrace_core::strict_json::value_from_slice(&bytes)
                            .map_err(anyhow::Error::from)
                    })
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
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = std::fs::metadata(&storage_root) {
                        let secure = metadata.permissions().mode() & 0o077 == 0;
                        passed &= !strict || secure;
                        checks.push(serde_json::json!({
                            "check": "storage_permissions",
                            "passed": secure,
                            "required": strict,
                            "detail": format!("mode {:o}; expected no group/other permissions", metadata.permissions().mode() & 0o777)
                        }));
                    }
                }
                for (name, variant) in &config.variants {
                    match variant {
                        VariantConfig::Recorded { path } => {
                            let path = resolve(&root, path);
                            let result = loaded_dataset.as_ref().map_or_else(
                                || Err(anyhow::anyhow!("dataset preflight failed")),
                                |dataset| {
                                    structtrace_core::output::RecordedOutputs::read_bounded(
                                        &path,
                                        dataset,
                                        &config.limits,
                                    )
                                    .map(|outputs| outputs.rows.len())
                                    .map_err(anyhow::Error::from)
                                },
                            );
                            let valid = result.is_ok();
                            passed &= valid;
                            checks.push(serde_json::json!({"check": format!("variant.{name}.recorded_output"), "passed": valid, "detail": {"path": path, "matched_rows": result.ok()}}));
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
                            let present = adapter
                                .api_key_env
                                .as_deref()
                                .is_none_or(|name| std::env::var_os(name).is_some());
                            passed &= present;
                            checks.push(serde_json::json!({
                                "check": format!("variant.{name}.credential"),
                                "passed": present,
                                "detail": {"environment_variable": adapter.api_key_env, "present": present, "authentication_required": adapter.api_key_env.is_some(), "network_checked": false},
                            }));
                            if let Some(path) = adapter
                                .structured_output
                                .as_ref()
                                .and_then(|structured| structured.schema.as_ref())
                            {
                                let path = resolve(&root, path);
                                let validation = std::fs::symlink_metadata(&path)
                                    .map_err(anyhow::Error::from)
                                    .and_then(|metadata| {
                                        anyhow::ensure!(
                                            metadata.is_file()
                                                && !metadata.file_type().is_symlink(),
                                            "schema must be a regular non-symlink file"
                                        );
                                        structtrace_core::hashing::read_bounded(
                                            &path,
                                            config.limits.max_schema_bytes,
                                            "model-facing schema",
                                        )
                                        .map_err(Into::into)
                                    })
                                    .and_then(|bytes| {
                                        structtrace_core::strict_json::value_from_slice(&bytes)
                                            .map_err(Into::into)
                                    })
                                    .and_then(|schema| {
                                        compile_schema(&schema).map(|_| ()).map_err(Into::into)
                                    });
                                let valid = validation.is_ok();
                                passed &= valid;
                                checks.push(serde_json::json!({"check": format!("variant.{name}.structured_schema"), "passed": valid, "detail": {"path": path, "compiled": valid}}));
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
                    "passed": null,
                    "required": false,
                    "status": "not_checked",
                    "detail": "Browser launch is optional; reports can always be opened or exported by path."
                }));
                if config.storage.process_logs.mode
                    == structtrace_core::config::ProcessLogMode::FullSensitive
                {
                    checks.push(serde_json::json!({
                        "check": "full_sensitive_process_logs",
                        "passed": false,
                        "required": false,
                        "status": "warning",
                        "detail": "Process logs may contain secrets because full_sensitive retention is explicitly enabled."
                    }));
                }
                if handshake {
                    let handshake_checks = local_worker_static_handshake(&root, &config)?;
                    for check in handshake_checks {
                        passed &= check["passed"].as_bool() == Some(true);
                        checks.push(check);
                    }
                }
                if let Some(case_count) = execute_cases {
                    anyhow::ensure!(case_count > 0, "--execute-cases requires at least one case");
                    checks.push(serde_json::json!({
                        "check": "execute_cases_side_effect_warning",
                        "passed": true,
                        "required": true,
                        "detail": "Explicit opt-in accepted: configured user code is being executed and may make network calls or cause side effects. OpenAI-compatible endpoints remain excluded."
                    }));
                    if let Some(dataset) = loaded_dataset.as_ref() {
                        let executed =
                            local_worker_handshake(&root, &config, dataset, case_count).await?;
                        for check in executed {
                            passed &= check["passed"].as_bool() == Some(true);
                            checks.push(check);
                        }
                    }
                } else if !handshake {
                    checks.push(serde_json::json!({
                        "check": "one_case_adapter_handshake",
                        "passed": null,
                        "required": false,
                        "status": "not_checked",
                        "detail": "Strict doctor is static only. Use --strict --handshake to import Python workers without cases, or --strict --execute-cases 1 to deliberately execute local user code. Network providers are never contacted by doctor."
                    }));
                }
            }
            Err(error) => {
                passed = false;
                checks.push(serde_json::json!({"check": "configuration", "passed": false, "detail": error.to_string()}));
            }
        }
    } else {
        passed &= !strict;
        checks.push(serde_json::json!({
            "check": "configuration",
            "passed": false,
            "required": strict,
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
                    let state = if check["status"].as_str() == Some("not_checked") {
                        "NOT CHECKED"
                    } else if check["passed"].as_bool() == Some(true) {
                        "PASS"
                    } else if check["required"].as_bool() == Some(false) {
                        "WARNING"
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
    Ok(if passed { 0 } else { EXIT_INVALID_INPUT })
}

async fn local_worker_handshake(
    root: &Path,
    config: &Config,
    dataset: &Dataset,
    requested_cases: usize,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let count = requested_cases.min(dataset.cases.len());
    let cases = dataset
        .cases
        .iter()
        .take(count)
        .map(structtrace_core::dataset::VariantCase::from)
        .collect::<Vec<_>>();
    let limits = CommandLimits {
        max_output_bytes: config.limits.max_output_bytes_per_case,
        max_stderr_bytes: config.limits.max_stderr_bytes_per_process,
    };
    let runtime_root = resolve(root, &config.storage.root).join("runtime");
    std::fs::create_dir_all(&runtime_root)?;
    let python_bridge = runtime_root.join("doctor-python-bridge-v3.py");
    let evaluator_bridge = runtime_root.join("doctor-evaluator-bridge-v3.py");
    std::fs::write(&python_bridge, BRIDGE_SOURCE)?;
    std::fs::write(&evaluator_bridge, EVALUATOR_BRIDGE_SOURCE)?;
    let mut checks = Vec::new();
    for (name, variant) in &config.variants {
        let run = match variant {
            VariantConfig::Command {
                command,
                process_mode,
                timeout_ms,
                ..
            } => {
                Some(run_command(command, *process_mode, *timeout_ms, &cases, root, &limits).await)
            }
            VariantConfig::Python {
                interpreter,
                callable,
                timeout_ms,
                ..
            } => Some(
                run_python(
                    interpreter,
                    callable,
                    *timeout_ms,
                    &cases,
                    root,
                    &python_bridge,
                    &limits,
                )
                .await,
            ),
            VariantConfig::Recorded { .. } | VariantConfig::OpenaiCompatible(_) => None,
        };
        if let Some(run) = run {
            let successful = run.protocol_errors.is_empty()
                && run.rows.len() == count
                && run
                    .rows
                    .iter()
                    .all(|row| row.status == structtrace_core::output::OutputStatus::Ok);
            checks.push(serde_json::json!({
                "check": format!("variant.{name}.local_handshake"),
                "passed": successful,
                "required": true,
                "detail": {"cases": count, "protocol_errors": run.protocol_errors.len(), "network_checked": false}
            }));
        }
    }

    let outputs = dataset
        .cases
        .iter()
        .take(count)
        .map(|case| structtrace_core::output::VariantOutput {
            case_id: case.id.clone(),
            status: structtrace_core::output::OutputStatus::Ok,
            raw_output: Some(
                serde_json::to_string(case.expected.as_ref().unwrap_or(&serde_json::Value::Null))
                    .unwrap_or_else(|_| "null".to_owned()),
            ),
            parsed_output: case.expected.clone(),
            error: None,
            latency_ms: None,
            usage: None,
            cost: None,
            metadata: serde_json::Value::Object(Default::default()),
            retries: Vec::new(),
        })
        .collect::<Vec<_>>();
    for evaluator in &config.evaluators {
        if !matches!(
            evaluator.kind,
            structtrace_core::config::EvaluatorKind::Command { .. }
                | structtrace_core::config::EvaluatorKind::Python { .. }
        ) {
            continue;
        }
        let invocations = dataset
            .cases
            .iter()
            .take(count)
            .zip(&outputs)
            .map(|(case, output)| EvaluatorInvocation { case, output })
            .collect::<Vec<_>>();
        let runs = run_external_evaluator_batch(
            &evaluator.id,
            &evaluator.kind,
            evaluator.implementation_version.as_deref(),
            &invocations,
            EvaluatorRuntime {
                variant_id: "doctor",
                working_directory: root,
                python_bridge: &evaluator_bridge,
                limits: &limits,
            },
        );
        let successful = runs.len() == count
            && runs.iter().all(|run| {
                run.result.status != structtrace_core::evaluation::EvaluationStatus::Error
            });
        checks.push(serde_json::json!({
            "check": format!("evaluator.{}.local_handshake", evaluator.id),
            "passed": successful,
            "required": true,
            "detail": {"cases": count, "network_checked": false}
        }));
    }
    if checks.is_empty() {
        checks.push(serde_json::json!({
            "check": "one_case_adapter_handshake",
            "passed": true,
            "required": true,
            "detail": "recorded inputs were fully parsed; no local workers are configured and network providers were not contacted"
        }));
    }
    Ok(checks)
}

fn local_worker_static_handshake(
    root: &Path,
    config: &Config,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let runtime_root = resolve(root, &config.storage.root).join("runtime");
    std::fs::create_dir_all(&runtime_root)?;
    let python_bridge = runtime_root.join("doctor-python-bridge-v3.py");
    let evaluator_bridge = runtime_root.join("doctor-evaluator-bridge-v3.py");
    std::fs::write(&python_bridge, BRIDGE_SOURCE)?;
    std::fs::write(&evaluator_bridge, EVALUATOR_BRIDGE_SOURCE)?;
    let mut checks = Vec::new();
    for (name, variant) in &config.variants {
        if let VariantConfig::Python {
            interpreter,
            callable,
            ..
        } = variant
        {
            let passed = std::process::Command::new(interpreter)
                .args([
                    python_bridge.as_os_str(),
                    std::ffi::OsStr::new("--callable"),
                    std::ffi::OsStr::new(callable),
                    std::ffi::OsStr::new("--check"),
                ])
                .current_dir(root)
                .status()
                .is_ok_and(|status| status.success());
            checks.push(serde_json::json!({
                "check": format!("variant.{name}.python_handshake"),
                "passed": passed,
                "required": true,
                "detail": {"callable": callable, "business_cases_executed": 0}
            }));
        }
    }
    for evaluator in &config.evaluators {
        if let structtrace_core::config::EvaluatorKind::Python {
            interpreter,
            callable,
            ..
        } = &evaluator.kind
        {
            let passed = std::process::Command::new(interpreter)
                .args([
                    evaluator_bridge.as_os_str(),
                    std::ffi::OsStr::new("--callable"),
                    std::ffi::OsStr::new(callable),
                    std::ffi::OsStr::new("--check"),
                ])
                .current_dir(root)
                .status()
                .is_ok_and(|status| status.success());
            checks.push(serde_json::json!({
                "check": format!("evaluator.{}.python_handshake", evaluator.id),
                "passed": passed,
                "required": true,
                "detail": {"callable": callable, "business_cases_executed": 0}
            }));
        }
    }
    if checks.is_empty() {
        checks.push(serde_json::json!({
            "check": "local_worker_handshake",
            "passed": true,
            "required": true,
            "detail": "No Python worker is configured; command workers remain statically checked because their v3 protocol has no side-effect-free startup request."
        }));
    }
    Ok(checks)
}

fn json_contains(haystack: &serde_json::Value, needle: &serde_json::Value) -> bool {
    haystack == needle
        || match haystack {
            serde_json::Value::Array(values) => {
                values.iter().any(|value| json_contains(value, needle))
            }
            serde_json::Value::Object(values) => {
                values.values().any(|value| json_contains(value, needle))
            }
            _ => false,
        }
}

fn scalar_leaves(value: &serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(values) => values.iter().flat_map(scalar_leaves).collect(),
        serde_json::Value::Object(values) => values.values().flat_map(scalar_leaves).collect(),
        serde_json::Value::Null => Vec::new(),
        scalar => vec![scalar.clone()],
    }
}

fn case_id_contains_label(case_id: &str, value: &serde_json::Value) -> bool {
    let label = match value {
        serde_json::Value::String(value) => value.trim().to_lowercase(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        _ => return false,
    };
    if label.is_empty() {
        return false;
    }
    let normalized_id = case_id.to_lowercase();
    normalized_id == label
        || normalized_id
            .split(|character: char| !character.is_alphanumeric())
            .any(|token| token == label)
}

fn evidence_unit_value(
    case: &structtrace_core::dataset::Case,
    config: &structtrace_core::config::EvidenceUnitConfig,
) -> anyhow::Result<serde_json::Value> {
    let envelope = serde_json::json!({
        "input": case.input,
        "expected": case.expected,
        "model_visible_metadata": case.model_visible_metadata,
        "metadata": case.metadata,
    });
    if let Some(pointer) = &config.pointer {
        return envelope.pointer(pointer).cloned().with_context(|| {
            format!(
                "evidence-unit pointer `{pointer}` did not resolve for `{}`",
                case.id
            )
        });
    }
    let include = config.include.clone().unwrap_or_else(|| {
        vec![
            "/input".to_owned(),
            "/expected".to_owned(),
            "/model_visible_metadata".to_owned(),
        ]
    });
    let mut selected = serde_json::Map::new();
    for pointer in include {
        let value = envelope.pointer(&pointer).cloned().with_context(|| {
            format!(
                "evidence-unit pointer `{pointer}` did not resolve for `{}`",
                case.id
            )
        })?;
        selected.insert(pointer, value);
    }
    Ok(serde_json::Value::Object(selected))
}

fn writable_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    let existed = path.exists();
    std::fs::create_dir_all(path)
        .with_context(|| format!("could not create storage directory {}", path.display()))?;
    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
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
    if let Some(run_dir) = &cli.run_dir {
        anyhow::ensure!(
            matches!(selector, "latest" | "latest-any"),
            "--run-dir cannot be combined with an explicit run selector"
        );
        let path = if run_dir.is_absolute() {
            run_dir.clone()
        } else {
            cli.project_root.join(run_dir)
        };
        anyhow::ensure!(path.is_dir(), "explicit run directory does not exist");
        anyhow::ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "explicit run directory must not be a symlink"
        );
        return Ok(path.canonicalize()?);
    }
    let storage = storage_root(cli)?;
    let runs = storage.join("runs");
    if matches!(
        selector,
        "latest" | "latest-any" | "latest-demo" | "latest-research" | "latest-test"
    ) {
        let required_kind = match selector {
            "latest" => Some(RunKind::Production),
            "latest-demo" => Some(RunKind::Demo),
            "latest-research" => Some(RunKind::ResearchFixture),
            "latest-test" => Some(RunKind::Test),
            _ => None,
        };
        let require_complete = selector != "latest-any";
        let mut candidates = std::fs::read_dir(&runs)
            .with_context(|| format!("no StructTrace runs found under {}", runs.display()))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter(|entry| {
                let manifest = read_json::<RunManifest>(&entry.path().join("manifest.json"));
                manifest.is_ok_and(|manifest| {
                    (!require_complete || manifest.status == RunStatus::Complete)
                        && required_kind.is_none_or(|kind| manifest.run_kind == kind)
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(std::fs::DirEntry::file_name);
        return candidates
            .last()
            .map(std::fs::DirEntry::path)
            .with_context(|| {
                if require_complete {
                    "no completed StructTrace run directories were found"
                } else {
                    "no StructTrace run directories were found"
                }
            });
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

fn storage_root(cli: &Cli) -> anyhow::Result<PathBuf> {
    let root = cli
        .project_root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", cli.project_root.display()))?;
    if let Some(storage) = &cli.storage_root {
        return Ok(resolve(&root, storage));
    }
    let config_path = resolve(&root, &cli.config);
    Ok(if config_path.is_file() {
        resolve(&root, &Config::load(&config_path)?.storage.root)
    } else {
        root.join(".structtrace")
    })
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

fn verify_manifest_artifact(run_dir: &Path, relative: &str) -> anyhow::Result<()> {
    let manifest: RunManifest = read_json(&run_dir.join("manifest.json"))?;
    let expected = manifest
        .artifacts
        .get(relative)
        .with_context(|| format!("manifest does not bind required artifact `{relative}`"))?;
    let relative_path = Path::new(relative);
    anyhow::ensure!(
        !relative_path.is_absolute()
            && relative_path
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
        "manifest contains unsafe artifact path `{relative}`"
    );
    let canonical_root = run_dir.canonicalize()?;
    let mut path = canonical_root.clone();
    for component in relative_path.components() {
        let std::path::Component::Normal(component) = component else {
            unreachable!()
        };
        path.push(component);
        anyhow::ensure!(
            !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "artifact `{relative}` contains a symbolic link"
        );
    }
    anyhow::ensure!(
        path.canonicalize()?.starts_with(canonical_root),
        "artifact `{relative}` escaped the run directory"
    );
    let actual = hash_file(&path)?;
    anyhow::ensure!(
        &actual == expected,
        "artifact `{relative}` failed manifest hash verification: expected {expected}, observed {actual}"
    );
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    structtrace_core::strict_json::from_slice(&bytes)
        .with_context(|| format!("invalid JSON in {}", path.display()))
}

fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn required_init_path(root: &Path, path: &Option<PathBuf>, flag: &str) -> anyhow::Result<PathBuf> {
    let path = path
        .as_ref()
        .with_context(|| format!("{flag} is required with --from-outputs"))?;
    Ok(if path.is_absolute() {
        path.clone()
    } else {
        root.join(path)
    })
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
