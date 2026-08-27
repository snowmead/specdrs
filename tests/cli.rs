use std::path::PathBuf;
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/payments/Cargo.toml")
}

#[test]
fn emit_and_show_work_for_a_consumer_crate() {
    let binary = env!("CARGO_BIN_EXE_cargo-specdrs");
    let emitted = Command::new(binary)
        .args(["emit", "--manifest-path"])
        .arg(fixture_manifest())
        .arg("--stdout")
        .output()
        .expect("emit should run");
    assert!(emitted.status.success());
    let json: serde_json::Value = serde_json::from_slice(&emitted.stdout).unwrap();
    assert!(
        json["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["id"] == "checkout")
    );

    let shown = Command::new(binary)
        .args(["show", "checkout", "--manifest-path"])
        .arg(fixture_manifest())
        .output()
        .expect("show should run");
    assert!(shown.status.success());
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("entry: payments::charge"));
    assert!(text.contains("unspecified:"));
}

#[test]
fn how_prints_the_authoring_guide() {
    let output = Command::new(env!("CARGO_BIN_EXE_specdrs"))
        .arg("how")
        .output()
        .expect("how should run");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("specdrs authoring guide"));
    assert!(text.contains("#[specdrs]"));
    assert!(text.contains("specdrs_span!"));
    assert!(text.contains("specdrs_module!"));
    assert!(text.contains("Objectives"));
    assert!(text.contains("in_spans"));
    assert!(!text.starts_with('{'));
}

#[test]
fn command_help_does_not_execute_the_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-specdrs"))
        .args(["emit", "--help"])
        .output()
        .expect("help should run");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("specdrs knowledge maps"));
    assert!(!text.starts_with('{'));
}

#[test]
fn show_json_uses_requested_projection_and_inherits_span_claims() {
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-specdrs"))
        .args(["show", "payments::stripe::capture", "--manifest-path"])
        .arg(fixture_manifest())
        .args(["--group-by", "owner,kind,axis", "--json"])
        .output()
        .expect("show should run");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["group_by"],
        serde_json::json!(["owner", "kind", "axis"])
    );
    let claims = json["claims"].as_array().unwrap();
    assert!(claims.iter().any(|claim| claim["owner"] == "span:checkout"));
    assert!(
        claims
            .iter()
            .any(|claim| { claim["owner"] == "item:payments::stripe::capture" })
    );
}
