//! Parses and executes `specdrs` and `cargo specdrs` commands.

use std::collections::BTreeMap;
use std::fs;
use std::path::{
    Path,
    PathBuf, //
};
use std::process::ExitCode;

use crate::{
    AnalysisExecution,
    AnalysisReport,
    AnalysisTarget,
    AnalysisVerdict,
    AnalyzeOptions,
    Axis,
    AxisEntry,
    AxisStatus,
    BuildOptions,
    ClaimKind,
    ClaimProjection,
    EvidenceResult,
    GroupKey,
    KnowledgeMap,
    ProjectedClaim,
    TargetFilter, //
};
use serde::Serialize;

use crate::prelude::*;

specdrs_module!(in_spans("command-line-interface"));

#[specdrs(
    span(
        id = "command-line-interface",
        parent = "specdrs",
        claims(
            Objectives(
                Job("Expose map emission, static checks, claim views, and semantic analysis as cargo subcommands." as purpose),
            ),
            Constraints(
                Interface(
                    "Each command accepts only its documented positional arguments and options." as command_contract,
                    "Show supports text and JSON projections with caller-selected grouping order." as show_contract,
                ),
                Effects(
                    "Emit writes one map file, its selected destination or the default map path whose parent it creates, and analyze contacts only the configured analyzer endpoint." as bounded_effects,
                ),
                Authority(
                    "The CLI exercises only the file writes and model calls its own arguments name." as bounded_authority,
                ),
                Failure(
                    "Invalid arguments, build failures, unavailable evidence, and non-passing analysis return a failing process status." as failure_exit,
                ),
                Observation(
                    "Machine-readable modes write versioned JSON and human modes identify fully qualified owners." as observable_output,
                ),
            ),
            Assumptions(
                Assumptions(
                    "The caller can read standard output and standard error and can access the selected Cargo manifest." as process_environment,
                ),
            ),
            NotApplicable(
                State = "Each invocation parses arguments and exits without retained process state.",
            ),
            evidence(
                command_contract(
                    Test = crate::cli::tests::conflicting_emit_destinations_fail,
                    Test = crate::cli::tests::cargo_forwarded_subcommand_name_is_accepted,
                ),
                show_contract(Test = crate::cli::tests::grouping_order_is_parsed),
                failure_exit(Test = crate::cli::tests::run_cli_returns_failure_for_invalid_command),
            ),
        )
    ),
    claims(
        Constraints(
            Failure(
                "A command error is written to standard error and returned as a failing exit code." as error_to_exit_status,
            ),
        ),
        evidence(
            error_to_exit_status(Test = crate::cli::tests::run_cli_returns_failure_for_invalid_command),
        ),
    )
)]
/// Runs one specdrs command and returns its process status.
pub fn run_cli(args: impl IntoIterator<Item = String>) -> ExitCode {
    match run(args.into_iter().collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Dispatches parsed process arguments to one CLI command.
///
/// # Errors
///
/// Returns an error when the command or any command argument is invalid, or when execution fails.
fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.first().is_some_and(|arg| arg == "specdrs") {
        args.remove(0);
    }
    let command = args.first().map(String::as_str).unwrap_or("help");
    if args[1..].iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    match command {
        "emit" => Args::parse(&args[1..])?.emit(),
        "check" => Args::parse(&args[1..])?.check(),
        "show" => Args::parse(&args[1..])?.show(),
        "analyze" => Args::parse(&args[1..])?.analyze(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" => {
            println!("specdrs {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        unknown => Err(format!(
            "unknown command `{unknown}`\n\nRun `cargo specdrs help`."
        )),
    }
}

#[derive(Debug)]
/// Contains parsed options shared across CLI commands.
struct Args {
    build: BuildOptions,
    stdout: bool,
    output: Option<PathBuf>,
    query: Option<String>,
    json: bool,
    group_by: Vec<GroupKey>,
    group_by_set: bool,
    jobs: Option<usize>,
    spans: Vec<String>,
    items: Vec<String>,
}

impl Args {
    /// Parses command arguments into one validated option set.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown options, missing values, invalid values, or extra arguments.
    #[specdrs(
    claims(
        Constraints(
            Interface(
                "Options with values consume exactly one following argument and one positional query is allowed." as option_arity,
                "An unset grouping order projects by kind, then axis, then owner." as default_projection_order,
            ),
            Failure(
                "Unknown options, missing values, invalid job counts, and extra positional arguments return errors." as invalid_arguments_fail,
            ),
        ),
        evidence(
            option_arity(Test = crate::cli::tests::grouping_order_is_parsed),
            invalid_arguments_fail(Test = crate::cli::tests::conflicting_emit_destinations_fail),
        ),
    )
    )]
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut parsed = Self {
            build: BuildOptions::default(),
            stdout: false,
            output: None,
            query: None,
            json: false,
            group_by: vec![GroupKey::Kind, GroupKey::Axis, GroupKey::Owner],
            group_by_set: false,
            jobs: None,
            spans: Vec::new(),
            items: Vec::new(),
        };
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--manifest-path" => {
                    index += 1;
                    parsed.build.manifest_path =
                        PathBuf::from(Self::required_value(args, index, "--manifest-path")?);
                }
                "--package" | "-p" => {
                    index += 1;
                    parsed.build.package =
                        Some(Self::required_value(args, index, "--package")?.to_owned());
                }
                "--output" | "-o" => {
                    index += 1;
                    parsed.output = Some(PathBuf::from(Self::required_value(
                        args, index, "--output",
                    )?));
                }
                "--stdout" => parsed.stdout = true,
                "--json" => parsed.json = true,
                "--group-by" => {
                    index += 1;
                    parsed.group_by =
                        Self::parse_grouping(Self::required_value(args, index, "--group-by")?)?;
                    parsed.group_by_set = true;
                }
                "--jobs" => {
                    index += 1;
                    let value = Self::required_value(args, index, "--jobs")?;
                    parsed.jobs = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| format!("invalid --jobs value `{value}`"))?,
                    );
                }
                "--span" => {
                    index += 1;
                    parsed
                        .spans
                        .push(Self::required_value(args, index, "--span")?.to_owned());
                }
                "--item" => {
                    index += 1;
                    parsed
                        .items
                        .push(Self::required_value(args, index, "--item")?.to_owned());
                }
                value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
                value if parsed.query.is_none() => parsed.query = Some(value.to_owned()),
                value => return Err(format!("unexpected argument `{value}`")),
            }
            index += 1;
        }
        Ok(parsed)
    }

    /// Parses a comma-separated grouping order.
    ///
    /// # Errors
    ///
    /// Returns an error when any grouping key is unknown.
    fn parse_grouping(value: &str) -> Result<Vec<GroupKey>, String> {
        value.split(',').map(str::parse).collect()
    }

    /// Reports whether any analysis target was named.
    fn has_target_selection(&self) -> bool {
        !self.spans.is_empty() || !self.items.is_empty()
    }

    /// Builds the analysis target selection.
    #[specdrs(
    claims(
        Constraints(
            Interface(
                "Naming no span and no item selects every claimed target." as empty_selection_is_all,
            ),
        ),
        evidence(
            empty_selection_is_all(Test = crate::cli::tests::target_selection_defaults_to_all),
        ),
    )
    )]
    fn target_filter(&self) -> TargetFilter {
        if self.has_target_selection() {
            TargetFilter::Named {
                spans: self.spans.clone(),
                items: self.items.clone(),
            }
        } else {
            TargetFilter::All
        }
    }

    /// Returns the value following an option.
    ///
    /// # Errors
    ///
    /// Returns an error when the option has no following value.
    fn required_value<'a>(
        args: &'a [String],
        index: usize,
        option: &str,
    ) -> Result<&'a str, String> {
        args.get(index)
            .map(String::as_str)
            .ok_or_else(|| format!("{option} requires a value"))
    }
}

