pub const AXES: &str = "Job, Interface, Effects, Invariants, Assumptions, State, Time, Failure, Resources, Authority, Observation, Change";
pub const CLAIM_GROUPS: &str = "Objectives, Constraints, Assumptions, NotApplicable, evidence";
pub const DIRECTIVES: &str = "span(...), in_spans(...), claims(...)";
pub const EVIDENCE_KINDS: &str = "Type, Test, Fuzz, Proof, Lint";
pub const SPAN_FIELDS: &str = "id, parent, entry, claims";

pub fn unknown_directive() -> String {
    format!(
        "unknown specdrs directive. Directives are {DIRECTIVES}. Use span(...) to declare a span, in_spans(...) to join this item to spans, or claims(...) to attach claims to this item"
    )
}

pub fn unknown_span_field() -> String {
    format!(
        "unknown span field. Span fields are {SPAN_FIELDS}. Example: span(id = \"checkout\", parent = \"payments\", entry = crate::checkout::run, claims(...))"
    )
}

pub fn duplicate_span_field() -> &'static str {
    "duplicate span field. Each of id, parent, entry, and claims may appear once in span(...)"
}

pub fn span_requires_id() -> &'static str {
    "span requires `id`. Write span(id = \"checkout\", ...)"
}

pub fn unknown_claims_group() -> String {
    format!(
        "unknown claims group. Groups must appear in this order, omitting unused ones: {CLAIM_GROUPS}"
    )
}

pub fn claims_group_order() -> String {
    format!(
        "claims groups must be ordered as {CLAIM_GROUPS}. Write Objectives, then Constraints, then Assumptions, then NotApplicable, then evidence. Do not interleave them"
    )
}

pub fn duplicate_claims_group() -> &'static str {
    "duplicate claims group. Each of Objectives, Constraints, Assumptions, NotApplicable, and evidence may appear once per claims(...) block"
}

pub fn unique_claim_aliases() -> &'static str {
    "claim aliases must be unique within one owner. Give each claim its own `as alias` and point evidence at that alias"
}

pub fn unknown_evidence_alias(alias: &str) -> String {
    format!(
        "evidence references unknown claim alias `{alias}`. Evidence names must match an `as alias` in this same claims(...) block"
    )
}

pub fn unknown_axis(value: &str) -> String {
    format!("unknown axis `{value}`. Axes are {AXES}")
}

pub fn duplicate_axis_in_kind_group() -> &'static str {
    "duplicate axis in claim kind group. List each axis once inside a kind: Constraints(Job(...), Failure(...))"
}

pub fn empty_axis_group() -> &'static str {
    "claim axis group must not be empty. Write Job(\"Complete checkout.\" as complete_checkout) or drop the axis"
}

pub fn empty_kind_group() -> &'static str {
    "claim kind group must not be empty. Put at least one axis with a claim inside Objectives, Constraints, or Assumptions, or omit the group"
}

pub fn duplicate_not_applicable_axis() -> &'static str {
    "duplicate axis in NotApplicable. Mark each axis once: NotApplicable(State = \"reason\", Time = \"reason\")"
}

pub fn empty_not_applicable() -> &'static str {
    "NotApplicable must not be empty. Write NotApplicable(State = \"Checkout retains no process-local state.\") or omit the group"
}

pub fn duplicate_evidence_alias() -> &'static str {
    "duplicate evidence alias. Each claim alias appears once in evidence(...): evidence(complete_checkout(Test = crate::tests::case))"
}

pub fn duplicate_evidence_link() -> &'static str {
    "duplicate evidence link. Repeat a Type/Test/Fuzz/Proof/Lint binder at most once under the same alias"
}

pub fn empty_evidence_alias() -> &'static str {
    "evidence alias must contain at least one link. Write complete_checkout(Test = crate::tests::case) or drop the alias"
}

pub fn empty_evidence_group() -> &'static str {
    "evidence must not be empty. Write evidence(alias(Test = crate::tests::case)) or omit the evidence group"
}

pub fn unknown_evidence_kind() -> String {
    format!(
        "unknown evidence kind. Evidence kinds are {EVIDENCE_KINDS}. Write Test = crate::tests::case"
    )
}

pub fn empty_in_spans() -> &'static str {
    "in_spans requires at least one span id. Write in_spans(\"checkout\") to join this item to a span declared elsewhere"
}

pub fn string_literal_required(name: &str) -> String {
    format!("`{name}` requires a string literal. Write {name} = \"checkout\"")
}

pub fn specdrs_requires_arguments() -> &'static str {
    "specdrs requires arguments. Write #[specdrs(span(...))], #[specdrs(in_spans(\"checkout\"))], or #[specdrs(claims(...))]"
}

pub fn specdrs_span_requires_entry() -> &'static str {
    "specdrs_span! requires `entry` because the invocation has no host item. Write specdrs_span!(id = \"checkout\", entry = crate::checkout::run)"
}

pub fn specdrs_module_requires_in_spans() -> &'static str {
    "specdrs_module! requires one or more in_spans directives. Write specdrs_module!(in_spans(\"checkout\")). It cannot declare a span or own claims"
}

pub fn impl_cannot_own_claims() -> &'static str {
    "an impl block cannot own claims. An impl has no def path, so it cannot be a claim owner. Declare span(...) on the impl with an explicit entry, or move claims(...) to the implemented type or one method"
}

pub fn impl_span_requires_entry(id: &str) -> String {
    format!(
        "span `{id}` declared on an impl block requires `entry`; an impl block has no def path to default to. Write span(id = \"{id}\", entry = self::Type::method)"
    )
}
