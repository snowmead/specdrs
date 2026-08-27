//! Projects stored claims into caller-selected reading orders.

use std::cmp::Ordering;
use std::collections::{
    BTreeMap,
    BTreeSet, //
};
use std::fmt;
use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize, //
};

use crate::prelude::*;

specdrs_module!(in_spans("claim-projection"));

use crate::{
    Axis,
    AxisEntry,
    Claim,
    ClaimKind,
    Item,
    KnowledgeMap,
    Span, //
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Selects one key used to order and display projected claims.
pub enum GroupKey {
    /// Groups claims by their span or item owner.
    Owner,
    /// Groups claims by objective, constraint, or assumption.
    Kind,
    /// Groups claims by semantic axis.
    Axis,
}

impl fmt::Display for GroupKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Owner => "owner",
            Self::Kind => "kind",
            Self::Axis => "axis",
        };
        formatter.write_str(name)
    }
}

impl FromStr for GroupKey {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "owner" => Ok(Self::Owner),
            "kind" => Ok(Self::Kind),
            "axis" => Ok(Self::Axis),
            _ => Err(format!("unknown grouping key `{value}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Contains one claim with its fully qualified owner and semantic axis.
pub struct ProjectedClaim {
    /// Contains the `span:` or `item:` owner label.
    pub owner: String,
    /// Contains the claim's semantic axis.
    pub axis: Axis,
    /// Contains the stored claim.
    pub claim: Claim,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Contains claims ordered for a caller-selected grouped view.
pub struct ClaimProjection {
    /// Contains grouping keys in outermost-to-innermost order.
    pub group_by: Vec<GroupKey>,
    /// Contains claims sorted by [`group_by`].
    ///
    /// [`group_by`]: crate::ClaimProjection::group_by
    pub claims: Vec<ProjectedClaim>,
}

impl ClaimProjection {
    /// Creates a projection ordered by the requested grouping keys.
    ///
    /// # Errors
    ///
    /// Returns an error when no grouping key is supplied or a key repeats.
    pub fn new(mut claims: Vec<ProjectedClaim>, group_by: Vec<GroupKey>) -> Result<Self, String> {
        Self::validate_grouping(&group_by)?;
        claims.sort_by(|left, right| Self::compare_claims(left, right, &group_by));
        Ok(Self { group_by, claims })
    }

    /// Validates that grouping keys are present and unique.
    ///
    /// # Errors
    ///
    /// Returns an error when no key is supplied or a key repeats.
    fn validate_grouping(group_by: &[GroupKey]) -> Result<(), String> {
        if group_by.is_empty() {
            return Err("grouping requires at least one key".into());
        }
        let unique: BTreeSet<_> = group_by
            .iter()
            .map(|key| match key {
                GroupKey::Owner => 0,
                GroupKey::Kind => 1,
                GroupKey::Axis => 2,
            })
            .collect();
        if unique.len() != group_by.len() {
            return Err("grouping keys must not repeat".into());
        }
        Ok(())
    }

    /// Compares two claims using the configured grouping order.
    #[specdrs(
        claims(
            Constraints(
                Invariants(
                    "Configured grouping keys decide group order without reordering claims inside an equal group." as stable_equal_groups,
                ),
            ),
            evidence(
                stable_equal_groups(Test = crate::projection::tests::claim_order_is_preserved_inside_one_group),
            ),
        )
    )]
    fn compare_claims(
        left: &ProjectedClaim,
        right: &ProjectedClaim,
        keys: &[GroupKey],
    ) -> Ordering {
        for key in keys {
            let ordering = match key {
                GroupKey::Owner => left.owner.cmp(&right.owner),
                GroupKey::Kind => {
                    Self::kind_order(left.claim.kind).cmp(&Self::kind_order(right.claim.kind))
                }
                GroupKey::Axis => left.axis.cmp(&right.axis),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        left.owner
            .cmp(&right.owner)
            .then(left.axis.cmp(&right.axis))
            .then(Self::kind_order(left.claim.kind).cmp(&Self::kind_order(right.claim.kind)))
    }

    /// Returns the fixed display order for a claim kind.
    fn kind_order(kind: ClaimKind) -> u8 {
        match kind {
            ClaimKind::Objective => 0,
            ClaimKind::Constraint => 1,
            ClaimKind::Assumption => 2,
        }
    }
}

impl KnowledgeMap {
    /// Returns claims owned by the requested span.
    pub fn span_claims(&self, span_id: &str) -> Option<Vec<ProjectedClaim>> {
        let span = self.spans.iter().find(|span| span.id == span_id)?;
        Some(owner_claims(&format!("span:{}", span.id), &span.axes))
    }

    #[specdrs(
    span(
        id = "claim-projection",
        parent = "specdrs",
        claims(
            Objectives(
                Job("Present stored claims in a caller-selected human reading order." as purpose),
            ),
            Constraints(
                Interface(
                    "Item views include applicable ancestor-span claims and claims owned by the item." as item_view_contract,
                    "Local item projections exclude span-owned claims." as local_item_contract,
                ),
                Invariants(
                    "Ancestor spans precede descendants and each applicable span appears once." as ancestor_order,
                    "Claims inside one owner-kind-axis group retain authored order." as stable_claim_order,
                ),
                Failure(
                    "Unknown owners return no projection and invalid grouping keys return an error." as invalid_projection_fails,
                ),
            ),
            Assumptions(
                Assumptions(
                    "The knowledge map has already passed span-parent validation." as validated_map,
                ),
            ),
            NotApplicable(
                Effects = "Projection only reads the knowledge map.",
            ),
            evidence(
                item_view_contract(Test = crate::projection::tests::item_view_includes_span_and_local_claims),
                local_item_contract(Test = crate::projection::tests::item_view_includes_span_and_local_claims),
                ancestor_order(Test = crate::projection::tests::ancestors_precede_descendants_without_duplicates),
                stable_claim_order(Test = crate::projection::tests::claim_order_is_preserved_inside_one_group),
                invalid_projection_fails(Test = crate::projection::tests::duplicate_group_keys_are_rejected),
            ),
        )
    ),
    claims(
        Constraints(
            Interface(
                "The returned view concatenates ancestor-span, direct-span, and local item claims without changing ownership labels." as composed_item_view,
            ),
        ),
        evidence(
            composed_item_view(Test = crate::projection::tests::item_view_includes_span_and_local_claims),
        ),
    )
)]
    /// Returns inherited span claims followed by claims owned by the requested item.
    pub fn item_claims(&self, item_path: &str) -> Option<Vec<ProjectedClaim>> {
        let item = self.items.get(item_path)?;
        let mut claims = Vec::new();
        for span in self.applicable_spans(item) {
            claims.extend(owner_claims(&format!("span:{}", span.id), &span.axes));
        }
        claims.extend(owner_claims(&format!("item:{item_path}"), &item.axes));
        Some(claims)
    }

    #[specdrs(
    claims(
        Constraints(
            Interface(
                "The returned projection contains only claims whose owner is the requested item." as excludes_span_claims,
            ),
        ),
        evidence(
            excludes_span_claims(Test = crate::projection::tests::item_view_includes_span_and_local_claims),
        ),
    )
)]
    /// Returns claims owned by the requested item without inherited span claims.
    pub fn local_item_claims(&self, item_path: &str) -> Option<Vec<ProjectedClaim>> {
        let item = self.items.get(item_path)?;
        Some(owner_claims(&format!("item:{item_path}"), &item.axes))
    }