impl Args {
    /// Emits the selected knowledge map to standard output or a file.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible options or failed map, serialization, or file operations.
    #[specdrs(
    claims(
        Constraints(
            Effects(
                "Emit builds the map, writes its selected destination or the default map path under the package target directory, and contacts no model." as emit_runs_no_analyzer,
            ),
        ),
        evidence(
            emit_runs_no_analyzer(Test = crate::cli::tests::conflicting_emit_destinations_fail),
        ),
    )
    )]
    fn emit(self) -> Result<(), String> {
        let args = self;
        if args.query.is_some()
            || args.json
            || args.group_by_set
            || args.jobs.is_some()
            || args.has_target_selection()
        {
            return Err(
                "emit accepts only --manifest-path, --package, --stdout, and --output".into(),
            );
        }
        if args.stdout && args.output.is_some() {
            return Err("use either `--stdout` or `--output`, not both".into());
        }
        let map = args.build.build().map_err(|error| error.to_string())?;
        let json = serde_json::to_string_pretty(&map)
            .map_err(|error| format!("cannot serialize knowledge map: {error}"))?;
        if args.stdout {
            println!("{json}");
            return Ok(());
        }

        let output = args
            .output
            .unwrap_or_else(|| default_output(&args.build, &map));
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::write(&output, format!("{json}\n"))
            .map_err(|error| format!("cannot write {}: {error}", output.display()))?;
        println!("{}", output.display());
        Ok(())
    }
}

