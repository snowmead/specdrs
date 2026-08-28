//! Builds validated knowledge maps from reachable Rust source files.

use std::collections::{
    BTreeMap,
    BTreeSet, //
};
use std::fmt;
use std::fs;
use std::path::{
    Path,
    PathBuf, //
};
use std::process::Command;
use std::sync::Arc;
use std::thread;

use proc_macro2::Span as TokenSpan;
use quote::ToTokens;
use serde::Deserialize;
use syn::spanned::Spanned;
use syn::{
    Attribute,
    ImplItem,
    Item as SynItem,
    TraitItem,
    Type, //
};

use crate::prelude::*;

use specdrs_syntax::{
    impl_cannot_own_claims,
    specdrs_module_requires_in_spans,
    specdrs_requires_arguments,
    specdrs_span_requires_entry, //
};

use crate::attribute::{
    ClaimArgs,
    ClaimsArgs,
    Directive,
    NotApplicableArgs,
    SpanArgs, //
    SpecdrsArgs,
};
use crate::model::{
    Axis,
    AxisEntry,
    AxisStatus,
    Claim,
    Evidence,
    EvidenceKind,
    EvidenceResult,
    Item,
    KnowledgeMap,
    SourcePosition,
    SourceRange,
    Span, //
};

#[derive(Debug, Clone)]
/// Selects the Cargo package to map.
pub struct BuildOptions {
    /// Points to a package or workspace manifest.
    pub manifest_path: PathBuf,
    /// Selects a package when the manifest belongs to a workspace.
    pub package: Option<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            manifest_path: PathBuf::from("Cargo.toml"),
            package: None,
        }
    }
}

#[derive(Debug)]
/// Reports metadata, source parsing, annotation, and map validation failures.
pub struct BuildError {
    messages: Vec<String>,
}

impl BuildError {
    /// Creates an error containing one diagnostic.
    fn one(message: impl Into<String>) -> Self {
        Self {
            messages: vec![message.into()],
        }
    }

    /// Creates an error from collected diagnostics.
    fn many(messages: Vec<String>) -> Self {
        Self { messages }
    }

    /// Returns each collected diagnostic in deterministic order.
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, message) in self.messages.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            write!(formatter, "{message}")?;
        }
        Ok(())
    }
}

impl std::error::Error for BuildError {}

impl BuildOptions {
    /// Builds a knowledge map from the selected Cargo package.
    ///
    /// # Errors
    ///
    /// Returns every source, metadata, annotation, and map validation error found.
    #[specdrs(
    span(
        id = "knowledge-map-build",
        parent = "specdrs",
        claims(
            Objectives(
                Job(
                    "Turn reachable Rust source annotations into a complete knowledge map." as purpose,
                ),
                Time("Minimize p95 wall time for a cold build invocation." as cold_p95_latency),
                Resources(
                    "Avoid parallel coordination when it costs more than sequential parsing." as adaptive_scheduling,
                ),
            ),
            Constraints(
                Interface(
                    "Every supported engineering annotation in reachable source contributes to the map." as annotation_contract,
                ),
                Effects(
                    "A build spawns `cargo metadata`, reads files reachable from the selected crate root, and writes nothing outside the returned map." as bounded_build_effects,
                ),
                Invariants(
                    "Every reachable Rust file is parsed into a full syn AST." as full_syn_parse,
                    "Identical source produces identical map data and ordering." as deterministic_map,
                    "Identical failures produce diagnostics in the same order." as deterministic_errors,
                ),
                State(
                    "A build retains no parser state after returning its map or error." as one_shot_state,
                ),
                Failure(
                    "Return every reachable parse failure in deterministic order." as collect_parse_failures,
                ),
                Resources(
                    "Concurrent parser work does not exceed available parallelism." as bounded_cpu,
                    "Performance is measured with configurable generated Rust fixtures." as generated_corpus,
                    "Benchmarks report phase-level and end-to-end p50 and p95 wall time." as latency_distribution,
                    "The build-only synthetic benchmark is runnable under an external peak-RSS measurement." as peak_memory,
                ),
                Observation(
                    "Every reachable parse and validation failure is reported through the returned build error." as reported_diagnostics,
                ),
            ),
            Assumptions(
                Assumptions(
                    "Supported crates range from a couple of files to several dozen files and tens of thousands of Rust lines, and the benchmark corpus is generated across that range rather than checked in." as mixed_crate_sizes,
                ),
                Change(
                    "Downstream callers can adopt breaking map changes in one release." as breaking_redesign_allowed,
                ),
            ),
            evidence(
                annotation_contract(Test = crate::build::tests::builds_own_knowledge_map),
                deterministic_map(Test = crate::build::tests::own_map_is_deterministic),
            ),
        )
    )
    )]
    pub fn build(&self) -> Result<KnowledgeMap, BuildError> {
        let manifest_path = self.absolute_manifest_path()?;
        let metadata = Metadata::load(&manifest_path)?;
        let package = metadata.select_package(&manifest_path, self.package.as_deref())?;
        let target = package.select_target()?;
        let manifest_dir = Path::new(&package.manifest_path)
            .parent()
            .ok_or_else(|| BuildError::one("package manifest has no parent directory"))?;
        let crate_name = target.name.replace('-', "_");

        let root = PathBuf::from(&target.src_path);
        let module_dir = root
            .parent()
            .ok_or_else(|| BuildError::one("crate root has no parent directory"))?
            .to_path_buf();
        let mut scanner = Scanner::new(manifest_dir.to_path_buf());
        scanner.scan_crate(root, vec![crate_name.clone()], module_dir);
        if !scanner.errors.is_empty() {
            return Err(BuildError::many(scanner.errors));
        }

        MapAssembler::new(
            crate_name,
            scanner.items,
            scanner.aliases,
            scanner.span_declarations,
            scanner.container_members,
        )
        .assemble()
    }

    /// Resolves the selected workspace root, package root, and package name.
    ///
    /// # Errors
    ///
    /// Returns an error when Cargo metadata fails or the package cannot be selected.
    pub(crate) fn project_roots(&self) -> Result<(PathBuf, PathBuf, String), BuildError> {
        let manifest_path = self.absolute_manifest_path()?;
        let metadata = Metadata::load(&manifest_path)?;
        let package = metadata.select_package(&manifest_path, self.package.as_deref())?;
        let package_root = Path::new(&package.manifest_path)
            .parent()
            .ok_or_else(|| BuildError::one("package manifest has no parent directory"))?
            .to_path_buf();
        Ok((
            PathBuf::from(&metadata.workspace_root),
            package_root,
            package.name.clone(),
        ))
    }

    /// Resolves and canonicalizes the configured manifest path.
    ///
    /// # Errors
    ///
    /// Returns an error when the current directory or manifest path cannot be read.
    fn absolute_manifest_path(&self) -> Result<PathBuf, BuildError> {
        let path = if self.manifest_path.is_absolute() {
            self.manifest_path.clone()
        } else {
            std::env::current_dir()
                .map_err(|error| {
                    BuildError::one(format!("cannot read current directory: {error}"))
                })?
                .join(&self.manifest_path)
        };
        path.canonicalize()
            .map_err(|error| BuildError::one(format!("cannot open {}: {error}", path.display())))
    }
}

#[derive(Debug, Deserialize)]
/// Contains the subset of Cargo metadata needed for package selection.
struct Metadata {
    packages: Vec<Package>,
    workspace_root: String,
}

