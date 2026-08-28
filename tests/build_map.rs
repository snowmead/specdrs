use std::fs;
use std::path::PathBuf;

use specdrs::{
    Axis,
    AxisStatus,
    BuildOptions,
    EvidenceResult, //
};

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/payments/Cargo.toml")
}

#[test]
fn builds_spans_items_axes_ranges_and_evidence() {
    let map = BuildOptions {
        manifest_path: fixture_manifest(),
        package: None,
    }
    .build()
    .expect("fixture map should build");

    assert_eq!(map.schema, 2);
    assert_eq!(map.crate_name, "payments");
    assert_eq!(map.spans.len(), 4);

    let checkout = map.spans.iter().find(|span| span.id == "checkout").unwrap();
    assert_eq!(checkout.id, "checkout");
    assert_eq!(checkout.entry, "payments::charge");
    for member in [
        "payments::charge",
        "payments::inline_member",
        "payments::inline_member::inherited",
        "payments::stripe",
        "payments::stripe::capture",
        "payments::stripe::inherited_shapes",
        "payments::stripe::inherited_shapes::Gateway",
        "payments::stripe::inherited_shapes::Gateway::send",
        "payments::stripe::inherited_shapes::Operation",
        "payments::stripe::inherited_shapes::Operation::execute",
        "payments::stripe::inherited_shapes::Status",
        "payments::stripe::inherited_shapes::Value",
        "payments::stripe::inherited_shapes::Amount",
        "payments::stripe::inherited_shapes::DEFAULT_AMOUNT",
        "payments::stripe::inherited_shapes::ENABLED",
    ] {
        assert!(
            checkout.members.iter().any(|candidate| candidate == member),
            "checkout is missing inherited member `{member}`"
        );
    }
    assert_eq!(checkout.axes.len(), 12);
    assert_eq!(checkout.axes[&Axis::Job].status, AxisStatus::Specified);
    assert_eq!(
        checkout.axes[&Axis::Resources].status,
        AxisStatus::NotApplicable
    );
    assert_eq!(
        checkout.axes[&Axis::Time].claims[0].evidence[0].result,
        EvidenceResult::Linked
    );

    let audit = map.spans.iter().find(|span| span.id == "audit").unwrap();
    assert_eq!(audit.entry, "payments::charge");
    assert_eq!(
        audit.members,
        [
            "payments::audit",
            "payments::charge",
            "payments::stripe::capture",
            "payments::stripe::inherited_shapes::Gateway::send",
        ],
        "an attribute host and its distinct entry are both direct members"
    );
    assert_eq!(map.items["payments::audit"].spans, ["audit"]);
    assert_eq!(map.items["payments::charge"].spans, ["audit", "checkout"]);

    let capture = &map.items["payments::stripe::capture"];
    assert_eq!(capture.source.file, "src/stripe.rs");
    assert!(capture.source.end.line >= capture.source.start.line);
    assert_eq!(capture.signature, "fn capture(amount: u64) -> u64");
    assert_eq!(capture.spans, ["audit", "checkout", "ledger"]);
    assert_eq!(
        map.items["payments::stripe::inherited_shapes::Gateway::send"].spans,
        ["audit", "checkout", "gateway"],
        "`gateway` arrives from the enclosing impl block"
    );
    assert_eq!(capture.axes.len(), 12);
    assert_eq!(
        capture.axes[&Axis::Resources].status,
        AxisStatus::NotApplicable
    );

    let claims = map.item_claims("payments::stripe::capture").unwrap();
    assert!(
        claims
            .iter()
            .any(|claim| claim.owner == "span:checkout" && claim.claim.id == "purpose")
    );
    assert!(claims.iter().any(|claim| {
        claim.owner == "item:payments::stripe::capture" && claim.claim.id == "positive_amount"
    }));

    let gateway = map.spans.iter().find(|span| span.id == "gateway").unwrap();
    assert_eq!(
        gateway.entry, "payments::stripe::inherited_shapes::Gateway::send",
        "`entry = self::Gateway::send` resolves against the declaring module"
    );
    assert_eq!(gateway.parent.as_deref(), Some("checkout"));
    assert_eq!(
        gateway.members,
        [
            "payments::stripe::inherited_shapes::Gateway::retry",
            "payments::stripe::inherited_shapes::Gateway::send"
        ],
        "the impl block seeds every method, including one with no attribute of its own"
    );
    assert_eq!(
        gateway.axes[&Axis::Interface].claims[0].evidence[0].result,
        EvidenceResult::Linked
    );
    assert!(
        !map.items
            .contains_key("payments::stripe::inherited_shapes::Gateway::gateway"),
        "a container declaration contributes no item of its own"
    );

    let ledger = map.spans.iter().find(|span| span.id == "ledger").unwrap();
    assert_eq!(
        ledger.entry, "payments::stripe::capture",
        "`entry = self::capture` resolves against the declaring module"
    );
    assert_eq!(ledger.parent.as_deref(), Some("checkout"));
    assert_eq!(
        ledger.members,
        ["payments::stripe::capture"],
        "the resolved entry is the span's only member and no host item is synthesized"
    );
    assert_eq!(
        ledger.axes[&Axis::Invariants].claims[0].evidence[0].result,
        EvidenceResult::Linked,
        "`self::tests::positive_amount` resolves against the declaring module"
    );
    assert!(
        !map.items
            .keys()
            .any(|path| path.contains("specdrs_span") || path.contains("__specdrs_span")),
        "a free-standing span declaration contributes no item"
    );

    let source = fs::read_to_string(
        fixture_manifest()
            .parent()
            .unwrap()
            .join(&capture.source.file),
    )
    .unwrap();
    let selected = source
        .lines()
        .skip(capture.source.start.line - 1)
        .take(capture.source.end.line - capture.source.start.line + 1)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(selected.starts_with("/// Capture an authorized payment."));
    assert!(selected.contains("#[specdrs("));
    assert!(selected.contains("pub fn capture(amount: u64) -> u64"));
    assert!(selected.ends_with('}'));
}

#[test]
fn emitted_json_uses_the_versioned_public_shape() {
    let map = BuildOptions {
        manifest_path: fixture_manifest(),
        package: None,
    }
    .build()
    .unwrap();
    let json = serde_json::to_value(map).unwrap();
    let checkout = json["spans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|span| span["id"] == "checkout")
        .unwrap();

    assert_eq!(json["schema"], 2);
    assert_eq!(json["crate"], "payments");
    assert_eq!(checkout["axes"]["Job"]["status"], "Specified");
    assert_eq!(checkout["axes"]["Job"]["claims"][0]["kind"], "Objective");
}
