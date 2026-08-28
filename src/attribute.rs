//! Converts shared engineering syntax into the crate's map-building model.

use syn::parse::{
    Parse,
    ParseStream, //
};

use crate::prelude::*;

specdrs_module!(in_spans("attribute-parsing"));

use crate::{
    Axis,
    ClaimKind,
    EvidenceKind, //
};

/// Contains every directive parsed from one engineering attribute.
pub(crate) struct SpecdrsArgs {
    pub directives: Vec<Directive>,
}

/// Identifies one supported engineering attribute directive.
pub(crate) enum Directive {
    Span(SpanArgs),
    InSpans(Vec<String>),
    Claims(ClaimsArgs),
}

/// Contains a semantic span declaration.
pub(crate) struct SpanArgs {
    pub id: String,
    pub parent: Option<String>,
    pub entry: Option<String>,
    pub claims: Option<ClaimsArgs>,
}

#[derive(Clone)]
/// Contains claims and not-applicable axis declarations for one owner.
pub(crate) struct ClaimsArgs {
    pub claims: Vec<ClaimArgs>,
    pub not_applicable: Vec<NotApplicableArgs>,
}

#[derive(Clone)]
/// Contains one parsed claim and its evidence declarations.
pub(crate) struct ClaimArgs {
    pub id: String,
    pub axis: Axis,
    pub kind: ClaimKind,
    pub text: String,
    pub evidence: Vec<EvidenceArgs>,
}

#[derive(Clone)]
/// Contains one parsed evidence declaration.
pub(crate) struct EvidenceArgs {
    pub kind: EvidenceKind,
    pub binder: String,
}

#[derive(Clone)]
/// Contains one parsed not-applicable axis declaration.
pub(crate) struct NotApplicableArgs {
    pub axis: Axis,
    pub reason: String,
}

impl Parse for SpecdrsArgs {
    #[specdrs(
        span(
            id = "attribute-parsing",
            parent = "specdrs",
            claims(
                Objectives(
                    Job("Parse compact engineering attributes into validated directives." as purpose),
                ),
                Constraints(
                    Interface(
                        "Accept span, in_spans, and claims as the complete top-level directive set." as directive_set,
                    ),
                    Invariants(
                        "Claim groups are ordered as objectives, constraints, assumptions, not-applicable axes, then evidence." as group_order,
                        "Claim aliases and kind-axis groups are unique within one owner." as unique_groups,
                        "Authored claim order is retained within each kind-axis group." as authored_order,
                    ),
                    Failure(
                        "Unknown directives, duplicate fields, and evidence for unknown claims are rejected during parsing." as reject_invalid_syntax,
                    ),
                    Resources(
                        "One attribute block is bounded by the Rust source that declares it." as bounded_by_source,
                    ),
                ),
                NotApplicable(
                    Effects = "Attribute parsing only constructs in-memory directives.",
                ),
                evidence(
                    directive_set(Test = crate::attribute::tests::parses_schema_two_attribute),
                    group_order(Test = crate::attribute::tests::rejects_out_of_order_groups),
                    unique_groups(
                        Test = crate::attribute::tests::rejects_duplicate_axis_groups,
                        Test = crate::attribute::tests::rejects_duplicate_claim_aliases,
                    ),
                    authored_order(Test = crate::attribute::tests::preserves_authored_claim_order),
                    reject_invalid_syntax(
                        Test = crate::attribute::tests::rejects_schema_one_directives,
                        Test = crate::attribute::tests::rejects_unknown_evidence_alias,
                    ),
                ),
            )
        ),
        claims(
            Constraints(
                Interface(
                    "The source scanner and proc macro consume the same parsed attribute shape." as shared_parser,
                ),
                Failure(
                    "A syntax error retains its original syn diagnostic." as preserves_diagnostic,
                ),
            ),
            evidence(
                shared_parser(Test = crate::attribute::tests::parses_schema_two_attribute),
                preserves_diagnostic(Test = crate::attribute::tests::rejects_schema_one_directives),
            ),
        )
    )]
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<specdrs_syntax::SpecdrsArgs>().map(Into::into)
    }
}

impl Parse for SpanArgs {
    #[specdrs(
        claims(
            Constraints(
                Interface(
                    "A free-standing span declaration parses with the same grammar as an attribute span directive." as shared_span_grammar,
                ),
            ),
            evidence(
                shared_span_grammar(Test = crate::attribute::tests::parses_free_standing_span_declaration),
            ),
        )
    )]
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<specdrs_syntax::SpanArgs>().map(Into::into)
    }
}