#[derive(Debug, Deserialize)]
/// Contains the subset of Cargo package metadata needed for target selection.
struct Package {
    name: String,
    manifest_path: String,
    targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
/// Contains the subset of Cargo target metadata needed for source scanning.
struct Target {
    name: String,
    kind: Vec<String>,
    src_path: String,
}

impl Metadata {
    /// Loads Cargo metadata for one manifest without dependency data.
    ///
    /// # Errors
    ///
    /// Returns an error when Cargo cannot run, rejects the manifest, or emits invalid JSON.
    fn load(manifest_path: &Path) -> Result<Self, BuildError> {
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--manifest-path",
            ])
            .arg(manifest_path)
            .output()
            .map_err(|error| BuildError::one(format!("cannot run cargo metadata: {error}")))?;
        if !output.status.success() {
            return Err(BuildError::one(format!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| BuildError::one(format!("invalid cargo metadata: {error}")))
    }

    /// Selects the requested package or infers it from the manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the package is absent or a workspace selection is ambiguous.
    fn select_package(
        &self,
        manifest_path: &Path,
        requested: Option<&str>,
    ) -> Result<&Package, BuildError> {
        if let Some(requested) = requested {
            return self
                .packages
                .iter()
                .find(|package| package.name == requested)
                .ok_or_else(|| BuildError::one(format!("package `{requested}` was not found")));
        }

        let exact = self.packages.iter().find(|package| {
            Path::new(&package.manifest_path)
                .canonicalize()
                .is_ok_and(|path| path == manifest_path)
        });
        if let Some(package) = exact {
            return Ok(package);
        }
        if self.packages.len() == 1 {
            return Ok(&self.packages[0]);
        }
        Err(BuildError::one(
            "the manifest is a workspace; select one crate with `--package <name>`",
        ))
    }
}

impl Package {
    /// Selects the package's library target or first binary target.
    ///
    /// # Errors
    ///
    /// Returns an error when the package has no library or binary target.
    fn select_target(&self) -> Result<&Target, BuildError> {
        self.targets
            .iter()
            .find(|target| target.kind.iter().any(|kind| kind == "lib"))
            .or_else(|| {
                self.targets
                    .iter()
                    .find(|target| target.kind.iter().any(|kind| kind == "bin"))
            })
            .ok_or_else(|| {
                BuildError::one(format!(
                    "package `{}` has no library or binary target",
                    self.name
                ))
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Classifies a scanned Rust item for evidence compatibility.
enum ItemShape {
    Function { test: bool },
    Type,
    Other,
}

/// Contains one scanned item before map assembly.
struct ScannedItem {
    source: Option<ScannedSource>,
    shape: ItemShape,
    annotations: Vec<SpecdrsArgs>,
    inherited_spans: BTreeSet<String>,
}

/// Contains source metadata retained for one scanned item.
struct ScannedSource {
    file: Arc<str>,
    start: SourcePosition,
    end: SourcePosition,
    signature: String,
    module_path: Vec<String>,
}

#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
/// Contains one span declared by a [`specdrs_span!`] invocation.
///
/// The invocation has no host item, so it carries the module path of its own
/// scope and names its entry explicitly.
///
/// [`specdrs_span!`]: crate::specdrs_span
struct ScannedSpan {
    id: String,
    parent: Option<String>,
    entry: String,
    claims: Option<ClaimsArgs>,
    module_path: Vec<String>,
    /// Names the container that declared this span, when one did.
    container: Option<String>,
}

#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
/// Owns crate traversal state and canonical scan indexes.
struct Scanner {
    manifest_dir: PathBuf,
    items: BTreeMap<String, ScannedItem>,
    aliases: BTreeMap<String, Vec<String>>,
    span_declarations: Vec<ScannedSpan>,
    container_members: BTreeMap<String, BTreeSet<String>>,
    errors: Vec<String>,
}

#[derive(Clone)]
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
/// Describes one source file waiting to be scanned.
struct FileTask {
    path: PathBuf,
    module_path: Vec<String>,
    module_dir: PathBuf,
    ancestors: Vec<PathBuf>,
    inherited_spans: BTreeSet<String>,
    module_item: Option<String>,
}

#[derive(Default)]
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
/// Contains the items, child tasks, and diagnostics produced by one file.
struct FileScan {
    items: Vec<(String, Option<String>, ScannedItem)>,
    children: Vec<FileTask>,
    errors: Vec<String>,
    module_membership: Option<(String, BTreeSet<String>)>,
    span_declarations: Vec<ScannedSpan>,
    container_members: BTreeMap<String, BTreeSet<String>>,
}

/// Contains a canonical item path and optional alternate alias.
struct ItemIdentity {
    path: String,
    alias: Option<String>,
}

impl ItemIdentity {
    /// Creates an identity with no alternate alias.
    fn new(path: String) -> Self {
        Self { path, alias: None }
    }
}

impl Scanner {
    /// Creates an empty scanner rooted at one package manifest directory.
    fn new(manifest_dir: PathBuf) -> Self {
        Self {
            manifest_dir,
            items: BTreeMap::new(),
            aliases: BTreeMap::new(),
            span_declarations: Vec::new(),
            container_members: BTreeMap::new(),
            errors: Vec::new(),
        }
    }

    /// Scans a crate root and every reachable module into deterministic indexes.
    ///
    /// Source, parse, traversal, and duplicate-item failures are accumulated in [`errors`].
    ///
    /// [`errors`]: crate::build::Scanner::errors
    #[specdrs(
        span(
            id = "knowledge-map-build.source-scanning",
            parent = "knowledge-map-build",
            claims(
                Objectives(
                    Job("Discover and parse every Rust item reachable from the selected crate target." as purpose),
                ),
                Constraints(
                    Effects(
                        "Scanning reads only files reachable from the selected crate target and writes none of them." as bounded_scan_effects,
                    ),
                    Invariants(
                        "Module traversal follows inline modules, conventional files, and explicit path attributes without revisiting an ancestor file." as module_traversal,
                        "Every scanned item retains its def path, signature, file, and complete source range." as source_metadata,
                        "Scanner output and diagnostics are deterministic across worker schedules." as deterministic_scan,
                    ),
                    Failure(
                        "Unreadable modules, parse failures, inclusion cycles, and duplicate annotated item paths are collected as diagnostics." as scan_failures,
                    ),
                    Resources(
                        "Small frontiers run sequentially and larger frontiers use at most available parallelism." as bounded_parallel_scan,
                    ),
                ),
                evidence(
                    source_metadata(Test = crate::build::tests::builds_own_knowledge_map),
                    deterministic_scan(Test = crate::build::tests::own_map_is_deterministic),
                    bounded_parallel_scan(Test = crate::build::tests::builds_own_knowledge_map),
                ),
            )
        ),
        claims(
            Constraints(
                State(
                    "The traversal frontier is drained before scan_crate returns and no worker survives the call." as drains_frontier,
                ),
                Invariants(
                    "Collected aliases and diagnostics are sorted and deduplicated before assembly." as canonical_scan_output,
                ),
            ),
        )
    )]
    fn scan_crate(&mut self, path: PathBuf, module_path: Vec<String>, module_dir: PathBuf) {
        let mut frontier = vec![FileTask {
            path,
            module_path,
            module_dir,
            ancestors: Vec::new(),
            inherited_spans: BTreeSet::new(),
            module_item: None,
        }];
        while !frontier.is_empty() {
            let scans = self.scan_batch(frontier);
            frontier = Vec::new();
            for scan in scans {
                let FileScan {
                    items,
                    children,
                    errors,
                    module_membership,
                    span_declarations,
                    container_members,
                } = scan;
                self.errors.extend(errors);
                self.span_declarations.extend(span_declarations);
                for (id, members) in container_members {
                    self.container_members
                        .entry(id)
                        .or_default()
                        .extend(members);
                }
                frontier.extend(children);
                for (path, alias, item) in items {
                    if let Some(alias) = alias {
                        self.aliases.entry(alias).or_default().push(path.clone());
                    }
                    match self.items.entry(path.clone()) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(item);
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            let existing = entry.get();
                            let existing_is_empty = existing.annotations.is_empty()
                                && existing.inherited_spans.is_empty();
                            let item_is_empty =
                                item.annotations.is_empty() && item.inherited_spans.is_empty();
                            match (existing_is_empty, item_is_empty) {
                                (true, true) | (false, true) => {}
                                (true, false) => {
                                    entry.insert(item);
                                }
                                (false, false) => {
                                    self.errors.push(format!(
                                        "duplicate item path `{path}`. Two annotated items resolved to the same definition path. Give one a distinct identity, or drop the extra annotation"
                                    ));
                                }
                            }
                        }
                    }
                }
                if let Some((path, spans)) = module_membership {
                    self.items
                        .get_mut(&path)
                        .expect("a file-backed module is recorded before its file is scanned")
                        .inherited_spans
                        .extend(spans);
                }
            }
        }
        for candidates in self.aliases.values_mut() {
            candidates.sort();
            candidates.dedup();
        }
        self.errors.sort();
        self.errors.dedup();
    }

    /// Scans one traversal frontier sequentially or with bounded worker threads.
    #[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Resources(
                "File batches smaller than four tasks run sequentially." as small_batches_are_sequential,
                "The worker count does not exceed available parallelism." as workers_are_bounded,
            ),
        ),
    )
    )]
    fn scan_batch(&self, tasks: Vec<FileTask>) -> Vec<FileScan> {
        const PARALLEL_FILE_THRESHOLD: usize = 4;

        let parallelism = thread::available_parallelism().map_or(1, usize::from);
        if tasks.len() < PARALLEL_FILE_THRESHOLD || parallelism == 1 {
            return tasks
                .into_iter()
                .map(|task| FileScan::scan(task, &self.manifest_dir))
                .collect();
        }

        let worker_count = parallelism.min(tasks.len());
        let chunk_size = tasks.len().div_ceil(worker_count);
        thread::scope(|scope| {
            let handles: Vec<_> = tasks
                .chunks(chunk_size)
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .cloned()
                            .map(|task| FileScan::scan(task, &self.manifest_dir))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let mut scans = Vec::with_capacity(tasks.len());
            for handle in handles {
                match handle.join() {
                    Ok(batch) => scans.extend(batch),
                    Err(_) => scans.push(FileScan {
                        errors: vec!["scanner worker panicked".into()],
                        ..FileScan::default()
                    }),
                }
            }
            scans
        })
    }
}