/// Selects the default output path for an emitted map.
fn default_output(options: &BuildOptions, map: &KnowledgeMap) -> PathBuf {
    let manifest = &options.manifest_path;
    let base = if manifest
        .file_name()
        .is_some_and(|name| name == "Cargo.toml")
    {
        manifest.parent().unwrap_or_else(|| Path::new("."))
    } else {
        Path::new(".")
    };
    base.join("target")
        .join("specdrs")
        .join(format!("{}.json", map.crate_name))
}

impl Args {
    /// Checks the selected map for unavailable evidence and claims without evidence.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible options, a failed build, or unavailable evidence.
    #[specdrs(
    claims(
        Constraints(
            Interface(
                "Check validates syntax, span structure, references, and evidence resolution, compares no two builds, and asks no model to judge prose against code." as static_only_check,
            ),
            Effects(
                "Check reads the map and contacts no model." as check_runs_no_analyzer,
            ),
            Observation(
                "Check reports every claim without evidence and every unresolved evidence binder." as reports_gaps,
                "Check counts unspecified axes only for owners that engaged at least one axis, so the complete item index is not reported as a completeness gap." as scoped_unspecified_count,
            ),
        ),
    )
    )]
    fn check(self) -> Result<(), String> {
        let args = self;
        if args.stdout
            || args.output.is_some()
            || args.query.is_some()
            || args.json
            || args.group_by_set
            || args.jobs.is_some()
            || args.has_target_selection()
        {
            return Err("check accepts only --manifest-path and --package".into());
        }
        let map = args.build.build().map_err(|error| error.to_string())?;
        let mut unavailable = Vec::new();
        let mut unsupported = Vec::new();
        let mut unspecified = 0;

        for span in &map.spans {
            inspect_axes(
                &format!("span:{}", span.id),
                &span.axes,
                &mut unavailable,
                &mut unsupported,
                &mut unspecified,
            );
        }
        for (path, item) in &map.items {
            inspect_axes(
                &format!("item:{path}"),
                &item.axes,
                &mut unavailable,
                &mut unsupported,
                &mut unspecified,
            );
        }

        println!(
            "checked {} spans and {} items; {unspecified} unspecified axes; {} claims without evidence",
            map.spans.len(),
            map.items.len(),
            unsupported.len()
        );
        for claim in unsupported {
            println!("unsupported: {claim}");
        }
        if unavailable.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} evidence link(s) are unavailable:\n{}",
                unavailable.len(),
                unavailable.join("\n")
            ))
        }
    }
}

/// Accumulates static-check findings from one owner's axis map.
fn inspect_axes(
    scope: &str,
    axes: &BTreeMap<Axis, AxisEntry>,
    unavailable: &mut Vec<String>,
    unsupported: &mut Vec<String>,
    unspecified: &mut usize,
) {
    // An owner that declares no claim at all has twelve unspecified axes by
    // construction. Counting those would report the complete item index as a
    // completeness gap, so only owners that engaged with any axis are counted.
    let engaged = axes
        .values()
        .any(|entry| entry.status != AxisStatus::Unspecified);
    for (axis, entry) in axes {
        if engaged && entry.status == AxisStatus::Unspecified {
            *unspecified += 1;
        }
        for claim in &entry.claims {
            if claim.evidence.is_empty() {
                unsupported.push(format!("{scope}/{} [{axis}, {:?}]", claim.id, claim.kind));
            }
            for evidence in &claim.evidence {
                if evidence.result == EvidenceResult::Unavailable {
                    unavailable.push(format!(
                        "{scope}/{}: {:?} {}",
                        claim.id, evidence.kind, evidence.binder
                    ));
                }
            }
        }
    }
}

#[derive(Serialize)]
/// Contains the versioned JSON representation of a `show` result.
struct ShowJson<'a> {
    schema: u32,
    target: &'a str,
    group_by: &'a [GroupKey],
    claims: &'a [ProjectedClaim],
}