    #[specdrs(
    claims(
        Constraints(
            Invariants(
                "Parent spans are returned before children and repeated memberships do not duplicate a span." as parent_first_deduplicated,
            ),
        ),
        evidence(
            parent_first_deduplicated(Test = crate::projection::tests::ancestors_precede_descendants_without_duplicates),
        ),
    )
)]
    /// Returns the item's applicable spans in ancestor-first order.
    pub fn applicable_spans<'a>(&'a self, item: &Item) -> Vec<&'a Span> {
        let mut spans = Vec::new();
        let mut visited = BTreeSet::new();
        for span_id in &item.spans {
            self.visit_span(span_id, &mut visited, &mut spans);
        }
        spans
    }

    /// Adds one span and its ancestors to an ancestor-first traversal.
    fn visit_span<'a>(
        &'a self,
        span_id: &str,
        visited: &mut BTreeSet<String>,
        spans: &mut Vec<&'a Span>,
    ) {
        if visited.contains(span_id) {
            return;
        }
        let Some(span) = self.spans.iter().find(|span| span.id == span_id) else {
            return;
        };
        if let Some(parent) = &span.parent {
            self.visit_span(parent, visited, spans);
        }
        if visited.insert(span.id.clone()) {
            spans.push(span);
        }
    }
}