impl FileScan {
    /// Reads, parses, and traverses one Rust source file.
    ///
    /// Failures are returned in [`errors`] so sibling files can still be scanned.
    ///
    /// [`errors`]: crate::build::FileScan::errors
    #[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Invariants(
                "A source file is fully parsed before any item in that file is traversed." as parse_before_traversal,
            ),
        ),
    )
    )]
    fn scan(task: FileTask, manifest_dir: &Path) -> Self {
        let FileTask {
            path,
            module_path,
            module_dir,
            mut ancestors,
            mut inherited_spans,
            module_item,
        } = task;
        let mut scan = FileScan::default();
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                scan.errors
                    .push(format!("cannot open module {}: {error}", path.display()));
                return scan;
            }
        };
        if ancestors.contains(&canonical) {
            scan.errors.push(format!(
                "module inclusion cycle reaches {}",
                canonical.display()
            ));
            return scan;
        }

        let source = match fs::read_to_string(&canonical) {
            Ok(source) => source,
            Err(error) => {
                scan.errors
                    .push(format!("cannot read {}: {error}", canonical.display()));
                return scan;
            }
        };
        let parsed = match syn::parse_file(&source) {
            Ok(parsed) => parsed,
            Err(error) => {
                scan.errors
                    .push(format!("cannot parse {}: {error}", canonical.display()));
                return scan;
            }
        };

        ancestors.push(canonical.clone());
        let file = Arc::<str>::from(display_path(&canonical, manifest_dir));
        let declared_spans = module_memberships(&parsed.items, &file, &mut scan.errors);
        scan.span_declarations.extend(module_span_declarations(
            &parsed.items,
            &file,
            &module_path,
            &mut scan.errors,
        ));
        if let Some(module_item) = module_item {
            scan.module_membership = Some((module_item, declared_spans.clone()));
        }
        inherited_spans.extend(declared_spans);
        for item in &parsed.items {
            scan.scan_item(
                item,
                &canonical,
                (&file, &module_path),
                &module_dir,
                &ancestors,
                &inherited_spans,
            );
        }
        scan
    }

    /// Records one Rust item and recursively follows nested modules and implementations.
    #[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Interface(
                "A span declared or joined on a container applies to every item inside that container." as container_distribution,
            ),
            Failure(
                "A span declared on an impl block without `entry`, and a claims block on an impl block, each add a file-and-line diagnostic." as impl_declaration_diagnostics,
            ),
        ),
        evidence(
            container_distribution(Test = crate::build::tests::containers_distribute_declared_and_joined_spans),
            impl_declaration_diagnostics(Test = crate::build::tests::impl_declarations_reject_missing_entry_and_claims),
        ),
    )
    )]
    fn scan_item(
        &mut self,
        item: &SynItem,
        file: &Path,
        location: (&Arc<str>, &[String]),
        module_dir: &Path,
        ancestors: &[PathBuf],
        inherited_spans: &BTreeSet<String>,
    ) {
        let (display_file, module_path) = location;
        match item {
            SynItem::Fn(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.sig.ident.to_string())),
                    value.span(),
                    || value.sig.to_token_stream().to_string(),
                    ItemShape::Function {
                        test: has_attribute(&value.attrs, "test"),
                    },
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Struct(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("struct {}{}", value.ident, value.generics.to_token_stream()),
                    ItemShape::Type,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Enum(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("enum {}{}", value.ident, value.generics.to_token_stream()),
                    ItemShape::Type,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Union(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("union {}{}", value.ident, value.generics.to_token_stream()),
                    ItemShape::Type,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Type(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("type {}{}", value.ident, value.generics.to_token_stream()),
                    ItemShape::Type,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Trait(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("trait {}{}", value.ident, value.generics.to_token_stream()),
                    ItemShape::Type,
                    (display_file, module_path, inherited_spans),
                );
                let mut trait_path = module_path.to_vec();
                trait_path.push(value.ident.to_string());
                for trait_item in &value.items {
                    if let TraitItem::Fn(method) = trait_item {
                        self.record(
                            &method.attrs,
                            ItemIdentity::new(item_path(
                                &trait_path,
                                &method.sig.ident.to_string(),
                            )),
                            method.span(),
                            || method.sig.to_token_stream().to_string(),
                            ItemShape::Function {
                                test: has_attribute(&method.attrs, "test"),
                            },
                            (display_file, module_path, inherited_spans),
                        );
                    }
                }
            }
            SynItem::Const(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("const {}: {}", value.ident, value.ty.to_token_stream()),
                    ItemShape::Other,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Static(value) => {
                self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("static {}: {}", value.ident, value.ty.to_token_stream()),
                    ItemShape::Other,
                    (display_file, module_path, inherited_spans),
                );
            }
            SynItem::Mod(value) => {
                let mut child_path = module_path.to_vec();
                child_path.push(value.ident.to_string());
                if let Some((_, items)) = &value.content {
                    let declarations = module_span_declarations(
                        items,
                        display_file,
                        &child_path,
                        &mut self.errors,
                    );
                    self.span_declarations.extend(declarations);
                }
                let declared_spans = value
                    .content
                    .as_ref()
                    .map_or_else(BTreeSet::new, |(_, items)| {
                        module_memberships(items, display_file, &mut self.errors)
                    });
                let mut module_spans = inherited_spans.clone();
                module_spans.extend(declared_spans);
                let direct_spans = self.record(
                    &value.attrs,
                    ItemIdentity::new(item_path(module_path, &value.ident.to_string())),
                    value.span(),
                    || format!("mod {}", value.ident),
                    ItemShape::Other,
                    (display_file, module_path, &module_spans),
                );
                let mut child_spans = module_spans;
                child_spans.extend(direct_spans);
                let child_item = item_path(module_path, &value.ident.to_string());
                let child_dir = module_dir.join(value.ident.to_string());
                if let Some((_, items)) = &value.content {
                    for child in items {
                        self.scan_item(
                            child,
                            file,
                            (display_file, &child_path),
                            &child_dir,
                            ancestors,
                            &child_spans,
                        );
                    }
                } else if let Some(path) = module_file(value, file, module_dir) {
                    self.children.push(FileTask {
                        path,
                        module_path: child_path,
                        module_dir: child_dir,
                        ancestors: ancestors.to_vec(),
                        inherited_spans: child_spans,
                        module_item: Some(child_item),
                    });
                }
            }
            SynItem::Impl(value) => {
                let line = value.span().start().line;
                let annotations =
                    parse_annotations(&value.attrs, display_file, line, &mut self.errors);
                let mut impl_spans = inherited_spans.clone();
                let mut declared = Vec::new();
                for directive in annotations
                    .iter()
                    .flat_map(|annotation| &annotation.directives)
                {
                    match directive {
                        Directive::InSpans(ids) => impl_spans.extend(ids.iter().cloned()),
                        Directive::Span(declaration) => {
                            let Some(entry) = declaration.entry.clone() else {
                                self.errors.push(format!(
                                    "{display_file}:{line}: span `{}` declared on an impl block requires `entry`; an impl block has no def path to default to. Write span(id = \"{}\", entry = self::Type::method)",
                                    declaration.id, declaration.id
                                ));
                                continue;
                            };
                            impl_spans.insert(declaration.id.clone());
                            declared.push(declaration.id.clone());
                            self.span_declarations.push(ScannedSpan {
                                id: declaration.id.clone(),
                                parent: declaration.parent.clone(),
                                entry,
                                claims: declaration.claims.clone(),
                                module_path: module_path.to_vec(),
                                container: Some(format!("{display_file}:{line}")),
                            });
                        }
                        Directive::Claims(_) => self.errors.push(format!(
                            "{display_file}:{line}: {}",
                            impl_cannot_own_claims()
                        )),
                    }
                }
                let Some((owner, alias_owner)) = impl_owner_paths(value, module_path) else {
                    return;
                };
                let mut seeded = BTreeSet::new();
                for impl_item in &value.items {
                    if let ImplItem::Fn(method) = impl_item {
                        let method_name = method.sig.ident.to_string();
                        let path = format!("{owner}::{method_name}");
                        seeded.insert(path.clone());
                        self.record(
                            &method.attrs,
                            ItemIdentity {
                                path,
                                alias: alias_owner
                                    .as_ref()
                                    .map(|owner| format!("{owner}::{method_name}")),
                            },
                            method.span(),
                            || method.sig.to_token_stream().to_string(),
                            ItemShape::Function {
                                test: has_attribute(&method.attrs, "test"),
                            },
                            (display_file, module_path, &impl_spans),
                        );
                    }
                }
                for id in declared {
                    self.container_members
                        .entry(id)
                        .or_default()
                        .extend(seeded.iter().cloned());
                }
            }
            _ => {}
        }
    }

    /// Adds one item's source metadata and engineering directives to this file scan.
    ///
    /// Malformed attributes are appended to [`errors`] without aborting the file.
    ///
    /// [`errors`]: crate::build::FileScan::errors
    #[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Interface(
                "Recorded items retain their complete syntax span, cleaned signature, module path, shape, and parsed engineering directives." as complete_scanned_item,
            ),
            Failure(
                "Each malformed engineering attribute adds a file-and-line diagnostic without aborting the rest of the file scan." as local_attribute_diagnostic,
            ),
            Invariants(
                "Container, module-wide, and item-level memberships combine recursively onto each recorded item." as combined_memberships,
            ),
        ),
    )
    )]
    fn record(
        &mut self,
        attrs: &[Attribute],
        identity: ItemIdentity,
        span: TokenSpan,
        signature: impl FnOnce() -> String,
        shape: ItemShape,
        location: (&Arc<str>, &[String], &BTreeSet<String>),
    ) -> BTreeSet<String> {
        let (file, module_path, inherited_spans) = location;
        let annotations = parse_annotations(attrs, file, span.start().line, &mut self.errors);
        let direct_spans = distributed_spans(&annotations);
        let source = Some(ScannedSource {
            file: Arc::clone(file),
            start: SourcePosition {
                line: span.start().line,
                column: span.start().column,
            },
            end: SourcePosition {
                line: span.end().line,
                column: span.end().column,
            },
            signature: clean_signature(signature()),
            module_path: module_path.to_vec(),
        });
        self.items.push((
            identity.path,
            identity.alias,
            ScannedItem {
                source,
                shape,
                annotations,
                inherited_spans: inherited_spans.clone(),
            },
        ));
        direct_spans
    }
}

#[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Failure(
                "Each malformed engineering attribute adds a file-and-line diagnostic and contributes no directives." as attribute_diagnostic,
            ),
        ),
        evidence(
            attribute_diagnostic(Test = crate::build::tests::malformed_attributes_report_file_and_line),
        ),
    )
)]
/// Parses every `specdrs` attribute on one item or container.
fn parse_annotations(
    attrs: &[Attribute],
    file: &Arc<str>,
    line: usize,
    errors: &mut Vec<String>,
) -> Vec<SpecdrsArgs> {
    let mut annotations = Vec::new();
    for attr in attrs.iter().filter(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|part| part.ident == "specdrs")
    }) {
        let syn::Meta::List(list) = &attr.meta else {
            errors.push(format!("{file}:{line}: {}", specdrs_requires_arguments()));
            continue;
        };
        match syn::parse2(list.tokens.clone()) {
            Ok(args) => annotations.push(args),
            Err(error) => errors.push(format!("{file}:{line}: invalid specdrs attribute: {error}")),
        }
    }
    annotations
}

