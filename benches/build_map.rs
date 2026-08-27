use std::env;
use std::fs;
use std::path::{
    Path,
    PathBuf, //
};
use std::process::Command;
use std::time::{
    Duration,
    Instant,
    SystemTime,
    UNIX_EPOCH, //
};

use specdrs::BuildOptions;

#[derive(Clone, Copy)]
struct Scenario {
    files: usize,
    depth: usize,
    fanout: usize,
    items_per_file: usize,
    annotate_every: usize,
    evidence_every: usize,
    colliding_impls: usize,
    samples: usize,
    build_only: bool,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            files: 16,
            depth: 3,
            fanout: 8,
            items_per_file: 100,
            annotate_every: 20,
            evidence_every: 2,
            colliding_impls: 0,
            samples: 30,
            build_only: false,
        }
    }
}

struct Fixture {
    root: PathBuf,
    manifest: PathBuf,
    sources: Vec<PathBuf>,
}

impl Fixture {
    fn generate(scenario: Scenario) -> Result<Self, String> {
        validate(scenario)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "specdrs-build-bench-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root)
            .map_err(|error| format!("cannot create {}: {error}", root.display()))?;

        let manifest = root.join("Cargo.toml");
        let specdrs = Path::new(env!("CARGO_MANIFEST_DIR"));
        fs::write(
            &manifest,
            format!(
                "[package]\nname = \"synthetic-build-bench\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nspecdrs = {{ path = {:?} }}\n",
                specdrs
            ),
        )
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))?;

        let source_dir = root.join("src");
        fs::create_dir(&source_dir)
            .map_err(|error| format!("cannot create {}: {error}", source_dir.display()))?;
        let parents = module_parents(scenario)?;
        let mut children = vec![Vec::new(); scenario.files];
        for (child, parent) in parents.into_iter().enumerate().skip(1) {
            children[parent].push(child);
        }

        let mut sources = Vec::with_capacity(scenario.files);
        for (file_index, child_modules) in children.iter().enumerate() {
            let path = if file_index == 0 {
                source_dir.join("lib.rs")
            } else {
                source_dir.join(format!("module_{file_index}.rs"))
            };
            let source = source_file(file_index, child_modules, scenario);
            fs::write(&path, source)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            sources.push(path);
        }

        Ok(Self {
            root,
            manifest,
            sources,
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn validate(scenario: Scenario) -> Result<(), String> {
    if scenario.files == 0 {
        return Err("--files must be at least 1".into());
    }
    if scenario.depth == 0 && scenario.files > 1 {
        return Err("--depth must be at least 1 when --files exceeds 1".into());
    }
    if scenario.fanout == 0 && scenario.files > 1 {
        return Err("--fanout must be at least 1 when --files exceeds 1".into());
    }
    if scenario.samples == 0 {
        return Err("--samples must be at least 1".into());
    }
    Ok(())
}

fn module_parents(scenario: Scenario) -> Result<Vec<usize>, String> {
    let mut parents = vec![0];
    let mut depths = vec![0];
    let mut next_parent = 0;
    let mut child_counts = vec![0usize];

    while parents.len() < scenario.files {
        while next_parent < parents.len()
            && (depths[next_parent] >= scenario.depth
                || child_counts[next_parent] >= scenario.fanout)
        {
            next_parent += 1;
        }
        if next_parent == parents.len() {
            return Err(format!(
                "{0} files do not fit within depth {1} and fanout {2}",
                scenario.files, scenario.depth, scenario.fanout
            ));
        }
        parents.push(next_parent);
        depths.push(depths[next_parent] + 1);
        child_counts[next_parent] += 1;
        child_counts.push(0);
    }
    Ok(parents)
}

fn source_file(file_index: usize, children: &[usize], scenario: Scenario) -> String {
    let mut source = String::with_capacity(scenario.items_per_file.saturating_mul(160));
    source.push_str("use specdrs::prelude::*;\n\n");
    for child in children {
        source.push_str(&format!(
            "#[path = \"module_{child}.rs\"]\nmod module_{child};\n"
        ));
    }

    for item_index in 0..scenario.items_per_file {
        let annotated = scenario.annotate_every != 0 && item_index % scenario.annotate_every == 0;
        if annotated {
            let annotation_index = item_index / scenario.annotate_every;
            let has_evidence = scenario.evidence_every != 0
                && annotation_index.is_multiple_of(scenario.evidence_every);
            source.push_str("#[specdrs(claims(Constraints(Job(\n");
            source.push_str(&format!(
                "    \"Synthetic benchmark claim.\" as claim_{file_index}_{item_index}"
            ));
            source.push_str("\n))");
            if has_evidence {
                source.push_str(&format!(
                    ", evidence(claim_{file_index}_{item_index}(Test = self::evidence_{file_index}_{item_index}))"
                ));
            }
            source.push_str("\n))]\n");
        }
        source.push_str(&format!(
            "pub fn item_{file_index}_{item_index}(value: usize) -> usize {{ value + {item_index} }}\n"
        ));
        if annotated
            && scenario.evidence_every != 0
            && (item_index / scenario.annotate_every).is_multiple_of(scenario.evidence_every)
        {
            source.push_str(&format!(
                "#[test]\nfn evidence_{file_index}_{item_index}() {{}}\n"
            ));
        }
    }

    if scenario.colliding_impls != 0 {
        source.push_str(&format!("struct Subject{file_index};\n"));
        for trait_index in 0..scenario.colliding_impls {
            source.push_str(&format!(
                "trait Trait{file_index}_{trait_index} {{ fn shared(&self); }}\n"
            ));
            source.push_str(&format!(
                "impl Trait{file_index}_{trait_index} for Subject{file_index} {{\n    #[specdrs(claims(Constraints(Job(\"Synthetic colliding method.\" as collision_{file_index}_{trait_index}))))]\n    fn shared(&self) {{}}\n}}\n"
            ));
        }
    }
    source
}

fn parse_args() -> Result<Scenario, String> {
    let mut scenario = Scenario::default();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        if option == "--bench" {
            index += 1;
            continue;
        }
        if option == "--build-only" {
            scenario.build_only = true;
            index += 1;
            continue;
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("{option} requires an integer"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid value for {option}: {error}"))?;
        match option {
            "--files" => scenario.files = value,
            "--depth" => scenario.depth = value,
            "--fanout" => scenario.fanout = value,
            "--items" => scenario.items_per_file = value,
            "--annotate-every" => scenario.annotate_every = value,
            "--evidence-every" => scenario.evidence_every = value,
            "--colliding-impls" => scenario.colliding_impls = value,
            "--samples" => scenario.samples = value,
            _ => return Err(format!("unknown option `{option}`")),
        }
        index += 1;
    }
    validate(scenario)?;
    Ok(scenario)
}

fn measure(
    samples: usize,
    mut operation: impl FnMut() -> Result<(), String>,
) -> Result<Vec<Duration>, String> {
    operation()?;
    let mut durations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        operation()?;
        durations.push(started.elapsed());
    }
    durations.sort_unstable();
    Ok(durations)
}

fn percentile(durations: &[Duration], percentile: usize) -> Duration {
    let index = durations
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1);
    durations[index.min(durations.len() - 1)]
}

fn report(name: &str, durations: &[Duration]) {
    println!(
        "{name}: p50={:.3}ms p95={:.3}ms",
        percentile(durations, 50).as_secs_f64() * 1_000.0,
        percentile(durations, 95).as_secs_f64() * 1_000.0
    );
}

fn run() -> Result<(), String> {
    if !env::args().any(|arg| arg == "--bench") {
        return Ok(());
    }
    if env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "synthetic build_map benchmark\n\n\
Options:\n  --files <N>\n  --depth <N>\n  --fanout <N>\n  --items <N>\n  \
--annotate-every <N>\n  --evidence-every <N>\n  --colliding-impls <N>\n  \
--samples <N>\n  --build-only\n\nRun with `--build-only` under `/usr/bin/time -l` to record peak RSS."
        );
        return Ok(());
    }
    let scenario = parse_args()?;
    let fixture = Fixture::generate(scenario)?;
    println!(
        "files={} depth={} fanout={} items/file={} annotate/every={} evidence/every={} colliding_impls={} samples={}",
        scenario.files,
        scenario.depth,
        scenario.fanout,
        scenario.items_per_file,
        scenario.annotate_every,
        scenario.evidence_every,
        scenario.colliding_impls,
        scenario.samples
    );

    if !scenario.build_only {
        let metadata = measure(scenario.samples, || {
            let output = Command::new("cargo")
                .args([
                    "metadata",
                    "--format-version",
                    "1",
                    "--no-deps",
                    "--manifest-path",
                ])
                .arg(&fixture.manifest)
                .output()
                .map_err(|error| format!("cannot run cargo metadata: {error}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        })?;
        report("metadata", &metadata);

        let parse = measure(scenario.samples, || {
            for path in &fixture.sources {
                let source = fs::read_to_string(path)
                    .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
                syn::parse_file(&source)
                    .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
            }
            Ok(())
        })?;
        report("full_ast_parse", &parse);
    }

    let build = measure(scenario.samples, || {
        BuildOptions {
            manifest_path: fixture.manifest.clone(),
            package: None,
        }
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
    })?;
    report("build_map", &build);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
