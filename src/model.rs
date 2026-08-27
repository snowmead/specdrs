//! Defines the versioned knowledge-map data model.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize, //
};

use crate::prelude::*;

specdrs_module!(in_spans("knowledge-map-model"));

/// Semantic axis occupied by a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Axis {
    /// Describes the outcome the owner exists to produce.
    Job,
    /// Describes the owner's caller-visible contract.
    Interface,
    /// Describes external state changes caused by the owner.
    Effects,
    /// Describes conditions that must always hold.
    Invariants,
    /// Describes facts the owner assumes to be true.
    Assumptions,
    /// Describes retained state and its lifecycle.
    State,
    /// Describes latency, ordering, and deadline requirements.
    Time,
    /// Describes failure behavior and recovery.
    Failure,
    /// Describes bounded compute, memory, and external work, including growth
    /// with input size.
    Resources,
    /// Describes permissions and decision rights.
    Authority,
    /// Describes emitted signals used to inspect behavior.
    Observation,
    /// Describes compatibility and evolution constraints.
    Change,
}

impl Axis {
    /// Contains every semantic axis in declaration order.
    pub const ALL: [Self; 12] = [
        Self::Job,
        Self::Interface,
        Self::Effects,
        Self::Invariants,
        Self::Assumptions,
        Self::State,
        Self::Time,
        Self::Failure,
        Self::Resources,
        Self::Authority,
        Self::Observation,
        Self::Change,
    ];

    /// Creates an unspecified entry for every semantic axis.
    pub(crate) fn empty_map() -> BTreeMap<Self, AxisEntry> {
        Self::ALL
            .into_iter()
            .map(|axis| (axis, AxisEntry::unspecified()))
            .collect()
    }
}

impl fmt::Display for Axis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl FromStr for Axis {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|axis| axis.to_string() == value)
            .ok_or_else(|| format!("unknown axis `{value}`"))
    }
}

/// Describes how strongly a claim binds its owner.
#[specdrs(
    claims(
        Constraints(
            Interface(
                "A claim is exactly one of objective, constraint, or assumption." as three_claim_kinds,
            ),
        ),
        evidence(
            three_claim_kinds(Test = crate::model::tests::schema_two_serialization_shape),
        ),
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClaimKind {
    /// States an outcome the owner is expected to achieve.
    Objective,
    /// States a condition the owner must satisfy.
    Constraint,
    /// States a fact accepted without enforcement by the owner.
    Assumption,
}

/// Identifies an inspectable artifact linked to a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// Links to a Rust type.
    Type,
    /// Links to a test function.
    Test,
    /// Links to a fuzz target.
    Fuzz,
    /// Links to a proof artifact.
    Proof,
    /// Links to a lint or static check.
    Lint,
}

/// Describes the resolution or runner result for an evidence link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceResult {
    /// The artifact resolved but was not executed.
    Linked,
    /// The artifact executed successfully.
    Passed,
    /// The artifact executed and failed.
    Failed,
    /// No compatible artifact could be resolved or executed.
    Unavailable,
}

/// Links a claim to an inspectable artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Identifies the artifact type.
    pub kind: EvidenceKind,
    /// Contains the artifact's Rust item path.
    pub binder: String,
    /// Contains the artifact's resolution or runner result.
    pub result: EvidenceResult,
}

/// States one proposition owned by a span or Rust item.
#[specdrs(
    claims(
        Constraints(
            Interface(
                "A claim carries a stable alias, one proposition of prose, one kind, and one of the twelve axes." as claim_has_four_fields,
            ),
        ),
        evidence(
            claim_has_four_fields(Test = crate::model::tests::schema_two_serialization_shape),
        ),
    )
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// Contains the stable alias within the owning claim block.
    pub id: String,
    /// Describes how the proposition binds its owner.
    pub kind: ClaimKind,
    /// Contains the proposition.
    pub text: String,
    /// Contains artifacts linked to the proposition.
    pub evidence: Vec<Evidence>,
}

/// Describes whether an owner accounts for one semantic axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AxisStatus {
    /// The owner does not account for the axis.
    Unspecified,
    /// The owner explicitly declares that the axis does not apply.
    NotApplicable,
    /// The owner declares one or more claims for the axis.
    Specified,
}

/// Contains one owner's status and claims for a semantic axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxisEntry {
    /// Describes whether the owner accounts for the axis.
    pub status: AxisStatus,
    /// Explains why the axis does not apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Contains claims assigned to the axis.
    pub claims: Vec<Claim>,
}