#[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Interface(
                "Both joined and declared span identifiers are distributed, so a span declared on a container covers the items inside it." as declared_spans_distribute,
                "A span identifier repeated across attributes collapses to one membership." as repeated_ids_collapse,
            ),
        ),
        evidence(
            declared_spans_distribute(Test = crate::build::tests::containers_distribute_declared_and_joined_spans),
        ),
    )
)]
/// Collects the span identifiers one item's attributes pass down to a container's items.
fn distributed_spans(annotations: &[SpecdrsArgs]) -> BTreeSet<String> {
    let mut spans = BTreeSet::new();
    for directive in annotations
        .iter()
        .flat_map(|annotation| &annotation.directives)
    {
        match directive {
            Directive::InSpans(ids) => spans.extend(ids.iter().cloned()),
            Directive::Span(value) => {
                spans.insert(value.id.clone());
            }
            Directive::Claims(_) => {}
        }
    }
    spans
}

#[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Interface(
                "A free-standing span declaration carries the module path of its own scope." as free_span_module_path,
                "A host-free declaration groups members that no single container encloses, which is the shape a cross-boundary grouping takes." as cross_boundary_grouping,
            ),
            Failure(
                "A declaration with invalid syntax or no entry adds a file-and-line diagnostic and contributes no span." as span_macro_diagnostic,
            ),
            Invariants(
                "A macro declaration contributes no Rust item of its own, so nothing synthetic enters the item index." as no_synthetic_item,
            ),
        ),
        evidence(
            free_span_module_path(Test = crate::build::tests::span_macro_carries_its_module_path),
            span_macro_diagnostic(Test = crate::build::tests::span_macro_requires_an_entry),
        ),
    )
)]
/// Parses spans declared by [`specdrs_span!`] invocations.
///
/// [`specdrs_span!`]: crate::specdrs_span
fn module_span_declarations(
    items: &[SynItem],
    file: &Arc<str>,
    module_path: &[String],
    errors: &mut Vec<String>,
) -> Vec<ScannedSpan> {
    let mut declarations = Vec::new();
    for item in items {
        let SynItem::Macro(item) = item else {
            continue;
        };
        if !item
            .mac
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "specdrs_span")
        {
            continue;
        }
        let line = item.span().start().line;
        let span: SpanArgs = match syn::parse2(item.mac.tokens.clone()) {
            Ok(span) => span,
            Err(error) => {
                errors.push(format!(
                    "{file}:{line}: invalid specdrs_span declaration: {error}"
                ));
                continue;
            }
        };
        let Some(entry) = span.entry else {
            errors.push(format!("{file}:{line}: {}", specdrs_span_requires_entry()));
            continue;
        };
        declarations.push(ScannedSpan {
            id: span.id,
            parent: span.parent,
            entry,
            claims: span.claims,
            module_path: module_path.to_vec(),
            container: None,
        });
    }
    declarations
}

#[specdrs(
    in_spans("knowledge-map-build.source-scanning"),
    claims(
        Constraints(
            Interface(
                "A module-wide declaration applies to the containing module and every inline or file-backed descendant, and nested declarations append to it." as module_wide_memberships,
            ),
            Failure(
                "A module-wide declaration carrying anything other than `in_spans` adds a file-and-line diagnostic." as memberships_only,
            ),
        ),
    )
)]
/// Parses module-wide memberships declared by [`specdrs_module!`] invocations.
///
/// [`specdrs_module!`]: crate::specdrs_module
fn module_memberships(
    items: &[SynItem],
    file: &Arc<str>,
    errors: &mut Vec<String>,
) -> BTreeSet<String> {
    let mut spans = BTreeSet::new();
    for item in items {
        let SynItem::Macro(item) = item else {
            continue;
        };
        if !item
            .mac
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "specdrs_module")
        {
            continue;
        }
        let args: SpecdrsArgs = match syn::parse2(item.mac.tokens.clone()) {
            Ok(args) => args,
            Err(error) => {
                errors.push(format!(
                    "{}:{}: invalid specdrs_module declaration: {error}",
                    file,
                    item.span().start().line
                ));
                continue;
            }
        };
        if args.directives.is_empty()
            || args
                .directives
                .iter()
                .any(|directive| !matches!(directive, Directive::InSpans(_)))
        {
            errors.push(format!(
                "{}:{}: {}",
                file,
                item.span().start().line,
                specdrs_module_requires_in_spans()
            ));
            continue;
        }
        for directive in args.directives {
            let Directive::InSpans(ids) = directive else {
                unreachable!("module directives were validated as memberships");
            };
            spans.extend(ids);
        }
    }
    spans
}

/// Builds a fully qualified item path from a module path and item name.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn item_path(module_path: &[String], name: &str) -> String {
    let capacity = module_path.iter().map(String::len).sum::<usize>()
        + module_path.len().saturating_mul(2)
        + name.len();
    let mut path = String::with_capacity(capacity);
    for module in module_path {
        if !path.is_empty() {
            path.push_str("::");
        }
        path.push_str(module);
    }
    if !path.is_empty() {
        path.push_str("::");
    }
    path.push_str(name);
    path
}

/// Normalizes token-stream spacing in a rendered Rust signature.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn clean_signature(mut signature: String) -> String {
    for (from, to) in [
        (" :: ", "::"),
        (" < ", "<"),
        (" >", ">"),
        (" (", "("),
        (" )", ")"),
        (" [", "["),
        (" ]", "]"),
        (" ,", ","),
        (" :", ":"),
        ("& ", "&"),
    ] {
        signature = signature.replace(from, to);
    }
    signature
}

/// Returns whether an attribute list contains the requested final path segment.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|part| part.ident == name)
    })
}

/// Derives the canonical owner path and optional inherent alias for an implementation.
#[specdrs(
    span(
        id = "knowledge-map-build.item-identity",
        parent = "knowledge-map-build",
        claims(
            Objectives(
                Job("Give distinct parsed Rust items distinct identities." as purpose),
            ),
            Constraints(
                Interface(
                    "A trait method identity contains its self type, trait, trait arguments, and method name." as trait_qualified_identity,
                ),
                Invariants(
                    "Separate trait implementations do not collapse onto the same item identity." as distinct_impls_are_distinct,
                    "Moving a file without changing Rust module or item names preserves item identities." as file_move_stability,
                ),
            ),
            NotApplicable(
                Effects = "Identity construction changes no state outside map construction.",
                State = "Item identities are derived without retained state.",
            ),
            evidence(
                trait_qualified_identity(
                    Test = crate::build::tests::trait_impl_paths_include_the_trait_and_its_arguments,
                ),
                distinct_impls_are_distinct(
                    Test = crate::build::tests::trait_impl_paths_include_the_trait_and_its_arguments,
                ),
            ),
        )
    )
)]
fn impl_owner_paths(
    item: &syn::ItemImpl,
    module_path: &[String],
) -> Option<(String, Option<String>)> {
    let self_type = normalize_type(&item.self_ty, module_path);
    if self_type.is_empty() {
        return None;
    }
    let Some((_, trait_path, _)) = &item.trait_ else {
        return Some((self_type, None));
    };
    let trait_path = normalize_path(trait_path, module_path, false);
    Some((format!("<{self_type} as {trait_path}>"), Some(self_type)))
}

/// Normalizes a Rust type for use in an item identity.
#[specdrs(in_spans("knowledge-map-build.item-identity"))]
fn normalize_type(ty: &Type, module_path: &[String]) -> String {
    match ty {
        Type::Path(path) if path.qself.is_none() => normalize_path(&path.path, module_path, true),
        _ => clean_signature(ty.to_token_stream().to_string()),
    }
}

/// Normalizes a Rust path relative to its declaring module.
#[specdrs(in_spans(
    "knowledge-map-build.item-identity",
    "knowledge-map-build.evidence-resolution"
))]
fn normalize_path(path: &syn::Path, module_path: &[String], qualify_bare: bool) -> String {
    let rendered: Vec<String> = path
        .segments
        .iter()
        .map(|segment| clean_signature(segment.to_token_stream().to_string()))
        .collect();
    let idents: Vec<String> = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let Some(first) = idents.first().map(String::as_str) else {
        return String::new();
    };
    if path.leading_colon.is_some() {
        return rendered.join("::");
    }

    let (prefix, skip) = match first {
        "crate" => (&module_path[..1], 1),
        "self" => (module_path, 1),
        "super" => {
            let count = idents.iter().take_while(|part| *part == "super").count();
            (
                &module_path[..module_path.len().saturating_sub(count)],
                count,
            )
        }
        value if value == module_path[0] => (&[][..], 0),
        _ if qualify_bare => (module_path, 0),
        _ => (&[][..], 0),
    };
    prefix
        .iter()
        .map(String::as_str)
        .chain(rendered.iter().skip(skip).map(String::as_str))
        .collect::<Vec<_>>()
        .join("::")
}

/// Resolves an out-of-line module to its explicit or conventional source file.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn module_file(module: &syn::ItemMod, source_file: &Path, module_dir: &Path) -> Option<PathBuf> {
    if let Some(path) = path_attribute(&module.attrs) {
        return source_file.parent().map(|parent| parent.join(path));
    }
    let name = module.ident.to_string();
    let flat = module_dir.join(format!("{name}.rs"));
    if flat.exists() {
        return Some(flat);
    }
    let nested = module_dir.join(name).join("mod.rs");
    nested.exists().then_some(nested)
}

