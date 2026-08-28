use std::path::PathBuf;
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/payments/Cargo.toml")
}

#[test]
fn generated_specdrs_docs_follow_authored_docs() {
    let manifest = fixture_manifest();
    let output = Command::new("cargo")
        .args([
            "doc",
            "--no-deps",
            "--offline",
            "--document-private-items",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .expect("cargo doc should run");
    assert!(
        output.status.success(),
        "cargo doc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let html = std::fs::read_to_string(
        manifest
            .parent()
            .expect("fixture manifest should have a parent")
            .join("target/doc/payments/stripe/fn.capture.html"),
    )
    .expect("capture rustdoc should exist");
    let authored = html
        .find("Capture an authorized payment.")
        .expect("authored docs should be rendered");
    let generated = html
        .find("<h3 id=\"specdrs\"")
        .expect("specdrs heading should be rendered");

    assert!(authored < generated);
    assert_eq!(html.matches("<h3 id=\"specdrs\"").count(), 1);
    assert!(html.contains("Member of spans:"));
    assert!(html.contains("audit"));
    assert!(!html.contains("checkout"));
    assert!(html.contains("A capture amount must be positive."));
    assert!(html.contains("Test = crate::stripe::tests::positive_amount"));
    assert!(!html.contains("Charge the customer and establish what we owe them."));
    assert!(
        !html.contains("Every capture is recorded exactly once."),
        "specdrs_span! emits no Rust item, so its claims reach no hover"
    );

    let audit = std::fs::read_to_string(
        manifest
            .parent()
            .expect("fixture manifest should have a parent")
            .join("target/doc/payments/audit/index.html"),
    )
    .expect("audit module rustdoc should exist");
    assert!(audit.contains("Declares spans:"));
    assert!(audit.contains("This item is a member of every span it declares."));

    // An impl-level declaration DOES reach a hover, unlike specdrs_span!.
    // rustdoc renders it on the implemented type's page, and its heading depth
    // follows the host, so do not assert `<h3 id="specdrs"` here: a method
    // docblock renders `<h6>`, an impl docblock renders deeper than a standalone
    // page, and duplicate heading ids are renumbered in document order.
    let gateway = std::fs::read_to_string(
        manifest
            .parent()
            .expect("fixture manifest should have a parent")
            .join("target/doc/payments/stripe/inherited_shapes/struct.Gateway.html"),
    )
    .expect("Gateway rustdoc should exist");

    assert!(gateway.contains("Declares spans:"));
    assert!(gateway.contains("Move one authorized capture to the payment provider."));
    assert!(
        gateway.contains("Every item in this block is a member."),
        "the hover states that the block distributes its declared span"
    );
    let implementations = gateway
        .find("id=\"implementations\"")
        .expect("the type page should list implementations");
    let declared = gateway
        .find("Declares spans:")
        .expect("the impl declaration should be rendered");
    assert!(
        implementations < declared,
        "the impl-level section renders under the implementations heading"
    );
}
