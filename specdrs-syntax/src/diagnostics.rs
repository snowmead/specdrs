pub fn specdrs_requires_arguments() -> &'static str {
    "specdrs requires arguments. Write #[specdrs(span(...))], #[specdrs(in_spans(\"checkout\"))], or #[specdrs(claims(...))]"
}

pub fn specdrs_span_requires_entrypoint() -> &'static str {
    "specdrs_span! requires `entrypoint` because the invocation has no host item. Write specdrs_span!(id = \"checkout\", entrypoint = crate::checkout::run)"
}

pub fn specdrs_module_requires_in_spans() -> &'static str {
    "specdrs_module! requires one or more in_spans directives. Write specdrs_module!(in_spans(\"checkout\")). It cannot declare a span or own claims"
}

pub fn impl_cannot_own_claims() -> &'static str {
    "an impl block cannot own claims. An impl has no def path, so it cannot be a claim owner. Declare span(...) on the impl with an explicit entrypoint, or move claims(...) to the implemented type or one method"
}