/// Returns the string value of a Rust `#[path = "..."]` attribute.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        let syn::Meta::NameValue(value) = &attr.meta else {
            return None;
        };
        let syn::Expr::Lit(value) = &value.value else {
            return None;
        };
        let syn::Lit::Str(value) = &value.lit else {
            return None;
        };
        Some(value.value())
    })
}

/// Renders a source path relative to the package manifest directory.
#[specdrs(in_spans("knowledge-map-build.source-scanning"))]
fn display_path(path: &Path, manifest_dir: &Path) -> String {
    path.strip_prefix(manifest_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Default)]
#[specdrs(in_spans("knowledge-map-build.map-assembly"))]
/// Accumulates declarations and members for one span.
struct SpanDraft {
    declarations: Vec<SpanDeclaration>,
    members: BTreeSet<String>,
}

#[specdrs(in_spans("knowledge-map-build.map-assembly"))]
/// Contains one parsed span declaration and its claim context.
struct SpanDeclaration {
    parent: Option<String>,
    entry: String,
    claims: Option<(ClaimsArgs, Vec<String>)>,
    /// Locates the container that declared this span, when one did.
    container: Option<String>,
}

#[derive(Default)]
#[specdrs(in_spans("knowledge-map-build.map-assembly"))]
/// Accumulates memberships and claim blocks for one item.
struct ItemDraft {
    spans: BTreeSet<String>,
    claims: Vec<(ClaimsArgs, Vec<String>)>,
}

/// Owns canonical scan output while assembling a knowledge map.
struct MapAssembler {
    crate_name: String,
    scanned: BTreeMap<String, ScannedItem>,
    aliases: BTreeMap<String, Vec<String>>,
    span_declarations: Vec<ScannedSpan>,
    container_members: BTreeMap<String, BTreeSet<String>>,
}

impl MapAssembler {
    /// Creates an assembler from canonical scan output.
    fn new(
        crate_name: String,
        scanned: BTreeMap<String, ScannedItem>,
        aliases: BTreeMap<String, Vec<String>>,
        span_declarations: Vec<ScannedSpan>,
        container_members: BTreeMap<String, BTreeSet<String>>,
    ) -> Self {
        Self {
            crate_name,
            scanned,
            aliases,
            span_declarations,
            container_members,
        }
    }

    /// Validates scanned directives and assembles a complete knowledge map.
    ///
    /// # Errors
    ///
    /// Returns all span, claim, evidence, and source validation diagnostics together.
    #[specdrs(
    span(
        id = "knowledge-map-build.map-assembly",
        parent = "knowledge-map-build",
        claims(
            Objectives(
                Job("Assemble scanned directives into a validated schema 2 knowledge map." as purpose),
            ),
            Constraints(
                Interface(
                    "Every emitted item is keyed by its def path and carries its signature, the span memberships it holds directly or by container and module inheritance, all twelve axes, and its complete source range." as emitted_item_contract,
                ),
                Invariants(
                    "Each span has one declaration, one resolvable entry, existing parents, and an acyclic parent chain." as valid_span_graph,
                    "Each item owns at most one claims block and every output axis has an explicit status." as valid_item_claims,
                    "Map collections and diagnostics use deterministic ordering." as deterministic_assembly,
                ),
                Failure(
                    "All assembly errors are sorted, deduplicated, and returned together." as aggregate_errors,
                ),
            ),
            NotApplicable(
                Effects = "Assembly only creates the returned map.",
            ),
            evidence(
                emitted_item_contract(Test = crate::build::tests::builds_own_knowledge_map),
                valid_span_graph(Test = crate::build::tests::builds_own_knowledge_map),
                valid_item_claims(Test = crate::build::tests::all_axes_are_emitted),
                deterministic_assembly(Test = crate::build::tests::own_map_is_deterministic),
                aggregate_errors(Test = crate::build::tests::own_map_is_deterministic),
            ),
        )
    ),
    claims(
        Constraints(
            Interface(
                "Every scanned Rust item is emitted, so the item index records what the crate contains and span membership is recorded per item rather than gating publication." as complete_item_index,
                "A span declaration without an explicit entry takes the def path of the item hosting it." as host_entry_default,
            ),
            Invariants(
                "Span-owned and item-owned claims remain in distinct axis maps during assembly." as preserves_claim_scope,
                "No knowledge map is returned while any collected assembly diagnostic remains." as errors_block_output,
                "Every resolved evidence binder names an emitted item." as evidence_binders_resolve,
            ),
        ),
        evidence(
            complete_item_index(Test = crate::build::tests::builds_own_knowledge_map),
            host_entry_default(Test = crate::build::tests::builds_own_knowledge_map),
            evidence_binders_resolve(Test = crate::build::tests::builds_own_knowledge_map),
        ),
    )
    )]
    fn assemble(self) -> Result<KnowledgeMap, BuildError> {
        let Self {
            crate_name,
            scanned,
            aliases,
            span_declarations,
            container_members,
        } = self;
        let mut spans: BTreeMap<String, SpanDraft> = BTreeMap::new();
        let mut item_drafts: BTreeMap<String, ItemDraft> = BTreeMap::new();
        let mut errors = Vec::new();

        for (def_path, item) in &scanned {
            let source = item
                .source
                .as_ref()
                .expect("scanned items retain source information");
            item_drafts.entry(def_path.clone()).or_default();
            for id in &item.inherited_spans {
                item_drafts
                    .entry(def_path.clone())
                    .or_default()
                    .spans
                    .insert(id.clone());
                spans
                    .entry(id.clone())
                    .or_default()
                    .members
                    .insert(def_path.clone());
            }
            for annotation in &item.annotations {
                item_drafts.entry(def_path.clone()).or_default();
                for directive in &annotation.directives {
                    match directive {
                        Directive::Span(value) => {
                            let normalized = value.entry.as_deref().map_or_else(
                                || def_path.clone(),
                                |entry| normalize_binder(entry, &source.module_path, &crate_name),
                            );
                            let entry = resolve_item_reference(
                                &normalized,
                                &scanned,
                                &aliases,
                                &value.id,
                                &mut errors,
                            );
                            record_span_declaration(
                                &value.id,
                                SpanDeclaration {
                                    parent: value.parent.clone(),
                                    entry,
                                    claims: value
                                        .claims
                                        .clone()
                                        .map(|claims| (claims, source.module_path.clone())),
                                    container: None,
                                },
                                &mut spans,
                                &mut item_drafts,
                                &scanned,
                            );
                        }
                        Directive::InSpans(ids) => {
                            for id in ids {
                                item_drafts
                                    .entry(def_path.clone())
                                    .or_default()
                                    .spans
                                    .insert(id.clone());
                                spans
                                    .entry(id.clone())
                                    .or_default()
                                    .members
                                    .insert(def_path.clone());
                            }
                        }
                        Directive::Claims(claims) => item_drafts
                            .entry(def_path.clone())
                            .or_default()
                            .claims
                            .push((claims.clone(), source.module_path.clone())),
                    }
                }
            }
        }

        for declaration in span_declarations {
            let normalized =
                normalize_binder(&declaration.entry, &declaration.module_path, &crate_name);
            let entry = resolve_item_reference(
                &normalized,
                &scanned,
                &aliases,
                &declaration.id,
                &mut errors,
            );
            record_span_declaration(
                &declaration.id,
                SpanDeclaration {
                    parent: declaration.parent,
                    entry,
                    claims: declaration
                        .claims
                        .map(|claims| (claims, declaration.module_path)),
                    container: declaration.container,
                },
                &mut spans,
                &mut item_drafts,
                &scanned,
            );
        }

        validate_spans(&spans, &mut errors);
        validate_parent_cycles(&spans, &mut errors);
        validate_container_spans(&spans, &container_members, &mut errors);
        for (path, draft) in &item_drafts {
            if draft.claims.len() > 1 {
                errors.push(format!(
                    "item:{path} has more than one claims block. Put every claim for this item in a single claims(...) directive. Stacked #[specdrs] attributes may add span(...) and in_spans(...), but only one claims(...) block"
                ));
            }
        }

        let mut span_models = Vec::new();
        for (id, mut draft) in spans {
            if draft.declarations.len() != 1 {
                continue;
            }
            let declaration = draft.declarations.remove(0);
            let (claims, not_applicable) = declaration.claims.map_or_else(
                || (Vec::new(), Vec::new()),
                |(claims, module_path)| split_claims(claims, module_path),
            );
            let axes = build_axes(
                &format!("span:{id}"),
                claims,
                not_applicable,
                &crate_name,
                &scanned,
                &aliases,
                &mut errors,
            );
            span_models.push(Span {
                id,
                parent: declaration.parent,
                entry: declaration.entry,
                members: draft.members.into_iter().collect(),
                axes,
            });
        }

        let mut item_models = BTreeMap::new();
        for (def_path, mut draft) in item_drafts {
            let source = scanned[&def_path]
                .source
                .as_ref()
                .expect("scanned items retain source information");
            let (claims, not_applicable) = draft.claims.pop().map_or_else(
                || (Vec::new(), Vec::new()),
                |(claims, module_path)| split_claims(claims, module_path),
            );
            let axes = build_axes(
                &format!("item:{def_path}"),
                claims,
                not_applicable,
                &crate_name,
                &scanned,
                &aliases,
                &mut errors,
            );
            item_models.insert(
                def_path,
                Item {
                    source: SourceRange {
                        file: source.file.to_string(),
                        start: source.start,
                        end: source.end,
                    },
                    signature: source.signature.clone(),
                    spans: draft.spans.into_iter().collect(),
                    axes,
                },
            );
        }

        if errors.is_empty() {
            Ok(KnowledgeMap {
                schema: 2,
                crate_name,
                spans: span_models,
                items: item_models,
            })
        } else {
            errors.sort();
            errors.dedup();
            Err(BuildError::many(errors))
        }
    }
}