impl Args {
    /// Displays one span or item claim projection.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible options, unknown targets, invalid grouping, or failed serialization.
    #[specdrs(
    claims(
        Constraints(
            Effects(
                "Show projects stored claims and contacts no model." as show_runs_no_analyzer,
            ),
            Observation(
                "A JSON projection carries its own schema number, independent of the map schema." as projection_schema,
            ),
        ),
        evidence(
            show_runs_no_analyzer(Test = crate::cli::tests::grouping_order_is_parsed),
        ),
    )
    )]
    fn show(self) -> Result<(), String> {
        let args = self;
        if args.stdout
            || args.output.is_some()
            || args.jobs.is_some()
            || args.has_target_selection()
        {
            return Err(
                "show does not accept --stdout, --output, --jobs, --span, or --item".into(),
            );
        }
        let query = args
            .query
            .as_deref()
            .ok_or_else(|| "show requires a span id or item def path".to_owned())?;
        let map = args.build.build().map_err(|error| error.to_string())?;
        let (target, claims, unspecified) =
            if let Some(span) = map.spans.iter().find(|span| span.id == query) {
                if !args.json {
                    println!("span:{query}");
                    if let Some(parent) = &span.parent {
                        println!("parent: {parent}");
                    }
                    println!("entry: {}", span.entry);
                    println!("members:");
                    for member in &span.members {
                        println!("  {member}");
                    }
                }
                (
                    format!("span:{query}"),
                    map.span_claims(query).expect("validated span query"),
                    unspecified_axes(&span.axes),
                )
            } else if let Some(item) = map.items.get(query) {
                if !args.json {
                    println!("item:{query}");
                    println!(
                        "source: {}:{}:{}-{}:{}",
                        item.source.file,
                        item.source.start.line,
                        item.source.start.column,
                        item.source.end.line,
                        item.source.end.column
                    );
                    println!("signature: {}", item.signature);
                    if !item.spans.is_empty() {
                        println!("spans: {}", item.spans.join(", "));
                    }
                }
                (
                    format!("item:{query}"),
                    map.item_claims(query).expect("validated item query"),
                    unspecified_axes(&item.axes),
                )
            } else {
                return Err(format!("no span or item named `{query}`"));
            };
        let projection = ClaimProjection::new(claims, args.group_by)?;
        if args.json {
            let output = ShowJson {
                schema: 1,
                target: &target,
                group_by: &projection.group_by,
                claims: &projection.claims,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&output)
                    .map_err(|error| format!("cannot serialize projection: {error}"))?
            );
        } else {
            print_projection(&projection);
            if !unspecified.is_empty() {
                println!("unspecified: {}", unspecified.join(", "));
            }
        }
        Ok(())
    }
}

/// Returns the names of every unspecified axis.
fn unspecified_axes(axes: &BTreeMap<Axis, AxisEntry>) -> Vec<String> {
    axes.iter()
        .filter(|(_, entry)| entry.status == AxisStatus::Unspecified)
        .map(|(axis, _)| axis.to_string())
        .collect()
}

/// Prints a grouped claim projection for human readers.
fn print_projection(projection: &ClaimProjection) {
    let mut previous: Vec<String> = Vec::new();
    for projected in &projection.claims {
        let values: Vec<_> = projection
            .group_by
            .iter()
            .map(|key| group_value(*key, projected))
            .collect();
        let changed = values
            .iter()
            .zip(&previous)
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| previous.len().min(values.len()));
        for (depth, (key, value)) in projection
            .group_by
            .iter()
            .zip(&values)
            .enumerate()
            .skip(changed)
        {
            println!("{}{}: {value}", "  ".repeat(depth), key);
        }
        println!(
            "{}- {}/{}: {}",
            "  ".repeat(projection.group_by.len()),
            projected.owner,
            projected.claim.id,
            projected.claim.text
        );
        for evidence in &projected.claim.evidence {
            println!(
                "{}evidence {:?}: {} [{:?}]",
                "  ".repeat(projection.group_by.len() + 1),
                evidence.kind,
                evidence.binder,
                evidence.result
            );
        }
        previous = values;
    }
}

/// Returns the display value for one projected claim grouping key.
fn group_value(key: GroupKey, projected: &ProjectedClaim) -> String {
    match key {
        GroupKey::Owner => projected.owner.clone(),
        GroupKey::Kind => match projected.claim.kind {
            ClaimKind::Objective => "objectives".into(),
            ClaimKind::Constraint => "constraints".into(),
            ClaimKind::Assumption => "assumptions".into(),
        },
        GroupKey::Axis => projected.axis.to_string(),
    }
}