/// Projects every claim in an axis map under one owner label.
fn owner_claims(owner: &str, axes: &BTreeMap<Axis, AxisEntry>) -> Vec<ProjectedClaim> {
    axes.iter()
        .flat_map(|(axis, entry)| {
            entry.claims.iter().cloned().map(|claim| ProjectedClaim {
                owner: owner.to_owned(),
                axis: *axis,
                claim,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axes_with_claim(id: &str) -> std::collections::BTreeMap<Axis, crate::AxisEntry> {
        let mut axes = Axis::empty_map();
        let entry = axes.get_mut(&Axis::Job).unwrap();
        entry.status = crate::AxisStatus::Specified;
        entry.claims.push(Claim {
            id: id.into(),
            kind: ClaimKind::Constraint,
            text: id.into(),
            evidence: Vec::new(),
        });
        axes
    }

    fn nested_map() -> KnowledgeMap {
        KnowledgeMap {
            schema: 2,
            crate_name: "sample".into(),
            spans: vec![
                Span {
                    id: "parent".into(),
                    parent: None,
                    entry: "sample::work".into(),
                    members: vec!["sample::work".into()],
                    axes: axes_with_claim("parent_claim"),
                },
                Span {
                    id: "child".into(),
                    parent: Some("parent".into()),
                    entry: "sample::work".into(),
                    members: vec!["sample::work".into()],
                    axes: axes_with_claim("child_claim"),
                },
            ],
            items: std::collections::BTreeMap::from([(
                "sample::work".into(),
                Item {
                    source: crate::SourceRange {
                        file: "src/lib.rs".into(),
                        start: crate::SourcePosition { line: 1, column: 0 },
                        end: crate::SourcePosition {
                            line: 1,
                            column: 12,
                        },
                    },
                    signature: "fn work()".into(),
                    spans: vec!["child".into(), "parent".into()],
                    axes: axes_with_claim("item_claim"),
                },
            )]),
        }
    }

    #[test]
    fn duplicate_group_keys_are_rejected() {
        let error = ClaimProjection::new(Vec::new(), vec![GroupKey::Kind, GroupKey::Kind])
            .expect_err("duplicate keys should fail");
        assert!(error.contains("repeat"));
    }

    #[test]
    fn claim_order_is_preserved_inside_one_group() {
        let claims = ["written_first", "written_second"]
            .into_iter()
            .map(|id| ProjectedClaim {
                owner: "item:crate::work".into(),
                axis: Axis::Job,
                claim: Claim {
                    id: id.into(),
                    kind: ClaimKind::Objective,
                    text: id.into(),
                    evidence: Vec::new(),
                },
            })
            .collect();
        let projection = ClaimProjection::new(
            claims,
            vec![GroupKey::Kind, GroupKey::Axis, GroupKey::Owner],
        )
        .unwrap();
        assert_eq!(projection.claims[0].claim.id, "written_first");
        assert_eq!(projection.claims[1].claim.id, "written_second");
    }

    #[test]
    fn ancestors_precede_descendants_without_duplicates() {
        let map = nested_map();
        let item = &map.items["sample::work"];
        let spans = map.applicable_spans(item);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.id.as_str())
                .collect::<Vec<_>>(),
            ["parent", "child"]
        );
    }

    #[test]
    fn item_view_includes_span_and_local_claims() {
        let map = nested_map();
        let all = map.item_claims("sample::work").unwrap();
        assert_eq!(
            all.iter()
                .map(|claim| claim.owner.as_str())
                .collect::<Vec<_>>(),
            ["span:parent", "span:child", "item:sample::work"]
        );
        let local = map.local_item_claims("sample::work").unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].owner, "item:sample::work");
    }
}