/// Separates claims from not-applicable declarations while retaining module context.
#[specdrs(in_spans("knowledge-map-build.map-assembly"))]
fn split_claims(
    claims: ClaimsArgs,
    module_path: Vec<String>,
) -> (Vec<(ClaimArgs, Vec<String>)>, Vec<NotApplicableArgs>) {
    (
        claims
            .claims
            .into_iter()
            .map(|claim| (claim, module_path.clone()))
            .collect(),
        claims.not_applicable,
    )
}

/// Resolves a span entry against canonical item paths and aliases.
#[specdrs(in_spans("knowledge-map-build.map-assembly"))]
fn resolve_item_reference(
    path: &str,
    scanned: &BTreeMap<String, ScannedItem>,
    aliases: &BTreeMap<String, Vec<String>>,
    span_id: &str,
    errors: &mut Vec<String>,
) -> String {
    if scanned.contains_key(path) {
        return path.to_owned();
    }
    match aliases.get(path).map(Vec::as_slice) {
        Some([candidate]) => candidate.clone(),
        Some(candidates) => {
            errors.push(format!(
                "span `{span_id}` entry `{path}` is ambiguous; candidates: {}. Point entry at one fully qualified item path",
                candidates.join(", ")
            ));
            path.to_owned()
        }
        _ => {
            errors.push(format!(
                "span `{span_id}` has missing entry `{path}`. entry must name a scanned Rust item: a function, type, trait, module, const, static, or impl method. Point it at an existing item, or add that item"
            ));
            path.to_owned()
        }
    }
}

#[specdrs(
    in_spans("knowledge-map-build.map-assembly"),
    claims(
        Constraints(
            Invariants(
                "A resolved entry that names a scanned item becomes a member of the span and gains the span membership." as entry_is_member,
                "A declaration whose entry names another item leaves its own host out of the span, unless the host declares the membership itself." as host_is_not_a_member,
            ),
        ),
        evidence(
            entry_is_member(Test = crate::build::tests::builds_own_knowledge_map),
            host_is_not_a_member(Test = crate::build::tests::builds_own_knowledge_map),
        ),
    )
)]
/// Records one resolved span declaration in the span and item drafts.
fn record_span_declaration(
    id: &str,
    declaration: SpanDeclaration,
    spans: &mut BTreeMap<String, SpanDraft>,
    item_drafts: &mut BTreeMap<String, ItemDraft>,
    scanned: &BTreeMap<String, ScannedItem>,
) {
    let entry = declaration.entry.clone();
    let span = spans.entry(id.to_owned()).or_default();
    span.declarations.push(declaration);
    if !scanned.contains_key(&entry) {
        return;
    }
    span.members.insert(entry.clone());
    item_drafts
        .entry(entry)
        .or_default()
        .spans
        .insert(id.to_owned());
}

#[specdrs(
    in_spans("knowledge-map-build.map-assembly"),
    claims(
        Constraints(
            Invariants(
                "A span declared on a container that seeded members accepts no member from outside that container." as containers_are_closed,
            ),
            Failure(
                "Each outside member names the span, the declaring container, and the correction." as outside_member_named,
            ),
        ),
        evidence(
            containers_are_closed(Test = crate::build::tests::container_spans_reject_outside_members),
            outside_member_named(Test = crate::build::tests::container_spans_reject_outside_members),
        ),
    )
)]
/// Rejects members that a container-declared span's container did not contribute.
///
/// A container that seeded no member is a documentation host rather than a grouping,
/// so its span stays open.
fn validate_container_spans(
    spans: &BTreeMap<String, SpanDraft>,
    container_members: &BTreeMap<String, BTreeSet<String>>,
    errors: &mut Vec<String>,
) {
    for (id, draft) in spans {
        let Some(seeded) = container_members
            .get(id)
            .filter(|seeded| !seeded.is_empty())
        else {
            continue;
        };
        let container = draft
            .declarations
            .first()
            .and_then(|declaration| declaration.container.as_deref())
            .unwrap_or("its declaring container");
        let entry = draft.declarations.first().map(|value| value.entry.as_str());
        for member in draft
            .members
            .iter()
            .filter(|member| Some(member.as_str()) != entry && !seeded.contains(*member))
        {
            errors.push(format!(
                "span `{id}` is declared on the container at {container}, but `{member}` joins it from outside. A container-declared span only covers items inside that container. For a cross-boundary grouping, declare the span with specdrs_span! and join it from both sides with in_spans, or give this span a parent and join the parent instead"
            ));
        }
    }
}

/// Validates span identifiers, declaration counts, and parent references.
#[specdrs(
    in_spans("knowledge-map-build.map-assembly"),
    claims(
        Constraints(
            Invariants(
                "Every referenced parent exists and every span has exactly one declaration." as declarations_and_parents,
            ),
        ),
        evidence(
            declarations_and_parents(Test = crate::build::tests::builds_own_knowledge_map),
        ),
    )
)]
fn validate_spans(spans: &BTreeMap<String, SpanDraft>, errors: &mut Vec<String>) {
    for (id, span) in spans {
        if id.trim().is_empty() {
            errors.push(
                "span id must not be empty. Write span(id = \"checkout\", ...) or specdrs_span!(id = \"checkout\", entry = ...)"
                    .to_owned(),
            );
        }
        if span.declarations.len() != 1 {
            errors.push(format!(
                "span `{id}` requires exactly one declaration; found {}. Declare a span once with span(...) or specdrs_span!. Other items join it with in_spans(...) or specdrs_module!(in_spans(...))",
                span.declarations.len()
            ));
            continue;
        }
        if let Some(parent) = span.declarations[0].parent.as_ref() {
            if parent.trim().is_empty() {
                errors.push(format!(
                    "span `{id}` has an empty parent id. Write parent = \"payments\" or omit parent"
                ));
            } else if parent == id {
                errors.push(format!(
                    "span `{id}` cannot be its own parent. Point parent at an enclosing span, or omit parent for a root span"
                ));
            } else if !spans.contains_key(parent) {
                errors.push(format!(
                    "span `{id}` has missing parent `{parent}`. Declare `{parent}` with span(...) or specdrs_span! before referencing it, or drop the parent field"
                ));
            }
        }
    }
}

/// Detects cycles in the span parent graph.
#[specdrs(
    in_spans("knowledge-map-build.map-assembly"),
    claims(
        Constraints(
            Invariants(
                "Following parent pointers from any span terminates without revisiting an ID." as acyclic_parent_chain,
            ),
        ),
    )
)]
fn validate_parent_cycles(spans: &BTreeMap<String, SpanDraft>, errors: &mut Vec<String>) {
    for start in spans.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(start.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                errors.push(format!(
                    "span parent cycle includes `{id}`. parent must name an ancestor, not a descendant or sibling that points back. Break the cycle by pointing one span at a true parent or dropping the parent field"
                ));
                break;
            }
            current = spans.get(id).and_then(|span| {
                (span.declarations.len() == 1)
                    .then(|| span.declarations[0].parent.as_deref())
                    .flatten()
            });
        }
    }
    errors.sort();
    errors.dedup();
}

/// Builds a complete axis map from claims and not-applicable declarations.
#[specdrs(
    in_spans("knowledge-map-build.map-assembly"),
    claims(
        Constraints(
            Invariants(
                "Every one of the twelve axes is emitted as specified, not applicable, or unspecified." as complete_axis_status,
            ),
            Failure(
                "An axis cannot contain claims while marked not applicable." as exclusive_axis_status,
                "A not-applicable axis declared without a reason adds a diagnostic." as not_applicable_needs_reason,
            ),
        ),
        evidence(
            complete_axis_status(Test = crate::build::tests::all_axes_are_emitted),
        ),
    )
)]
fn build_axes(
    scope: &str,
    claims: Vec<(ClaimArgs, Vec<String>)>,
    not_applicable: Vec<NotApplicableArgs>,
    crate_name: &str,
    scanned: &BTreeMap<String, ScannedItem>,
    aliases: &BTreeMap<String, Vec<String>>,
    errors: &mut Vec<String>,
) -> BTreeMap<Axis, AxisEntry> {
    let mut axes = Axis::empty_map();
    let mut claim_ids = BTreeSet::new();

    for not_applicable in not_applicable {
        if not_applicable.reason.trim().is_empty() {
            errors.push(format!(
                "{scope} marks {} not applicable without a reason. Write NotApplicable({} = \"why this axis does not apply\")",
                not_applicable.axis,
                not_applicable.axis
            ));
            continue;
        }
        let entry = axes
            .get_mut(&not_applicable.axis)
            .expect("all axes are initialized");
        if entry.status != AxisStatus::Unspecified {
            errors.push(format!(
                "{scope} marks {} not applicable more than once. Keep a single NotApplicable({} = \"reason\") entry",
                not_applicable.axis,
                not_applicable.axis
            ));
            continue;
        }
        entry.status = AxisStatus::NotApplicable;
        entry.reason = Some(not_applicable.reason);
    }

    for (claim, module_path) in claims {
        if claim.id.trim().is_empty() {
            errors.push(format!(
                "{scope} has an empty claim id. Write \"A proposition.\" as alias"
            ));
            continue;
        }
        if claim.text.trim().is_empty() {
            errors.push(format!(
                "{scope} claim `{}` has empty text. Write a falsifiable sentence before `as {}`",
                claim.id, claim.id
            ));
            continue;
        }
        if !claim_ids.insert(claim.id.clone()) {
            errors.push(format!(
                "{scope} has duplicate claim id `{}`. Give each claim its own alias inside this owner",
                claim.id
            ));
            continue;
        }
        let entry = axes.get_mut(&claim.axis).expect("all axes are initialized");
        if entry.status == AxisStatus::NotApplicable {
            errors.push(format!(
                "{scope} has a {} claim but marks that axis not applicable. Either keep the claim and drop NotApplicable({}), or keep NotApplicable and move the claim off that axis",
                claim.axis, claim.axis
            ));
            continue;
        }
        let evidence = claim
            .evidence
            .into_iter()
            .map(|evidence| {
                EvidenceContext {
                    crate_name,
                    scanned,
                    aliases,
                    scope,
                    claim_id: &claim.id,
                }
                .resolve(evidence, &module_path, errors)
            })
            .collect();
        entry.status = AxisStatus::Specified;
        entry.claims.push(Claim {
            id: claim.id,
            kind: claim.kind,
            text: claim.text,
            evidence,
        });
    }
    axes
}

