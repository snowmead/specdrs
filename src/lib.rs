//! Evidence-linked knowledge maps for Rust crates.
//!
//! The [`specdrs`] attribute appends local claims and evidence to rustdoc so they appear
//! in rust-analyzer hovers. [`specdrs_module`] applies memberships to every item in one
//! module. [`specdrs_span`] declares a span without an attribute host item.
//! `cargo specdrs show` provides inherited span claims.
//!
//! [`specdrs`]: crate::specdrs
//! [`specdrs_module`]: crate::specdrs_module
//! [`specdrs_span`]: crate::specdrs_span

mod analysis;
mod attribute;
mod build;
mod cli;
mod model;
pub mod prelude;
mod projection;

pub use analysis::{
    AnalysisExecution,
    AnalysisFinding,
    AnalysisReport,
    AnalysisTarget,
    AnalysisVerdict,
    AnalyzeError,
    AnalyzeOptions,
    AnalyzedTarget,
    FindingKind,
    TargetFilter, //
};
pub use build::{
    BuildError,
    BuildOptions, //
};
pub use cli::run_cli;
pub use specdrs_macros::{
    specdrs,
    specdrs_module,
    specdrs_span, //
};
pub use model::{
    Axis,
    AxisEntry,
    AxisStatus,
    Claim,
    ClaimKind,
    Evidence,
    EvidenceKind,
    EvidenceResult,
    Item,
    KnowledgeMap,
    SourcePosition,
    SourceRange,
    Span, //
};
pub use projection::{
    ClaimProjection,
    GroupKey,
    ProjectedClaim, //
};

specdrs_span!(
    id = "specdrs",
    entry = crate::build::BuildOptions::build,
    claims(
        Objectives(Job(
            "Keep engineering intent, implementation locations, and evidence navigable from one generated map."
                as purpose
        ),),
        Constraints(
            Interface(
                "Rust attributes are the authored source and schema 2 JSON is the generated interchange format."
                    as authored_and_generated_contract,
                "A span with children names its primary implementation chokepoint as its entry, which may coincide with a child span's entry."
                    as parent_entry_may_coincide,
                "Source runs through the scanner, schema and reference validation, the schema 2 map, then either a projection, a static check, or model audits."
                    as pipeline_order,
            ),
            Invariants(
                "Every span names one reading entry and lists exactly the Rust items assigned to it."
                    as navigable_subsystems,
                "Behaviour implemented in the proc-macro and syntax crates cannot appear in this map: the macro crate's own unit tests and the consumer rustdoc test cover it, and the syntax grammar is exercised through the adapter's tests."
                    as unmappable_crate_boundary,
            ),
            Failure(
                "Every command reports invalid input, build diagnostics, unavailable evidence, and non-passing analysis through a failing process status."
                    as failures_reach_the_caller,
            ),
            Authority(
                "No subsystem grants domain authority: the crate reads source, spawns `cargo metadata`, writes either the destination its arguments name or the documented default map path, and calls the configured model."
                    as no_domain_authority,
            ),
            Observation(
                "Static checks and semantic analysis report failures through the specdrs CLI."
                    as observable_failures,
            ),
        ),
        Assumptions(Assumptions(
            "Consumers treat generated maps and model verdicts as derived artifacts rather than authored source."
                as derived_outputs,
        ),),
        evidence(
            authored_and_generated_contract(Test = crate::build::tests::builds_own_knowledge_map),
            navigable_subsystems(Test = crate::build::tests::dogfood_map_covers_every_subsystem),
            parent_entry_may_coincide(Test = crate::build::tests::builds_own_knowledge_map),
            failures_reach_the_caller(
                Test = crate::cli::tests::run_cli_returns_failure_for_invalid_command
            ),
        ),
    ),
);