impl Args {
    /// Runs semantic analysis and prints its report.
    ///
    /// # Errors
    ///
    /// Returns an error for incompatible options, runtime failures, analysis failures, or non-passing reports.
    fn analyze(self) -> Result<(), String> {
        let args = self;
        if args.stdout || args.output.is_some() || args.query.is_some() || args.group_by_set {
            return Err(
                "analyze accepts only --manifest-path, --package, --jobs, --span, --item, and --json".into(),
            );
        }
        let targets = args.target_filter();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("cannot start analyzer runtime: {error}"))?;
        let report = runtime
            .block_on(
                AnalyzeOptions {
                    build: args.build,
                    jobs: args.jobs,
                    targets,
                }
                .analyze(),
            )
            .map_err(|error| error.to_string())?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| format!("cannot serialize analysis report: {error}"))?
            );
        } else {
            print_analysis(&report);
        }
        if report.passed() {
            Ok(())
        } else {
            Err("analysis did not pass".into())
        }
    }
}

/// Prints a semantic analysis report for human readers.
fn print_analysis(report: &AnalysisReport) {
    println!(
        "analyzed {} with {}/{}",
        report.crate_name, report.provider, report.model
    );
    for target in &report.results {
        let label = match &target.target {
            AnalysisTarget::Span { id } => format!("span:{id}"),
            AnalysisTarget::Item { path } => format!("item:{path}"),
        };
        match &target.result {
            AnalysisExecution::Completed {
                verdict: AnalysisVerdict::Pass,
            } => println!("pass: {label}"),
            AnalysisExecution::Completed {
                verdict: AnalysisVerdict::Indeterminate { reason },
            } => println!("indeterminate: {label}: {reason}"),
            AnalysisExecution::Completed {
                verdict: AnalysisVerdict::Fail { findings },
            } => {
                println!("fail: {label}");
                for finding in findings {
                    println!(
                        "  {:?}: {} [{}]",
                        finding.kind,
                        finding.reason,
                        finding.claims.join(", ")
                    );
                    for range in &finding.ranges {
                        println!(
                            "    {}:{}:{}-{}:{}",
                            range.file,
                            range.start.line,
                            range.start.column,
                            range.end.line,
                            range.end.column
                        );
                    }
                }
            }
            AnalysisExecution::Error { message } => println!("error: {label}: {message}"),
        }
    }
}

/// Prints command usage and option help.
fn print_help() {
    println!(
        "specdrs knowledge maps\n\n\
Usage:\n  cargo specdrs emit [--stdout | --output <path>] [OPTIONS]\n  \
cargo specdrs check [OPTIONS]\n  \
cargo specdrs show <span-or-item> [--group-by kind,axis,owner] [--json] [OPTIONS]\n  \
cargo specdrs analyze [--span <id>]... [--item <path>]... [--jobs <count>] [--json] [OPTIONS]\n\n\
Options:\n  --manifest-path <path>  Cargo.toml to inspect\n  -p, --package <name>    package in a workspace\n  \
--span <id>             analyze only this span; repeatable\n  \
--item <path>           analyze only this item; repeatable\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_forwarded_subcommand_name_is_accepted() {
        run(vec!["specdrs".into(), "--version".into()]).unwrap();
    }

    #[test]
    fn conflicting_emit_destinations_fail() {
        let error = run(vec![
            "emit".into(),
            "--stdout".into(),
            "--output".into(),
            "map.json".into(),
        ])
        .unwrap_err();
        assert!(error.contains("either"));
    }

    #[test]
    fn grouping_order_is_parsed() {
        let parsed = Args::parse(&[
            "target".into(),
            "--group-by".into(),
            "owner,kind,axis".into(),
        ])
        .unwrap();
        assert_eq!(
            parsed.group_by,
            vec![GroupKey::Owner, GroupKey::Kind, GroupKey::Axis]
        );
    }

    #[test]
    fn target_selection_defaults_to_all() {
        assert_eq!(Args::parse(&[]).unwrap().target_filter(), TargetFilter::All);
        let parsed = Args::parse(&[
            "--span".into(),
            "checkout".into(),
            "--item".into(),
            "payments::charge".into(),
            "--span".into(),
            "payments".into(),
        ])
        .unwrap();
        assert_eq!(
            parsed.target_filter(),
            TargetFilter::Named {
                spans: vec!["checkout".into(), "payments".into()],
                items: vec!["payments::charge".into()],
            }
        );
    }

    #[test]
    fn run_cli_returns_failure_for_invalid_command() {
        assert_eq!(run_cli(["not-a-command".into()]), ExitCode::FAILURE);
    }
}