#[specdrs(in_spans("knowledge-map-build.evidence-resolution"))]
/// Provides the indexes and owner context needed to resolve evidence.
struct EvidenceContext<'a> {
    crate_name: &'a str,
    scanned: &'a BTreeMap<String, ScannedItem>,
    aliases: &'a BTreeMap<String, Vec<String>>,
    scope: &'a str,
    claim_id: &'a str,
}

impl EvidenceContext<'_> {
    /// Resolves one evidence declaration to a canonical artifact and status.
    #[specdrs(
    span(
        id = "knowledge-map-build.evidence-resolution",
        parent = "knowledge-map-build",
        claims(
            Objectives(
                Job("Link claim evidence to one inspectable Rust artifact." as purpose),
            ),
            Constraints(
                Interface(
                    "A short evidence binder resolves only when it identifies one item." as unique_short_binder,
                ),
                Invariants(
                    "Resolved evidence retains a fully qualified binder and a result compatible with its evidence kind." as qualified_typed_evidence,
                ),
                Failure(
                    "An ambiguous short evidence binder fails and reports its qualified candidates." as ambiguous_binder_fails,
                ),
            ),
            NotApplicable(
                Effects = "Evidence resolution reads the scanned item index without running evidence.",
            ),
            evidence(
                unique_short_binder(Test = crate::build::tests::ambiguous_short_evidence_binders_report_qualified_candidates),
                qualified_typed_evidence(
                    Test = crate::build::tests::normalizes_relative_binders,
                    Test = crate::attribute::tests::parses_qualified_evidence_binders,
                ),
                ambiguous_binder_fails(Test = crate::build::tests::ambiguous_short_evidence_binders_report_qualified_candidates),
            ),
        )
    ),
    claims(
        Constraints(
            Interface(
                "Resolution returns a normalized binder and linked or unavailable status for exactly one evidence declaration." as one_evidence_result,
            ),
            Failure(
                "Zero compatible targets return unavailable and multiple targets append an ambiguity diagnostic." as candidate_cardinality,
            ),
        ),
    )
    )]
    fn resolve(
        &self,
        raw: crate::attribute::EvidenceArgs,
        module_path: &[String],
        errors: &mut Vec<String>,
    ) -> Evidence {
        let binder = normalize_binder(&raw.binder, module_path, self.crate_name);
        let mut candidates = self.aliases.get(&binder).cloned().unwrap_or_default();
        if self.scanned.contains_key(&binder) {
            candidates.push(binder.clone());
        }
        candidates.sort();
        candidates.dedup();
        let (binder, result) = match candidates.as_slice() {
            [] => (binder, EvidenceResult::Unavailable),
            [candidate] => {
                let result = self
                    .scanned
                    .get(candidate)
                    .filter(|item| item.shape.supports(raw.kind))
                    .map_or(EvidenceResult::Unavailable, |_| EvidenceResult::Linked);
                (candidate.clone(), result)
            }
            _ => {
                errors.push(format!(
                    "{} claim `{}` evidence binder `{binder}` is ambiguous; candidates: {}. Point the binder at one fully qualified item",
                    self.scope,
                    self.claim_id,
                    candidates.join(", ")
                ));
                (binder, EvidenceResult::Unavailable)
            }
        };
        Evidence {
            kind: raw.kind,
            binder,
            result,
        }
    }
}

/// Normalizes an evidence binder relative to its declaring module and crate.
#[specdrs(in_spans("knowledge-map-build.evidence-resolution"))]
fn normalize_binder(raw: &str, module_path: &[String], crate_name: &str) -> String {
    let path: syn::TypePath = syn::parse_str(raw).expect("validated evidence binder");
    let Some(qself) = &path.qself else {
        return normalize_path(&path.path, module_path, true);
    };
    let self_type = normalize_type(&qself.ty, module_path);
    let mut trait_path = path.path.clone();
    trait_path.segments = path
        .path
        .segments
        .iter()
        .take(qself.position)
        .cloned()
        .collect();
    let suffix = path
        .path
        .segments
        .iter()
        .skip(qself.position)
        .map(|segment| clean_signature(segment.to_token_stream().to_string()))
        .collect::<Vec<_>>()
        .join("::");
    if qself.as_token.is_none() {
        return format!("<{self_type}>::{suffix}");
    }
    let trait_path = normalize_path(&trait_path, module_path, false).replacen(
        "crate::",
        &format!("{crate_name}::"),
        1,
    );
    format!("<{self_type} as {trait_path}>::{suffix}")
}