impl From<specdrs_syntax::SpecdrsArgs> for SpecdrsArgs {
    fn from(value: specdrs_syntax::SpecdrsArgs) -> Self {
        Self {
            directives: value.directives.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<specdrs_syntax::Directive> for Directive {
    fn from(value: specdrs_syntax::Directive) -> Self {
        match value {
            specdrs_syntax::Directive::Span(span) => Self::Span(span.into()),
            specdrs_syntax::Directive::InSpans(ids) => Self::InSpans(ids),
            specdrs_syntax::Directive::Claims(claims) => Self::Claims(claims.into()),
        }
    }
}

impl From<specdrs_syntax::SpanArgs> for SpanArgs {
    fn from(value: specdrs_syntax::SpanArgs) -> Self {
        Self {
            id: value.id,
            parent: value.parent,
            entry: value.entry,
            claims: value.claims.map(Into::into),
        }
    }
}

impl From<specdrs_syntax::ClaimsArgs> for ClaimsArgs {
    fn from(value: specdrs_syntax::ClaimsArgs) -> Self {
        Self {
            claims: value.claims.into_iter().map(Into::into).collect(),
            not_applicable: value.not_applicable.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<specdrs_syntax::ClaimArgs> for ClaimArgs {
    fn from(value: specdrs_syntax::ClaimArgs) -> Self {
        Self {
            id: value.id,
            axis: value.axis.into(),
            kind: value.kind.into(),
            text: value.text,
            evidence: value.evidence.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<specdrs_syntax::EvidenceArgs> for EvidenceArgs {
    fn from(value: specdrs_syntax::EvidenceArgs) -> Self {
        Self {
            kind: value.kind.into(),
            binder: value.binder,
        }
    }
}

impl From<specdrs_syntax::NotApplicableArgs> for NotApplicableArgs {
    fn from(value: specdrs_syntax::NotApplicableArgs) -> Self {
        Self {
            axis: value.axis.into(),
            reason: value.reason,
        }
    }
}

impl From<specdrs_syntax::Axis> for Axis {
    fn from(value: specdrs_syntax::Axis) -> Self {
        match value {
            specdrs_syntax::Axis::Job => Self::Job,
            specdrs_syntax::Axis::Interface => Self::Interface,
            specdrs_syntax::Axis::Effects => Self::Effects,
            specdrs_syntax::Axis::Invariants => Self::Invariants,
            specdrs_syntax::Axis::Assumptions => Self::Assumptions,
            specdrs_syntax::Axis::State => Self::State,
            specdrs_syntax::Axis::Time => Self::Time,
            specdrs_syntax::Axis::Failure => Self::Failure,
            specdrs_syntax::Axis::Resources => Self::Resources,
            specdrs_syntax::Axis::Authority => Self::Authority,
            specdrs_syntax::Axis::Observation => Self::Observation,
            specdrs_syntax::Axis::Change => Self::Change,
        }
    }
}

impl From<specdrs_syntax::ClaimKind> for ClaimKind {
    fn from(value: specdrs_syntax::ClaimKind) -> Self {
        match value {
            specdrs_syntax::ClaimKind::Objective => Self::Objective,
            specdrs_syntax::ClaimKind::Constraint => Self::Constraint,
            specdrs_syntax::ClaimKind::Assumption => Self::Assumption,
        }
    }
}

impl From<specdrs_syntax::EvidenceKind> for EvidenceKind {
    fn from(value: specdrs_syntax::EvidenceKind) -> Self {
        match value {
            specdrs_syntax::EvidenceKind::Type => Self::Type,
            specdrs_syntax::EvidenceKind::Test => Self::Test,
            specdrs_syntax::EvidenceKind::Fuzz => Self::Fuzz,
            specdrs_syntax::EvidenceKind::Proof => Self::Proof,
            specdrs_syntax::EvidenceKind::Lint => Self::Lint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_free_standing_span_declaration() {
        let span: SpanArgs = syn::parse_str(
            r#"
            id = "ledger",
            parent = "checkout",
            entry = self::capture,
            claims(Constraints(Invariants("Every capture is recorded." as recorded))),
            "#,
        )
        .expect("a bare span declaration should parse");
        assert_eq!(span.id, "ledger");
        assert_eq!(span.parent.as_deref(), Some("checkout"));
        assert_eq!(span.entry.as_deref(), Some("self :: capture"));
        assert_eq!(span.claims.expect("claims are declared").claims.len(), 1);
    }

    #[test]
    fn parses_schema_two_attribute() {
        let args: SpecdrsArgs = syn::parse_str(
            r#"
            span(
                id = "checkout",
                claims(
                    Objectives(Job("Charge the customer." as purpose)),
                    Constraints(
                        Invariants(
                            "One key creates one row." as no_duplicate,
                            "Every capture is recorded." as capture_recorded,
                        ),
                    ),
                    Assumptions(Change("The provider remains compatible." as provider_stability)),
                    NotApplicable(Resources = "No resource target."),
                    evidence(
                        no_duplicate(
                            Fuzz = crate::props::duplicate_keys,
                            Test = crate::tests::duplicate_keys,
                        ),
                    ),
                ),
            ),
            in_spans("audit", "payments"),
            claims(Constraints(Interface("Amount uses account currency." as currency))),
            "#,
        )
        .expect("schema two attribute should parse");

        assert_eq!(args.directives.len(), 3);
        let Directive::Span(span) = &args.directives[0] else {
            panic!("expected span");
        };
        let claims = span.claims.as_ref().expect("span claims");
        assert_eq!(claims.claims.len(), 4);
        assert_eq!(claims.claims[1].evidence.len(), 2);
    }

    #[test]
    fn rejects_schema_one_directives() {
        let error = syn::parse_str::<SpecdrsArgs>(
            r#"item_claim(id = "old", axis = Job, kind = Constraint, text = "Old.")"#,
        )
        .err()
        .expect("schema one syntax should fail");
        let message = error.to_string();
        assert!(message.contains("unknown specdrs directive"), "{message}");
        assert!(message.contains("in_spans(...)"), "{message}");
    }

    #[test]
    fn rejects_unknown_evidence_alias() {
        let error = syn::parse_str::<SpecdrsArgs>(
            r#"claims(
                Constraints(Job("Known." as known)),
                evidence(missing(Test = crate::tests::missing)),
            )"#,
        )
        .err()
        .expect("unknown alias should fail");
        assert!(error.to_string().contains("unknown claim alias"));
    }

    #[test]
    fn rejects_out_of_order_groups() {
        let error = syn::parse_str::<SpecdrsArgs>(
            r#"claims(
                Constraints(Job("Required." as required)),
                Objectives(Job("Desired." as desired)),
            )"#,
        )
        .err()
        .expect("out-of-order groups should fail");
        assert!(error.to_string().contains("must be ordered"));
    }

    #[test]
    fn rejects_duplicate_axis_groups() {
        let error = syn::parse_str::<SpecdrsArgs>(
            r#"claims(Constraints(
                Job("First." as first),
                Job("Second." as second),
            ))"#,
        )
        .err()
        .expect("duplicate axes should fail");
        assert!(error.to_string().contains("duplicate axis"));
    }

    #[test]
    fn rejects_duplicate_claim_aliases() {
        let error = syn::parse_str::<SpecdrsArgs>(
            r#"claims(Constraints(Job(
                "First." as duplicate,
                "Second." as duplicate,
            )))"#,
        )
        .err()
        .expect("duplicate aliases should fail");
        assert!(error.to_string().contains("aliases must be unique"));
    }

    #[test]
    fn preserves_authored_claim_order() {
        let args: SpecdrsArgs = syn::parse_str(
            r#"claims(Objectives(Job(
                "Written first." as z_first,
                "Written second." as a_second,
            )))"#,
        )
        .expect("ordered claims should parse");
        let Directive::Claims(claims) = &args.directives[0] else {
            panic!("expected claims");
        };
        assert_eq!(claims.claims[0].id, "z_first");
        assert_eq!(claims.claims[1].id, "a_second");
    }

    #[test]
    fn parses_qualified_evidence_binders() {
        let args: SpecdrsArgs = syn::parse_str(
            r#"claims(
                Constraints(Invariants("Qualified evidence." as qualified)),
                evidence(qualified(Test = <Receipt as From<Request>>::from)),
            )"#,
        )
        .expect("qualified binder should parse");

        let Directive::Claims(claims) = &args.directives[0] else {
            panic!("expected claims");
        };
        assert_eq!(
            claims.claims[0].evidence[0].binder,
            "< Receipt as From < Request > > :: from"
        );
    }
}