impl AxisEntry {
    /// Creates an axis entry that has not been accounted for.
    pub(crate) fn unspecified() -> Self {
        Self {
            status: AxisStatus::Unspecified,
            reason: None,
            claims: Vec::new(),
        }
    }
}

/// Identifies a zero-based column on a one-based source line.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct SourcePosition {
    /// Contains the one-based source line.
    pub line: usize,
    /// Contains the zero-based UTF-8 byte column.
    pub column: usize,
}

/// Identifies a half-open range in one source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRange {
    /// Contains the source path relative to the package root.
    pub file: String,
    /// Contains the inclusive start position.
    pub start: SourcePosition,
    /// Contains the exclusive end position.
    pub end: SourcePosition,
}

/// Describes a named semantic span across Rust items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Contains the stable span identifier.
    pub id: String,
    /// Contains the optional parent span identifier.
    pub parent: Option<String>,
    /// Contains the Rust item path where reading starts.
    pub entry: String,
    /// Contains Rust item paths included in the span.
    pub members: Vec<String>,
    /// Contains all semantic axes for the span.
    pub axes: BTreeMap<Axis, AxisEntry>,
}

/// Describes one Rust item in a knowledge map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Contains the item's complete source range.
    pub source: SourceRange,
    /// Contains the item's normalized Rust signature.
    pub signature: String,
    /// Contains span identifiers assigned to the item.
    pub spans: Vec<String>,
    /// Contains all semantic axes for the item.
    pub axes: BTreeMap<Axis, AxisEntry>,
}

#[specdrs(
    span(
        id = "knowledge-map-model",
        parent = "specdrs",
        claims(
            Objectives(
                Job("Expose a versioned machine-readable map of spans, items, claims, evidence, and source locations." as purpose),
            ),
            Constraints(
                Interface(
                    "Schema 2 identifies the crate, spans, and Rust items with stable serialized field names." as schema_two_shape,
                    "The serialized claim kind carries no objective rank field." as objectives_are_unranked,
                    "Every emitted item carries a file and one-based line plus zero-based column boundaries." as source_location_shape,
                ),
                Invariants(
                    "Every owner contains exactly the twelve defined semantic axes." as twelve_axes,
                ),
            ),
            Assumptions(
                Change("Breaking schema changes increment the top-level schema number." as versioned_breaking_changes),
            ),
            NotApplicable(
                Effects = "The data model contains no effectful operations.",
                Time = "The data model defines no timing behavior.",
                Resources = "The data model defines no resource budget.",
            ),
            evidence(
                schema_two_shape(Test = crate::model::tests::schema_two_serialization_shape),
                source_location_shape(Test = crate::model::tests::schema_two_serialization_shape),
                twelve_axes(Test = crate::model::tests::empty_axes_contains_every_axis),
                objectives_are_unranked(Test = crate::model::tests::schema_two_serialization_shape),
            ),
        )
    ),
    claims(
        Constraints(
            Interface(
                "The serialized root contains schema, crate, spans, and items as required fields." as required_root_fields,
            ),
        ),
        evidence(
            required_root_fields(Test = crate::model::tests::schema_two_serialization_shape),
        ),
    )
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Contains the versioned knowledge map for one Rust crate.
pub struct KnowledgeMap {
    /// Contains the serialized schema version.
    pub schema: u32,
    /// Contains the normalized Rust crate name.
    #[serde(rename = "crate")]
    pub crate_name: String,
    /// Contains declared semantic spans.
    pub spans: Vec<Span>,
    /// Maps Rust item paths to item metadata and claims.
    pub items: BTreeMap<String, Item>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_axes_contains_every_axis() {
        let axes = Axis::empty_map();
        assert_eq!(axes.len(), Axis::ALL.len());
        assert!(
            Axis::ALL
                .iter()
                .all(|axis| axes[axis].status == AxisStatus::Unspecified)
        );
    }

    #[test]
    fn schema_two_serialization_shape() {
        let map = KnowledgeMap {
            schema: 2,
            crate_name: "sample".into(),
            spans: Vec::new(),
            items: BTreeMap::new(),
        };
        let json = serde_json::to_value(map).unwrap();
        assert_eq!(json["schema"], 2);
        assert_eq!(json["crate"], "sample");
        assert!(json.get("spans").is_some());
        assert!(json.get("items").is_some());

        let claim = serde_json::to_value(Claim {
            id: "goal".into(),
            kind: ClaimKind::Objective,
            text: "Meet the goal.".into(),
            evidence: Vec::new(),
        })
        .unwrap();
        assert_eq!(claim["kind"], "Objective");
        assert!(claim.get("rank").is_none());
    }
}