#[specdrs(in_spans("knowledge-map-build.evidence-resolution"))]
impl ItemShape {
    /// Returns whether this item shape can satisfy an evidence kind.
    fn supports(self, kind: EvidenceKind) -> bool {
        match kind {
            EvidenceKind::Type => self == Self::Type,
            EvidenceKind::Test => self == Self::Function { test: true },
            EvidenceKind::Fuzz | EvidenceKind::Proof => {
                matches!(self, Self::Function { .. })
            }
            EvidenceKind::Lint => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_manifest() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
    }

    #[test]
    fn builds_own_knowledge_map() {
        let map = BuildOptions {
            manifest_path: own_manifest(),
            package: Some("specdrs".into()),
        }
        .build()
        .expect("the specdrs crate should build its own map");

        let build = map
            .spans
            .iter()
            .find(|span| span.id == "knowledge-map-build")
            .expect("build span should exist");
        assert_eq!(build.entry, "specdrs::build::BuildOptions::build");
        assert_eq!(build.parent.as_deref(), Some("specdrs"));
        assert_eq!(
            build.axes[&Axis::Interface].claims[0].evidence[0].result,
            EvidenceResult::Linked
        );

        let identity = map
            .spans
            .iter()
            .find(|span| span.id == "knowledge-map-build.item-identity")
            .expect("identity span should exist");
        assert_eq!(identity.parent.as_deref(), Some("knowledge-map-build"));
        assert_eq!(identity.entry, "specdrs::build::impl_owner_paths");
        assert_eq!(
            identity.axes[&Axis::Invariants].claims[0].evidence[0].result,
            EvidenceResult::Linked
        );
    }

    #[test]
    fn own_map_is_deterministic() {
        let options = BuildOptions {
            manifest_path: own_manifest(),
            package: Some("specdrs".into()),
        };

        assert_eq!(options.build().unwrap(), options.build().unwrap());
    }

    #[test]
    fn dogfood_map_covers_every_subsystem() {
        let map = BuildOptions {
            manifest_path: own_manifest(),
            package: Some("specdrs".into()),
        }
        .build()
        .unwrap();
        for id in [
            "specdrs",
            "attribute-parsing",
            "knowledge-map-model",
            "knowledge-map-build",
            "knowledge-map-build.source-scanning",
            "knowledge-map-build.map-assembly",
            "knowledge-map-build.item-identity",
            "knowledge-map-build.evidence-resolution",
            "claim-projection",
            "semantic-analysis",
            "command-line-interface",
        ] {
            let span = map
                .spans
                .iter()
                .find(|span| span.id == id)
                .unwrap_or_else(|| panic!("missing dogfood span `{id}`"));
            assert!(!span.members.is_empty(), "span `{id}` has no members");
            assert!(
                span.axes.values().any(|axis| !axis.claims.is_empty()),
                "span `{id}` has no claims"
            );
        }
        for (id, module) in [
            ("attribute-parsing", "specdrs::attribute"),
            ("knowledge-map-model", "specdrs::model"),
            ("claim-projection", "specdrs::projection"),
            ("semantic-analysis", "specdrs::analysis"),
            ("command-line-interface", "specdrs::cli"),
        ] {
            let span = map.spans.iter().find(|span| span.id == id).unwrap();
            assert!(
                span.members.iter().any(|member| member == module),
                "span `{id}` is missing module-wide member `{module}`"
            );
        }
    }

    #[test]
    fn malformed_attributes_report_file_and_line() {
        let file = syn::parse_file(
            r"
            #[specdrs]
            fn missing_arguments() {}
            ",
        )
        .unwrap();
        let SynItem::Fn(item) = &file.items[0] else {
            panic!("the fixture declares a function");
        };
        let mut errors = Vec::new();
        let annotations =
            parse_annotations(&item.attrs, &Arc::<str>::from("src/lib.rs"), 2, &mut errors);

        assert!(annotations.is_empty());
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].starts_with("src/lib.rs:2: specdrs requires arguments"),
            "{}",
            errors[0]
        );
    }

    #[test]
    fn containers_distribute_declared_and_joined_spans() {
        let file = syn::parse_file(
            r#"
            #[specdrs(
                in_spans("audit"),
                span(id = "gateway", entry = self::Gateway::send)
            )]
            impl Gateway {
                fn send(&self) {}
                fn retry(&self) {}
            }
            "#,
        )
        .unwrap();
        let display = Arc::<str>::from("src/lib.rs");
        let mut scan = FileScan::default();
        scan.scan_item(
            &file.items[0],
            Path::new("src/lib.rs"),
            (&display, &["payments".to_owned()]),
            Path::new("src"),
            &[],
            &BTreeSet::new(),
        );

        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        assert_eq!(scan.items.len(), 2);
        for (path, _, item) in &scan.items {
            assert_eq!(
                item.inherited_spans
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["audit", "gateway"],
                "`{path}` should carry both the joined and the declared span"
            );
        }
        assert_eq!(scan.span_declarations.len(), 1);
        let declaration = &scan.span_declarations[0];
        assert_eq!(declaration.id, "gateway");
        assert_eq!(declaration.entry, "self :: Gateway :: send");
        assert_eq!(declaration.module_path, ["payments"]);
        assert!(
            declaration.container.is_some(),
            "an impl-declared span records its container"
        );
        assert_eq!(
            scan.container_members["gateway"]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["payments::Gateway::retry", "payments::Gateway::send"],
            "the block seeds every method it declares"
        );
    }

    #[test]
    fn impl_declarations_reject_missing_entry_and_claims() {
        let file = syn::parse_file(
            r#"
            #[specdrs(span(id = "no-entry"))]
            impl First {
                fn a(&self) {}
            }
            #[specdrs(claims(Constraints(Job("No owner." as no_owner))))]
            impl Second {
                fn b(&self) {}
            }
            "#,
        )
        .unwrap();
        let display = Arc::<str>::from("src/lib.rs");
        let mut scan = FileScan::default();
        for item in &file.items {
            scan.scan_item(
                item,
                Path::new("src/lib.rs"),
                (&display, &["payments".to_owned()]),
                Path::new("src"),
                &[],
                &BTreeSet::new(),
            );
        }

        assert!(
            scan.span_declarations.is_empty(),
            "a declaration without `entry` contributes no span"
        );
        assert_eq!(scan.errors.len(), 2, "{:?}", scan.errors);
        assert!(
            scan.errors[0].starts_with(
                "src/lib.rs:2: span `no-entry` declared on an impl block requires `entry`"
            ),
            "{}",
            scan.errors[0]
        );
        assert!(
            scan.errors[1].starts_with("src/lib.rs:6: an impl block cannot own claims"),
            "{}",
            scan.errors[1]
        );
    }

    #[test]
    fn container_spans_reject_outside_members() {
        let seeded = BTreeMap::from([(
            "gateway".to_owned(),
            BTreeSet::from(["payments::Gateway::send".to_owned()]),
        )]);
        let spans = BTreeMap::from([(
            "gateway".to_owned(),
            SpanDraft {
                declarations: vec![SpanDeclaration {
                    parent: None,
                    entry: "payments::Gateway".to_owned(),
                    claims: None,
                    container: Some("src/lib.rs:2".to_owned()),
                }],
                members: BTreeSet::from([
                    "payments::Gateway".to_owned(),
                    "payments::Gateway::send".to_owned(),
                    "payments::elsewhere".to_owned(),
                ]),
            },
        )]);
        let mut errors = Vec::new();
        validate_container_spans(&spans, &seeded, &mut errors);

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("span `gateway`"));
        assert!(errors[0].contains("src/lib.rs:2"));
        assert!(
            errors[0].contains("`payments::elsewhere` joins it from outside"),
            "{}",
            errors[0]
        );

        let mut open = Vec::new();
        validate_container_spans(&spans, &BTreeMap::new(), &mut open);
        assert!(
            open.is_empty(),
            "a span no container seeded stays open: {open:?}"
        );
    }

    #[test]
    fn span_macro_carries_its_module_path() {
        let file = syn::parse_file(
            r#"
            specdrs_span!(
                id = "ledger",
                parent = "checkout",
                entry = self::capture,
                claims(Constraints(Invariants("Recorded once." as recorded))),
            );
            mod inner {
                specdrs_span!(id = "inner", entry = self::work);
                fn work() {}
            }
            "#,
        )
        .unwrap();
        let display = Arc::<str>::from("src/lib.rs");
        let mut errors = Vec::new();

        let top =
            module_span_declarations(&file.items, &display, &["sample".to_owned()], &mut errors);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(top.len(), 1, "the nested declaration is not a sibling");
        assert_eq!(top[0].id, "ledger");
        assert_eq!(top[0].parent.as_deref(), Some("checkout"));
        assert_eq!(top[0].entry, "self :: capture");
        assert_eq!(top[0].module_path, ["sample"]);

        let mut scan = FileScan::default();
        scan.scan_item(
            &file.items[1],
            Path::new("src/lib.rs"),
            (&display, &["sample".to_owned()]),
            Path::new("src"),
            &[],
            &BTreeSet::new(),
        );
        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        assert_eq!(scan.span_declarations.len(), 1);
        assert_eq!(
            scan.span_declarations[0].module_path,
            ["sample", "inner"],
            "an inline module declaration resolves against the child module path"
        );
    }

    #[test]
    fn span_macro_requires_an_entry() {
        let file = syn::parse_file(
            r#"
            specdrs_span!(id = "no-entry");
            specdrs_span!(id = "bad-syntax", entry = self::work, nonsense = 1);
            "#,
        )
        .unwrap();
        let mut errors = Vec::new();
        let declarations = module_span_declarations(
            &file.items,
            &Arc::<str>::from("src/lib.rs"),
            &["sample".to_owned()],
            &mut errors,
        );

        assert!(
            declarations.is_empty(),
            "neither declaration should contribute a span"
        );
        assert_eq!(errors.len(), 2);
        assert!(
            errors[0].contains("src/lib.rs:2: specdrs_span! requires `entry`"),
            "{}",
            errors[0]
        );
        assert!(
            errors[1].contains("src/lib.rs:3: invalid specdrs_span declaration"),
            "{}",
            errors[1]
        );
    }

    #[test]
    fn normalizes_relative_binders() {
        let module = vec!["payments".into(), "charge".into()];

        assert_eq!(
            normalize_binder("crate::tests::order", &module, "payments"),
            "payments::tests::order"
        );
        assert_eq!(
            normalize_binder("self::local_test", &module, "payments"),
            "payments::charge::local_test"
        );
        assert_eq!(
            normalize_binder("super::tests::order", &module, "payments"),
            "payments::tests::order"
        );
    }

    #[test]
    fn all_axes_are_emitted() {
        assert_eq!(Axis::empty_map().len(), 12);
    }

    #[test]
    fn inherent_impl_paths_follow_the_implemented_type() {
        let module = vec!["payments".into(), "adapters".into()];
        let item: syn::ItemImpl = syn::parse_str("impl crate::checkout::Charge {}").unwrap();

        assert_eq!(
            impl_owner_paths(&item, &module).unwrap(),
            ("payments::checkout::Charge".into(), None)
        );
    }

    #[test]
    fn trait_impl_paths_include_the_trait_and_its_arguments() {
        let module = vec!["payments".into(), "adapters".into()];
        let item: syn::ItemImpl = syn::parse_str("impl From<Request> for Receipt {}").unwrap();

        assert_eq!(
            impl_owner_paths(&item, &module).unwrap(),
            (
                "<payments::adapters::Receipt as From<Request>>".into(),
                Some("payments::adapters::Receipt".into())
            )
        );
        assert_eq!(
            normalize_binder("<Receipt as From<Request>>::from", &module, "payments"),
            "<payments::adapters::Receipt as From<Request>>::from"
        );
    }

    #[test]
    fn ambiguous_short_evidence_binders_report_qualified_candidates() {
        let first = "<payments::Subject as First>::shared".to_owned();
        let second = "<payments::Subject as Second>::shared".to_owned();
        let scanned = BTreeMap::from([
            (
                first.clone(),
                ScannedItem {
                    source: None,
                    shape: ItemShape::Function { test: true },
                    annotations: Vec::new(),
                    inherited_spans: BTreeSet::new(),
                },
            ),
            (
                second.clone(),
                ScannedItem {
                    source: None,
                    shape: ItemShape::Function { test: true },
                    annotations: Vec::new(),
                    inherited_spans: BTreeSet::new(),
                },
            ),
        ]);
        let aliases =
            BTreeMap::from([("payments::Subject::shared".to_owned(), vec![first, second])]);
        let mut errors = Vec::new();

        let evidence = EvidenceContext {
            crate_name: "payments",
            scanned: &scanned,
            aliases: &aliases,
            scope: "item:payments::charge",
            claim_id: "evidence",
        }
        .resolve(
            crate::attribute::EvidenceArgs {
                kind: EvidenceKind::Test,
                binder: "Subject::shared".into(),
            },
            &["payments".into()],
            &mut errors,
        );

        assert_eq!(evidence.result, EvidenceResult::Unavailable);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("is ambiguous"));
        assert!(errors[0].contains("<payments::Subject as First>::shared"));
        assert!(errors[0].contains("<payments::Subject as Second>::shared"));
    }

    #[test]
    fn tokenized_signatures_are_readable() {
        assert_eq!(
            clean_signature(
                "fn charge (amount : & ChargeReq) -> Result < Receipt , Error >".into()
            ),
            "fn charge(amount: &ChargeReq) -> Result<Receipt, Error>"
        );
    }
}
