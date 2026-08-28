use std::collections::BTreeSet;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use specdrs_syntax::{
    ClaimArgs,
    ClaimsArgs,
    Directive,
    SpanArgs,
    SpecdrsArgs,
    impl_cannot_own_claims,
    specdrs_module_requires_in_spans,
    specdrs_requires_arguments,
    specdrs_span_requires_entry, //
};
use syn::parse::{ParseStream, Parser};
use syn::{Attribute, LitStr, Meta};

/// Attaches specdrs metadata and generated rustdoc to a Rust item.
///
/// Declaring a span makes an addressable host item and the resolved span entry
/// direct members. An `impl` block has no item identity, so its resolved entry
/// and methods join instead.
///
/// Invalid attribute syntax fails compilation:
///
/// ```compile_fail
/// use specdrs_macros::specdrs;
///
/// #[specdrs(in_spans())]
/// fn missing_span_id() {}
/// ```
#[proc_macro_attribute]
pub fn specdrs(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Applies span memberships to every item in the containing Rust module.
///
/// The source scanner consumes this declaration. The macro emits no Rust item.
#[proc_macro]
pub fn specdrs_module(input: TokenStream) -> TokenStream {
    validate_module_memberships(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares one semantic span without an attribute host item.
///
/// The declaration requires `entry`, because no host item can supply a default
/// def path. The source scanner consumes this declaration. The macro emits no
/// Rust item, so the resolved entry is its only automatic member and the span's
/// claims do not appear in any rustdoc hover.
///
/// A declaration without `entry` fails compilation:
///
/// ```compile_fail
/// use specdrs_macros::specdrs_span;
///
/// specdrs_span!(id = "checkout");
/// ```
#[proc_macro]
pub fn specdrs_span(input: TokenStream) -> TokenStream {
    validate_span_declaration(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn validate_span_declaration(input: TokenStream2) -> syn::Result<TokenStream2> {
    let span: SpanArgs = syn::parse2(input)?;
    if span.entry.is_none() {
        return Err(syn::Error::new(
            Span::call_site(),
            specdrs_span_requires_entry(),
        ));
    }
    Ok(TokenStream2::new())
}

fn validate_module_memberships(input: TokenStream2) -> syn::Result<TokenStream2> {
    let args: SpecdrsArgs = syn::parse2(input)?;
    if args.directives.is_empty()
        || args
            .directives
            .iter()
            .any(|directive| !matches!(directive, Directive::InSpans(_)))
    {
        return Err(syn::Error::new(
            Span::call_site(),
            specdrs_module_requires_in_spans(),
        ));
    }
    Ok(TokenStream2::new())
}

fn expand(attribute: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let mut annotations = vec![syn::parse2(attribute)?];
    let (attributes, body) = split_item(item)?;
    let mut retained = Vec::new();

    for attribute in attributes {
        if is_specdrs(&attribute) {
            annotations.push(parse_attribute(&attribute)?);
        } else {
            retained.push(attribute);
        }
    }

    let is_impl = syn::parse2::<syn::ItemImpl>(body.clone()).is_ok();
    if is_impl
        && annotations
            .iter()
            .flat_map(|annotation| &annotation.directives)
            .any(|directive| matches!(directive, Directive::Claims(_)))
    {
        return Err(syn::Error::new(Span::call_site(), impl_cannot_own_claims()));
    }

    let Some(documentation) = render_documentation(&annotations, is_impl) else {
        return Ok(quote! { #(#retained)* #body });
    };
    let documentation = LitStr::new(&documentation, Span::call_site());
    Ok(quote! {
        #(#retained)*
        #[doc = #documentation]
        #body
    })
}

fn split_item(item: TokenStream2) -> syn::Result<(Vec<Attribute>, TokenStream2)> {
    let parser = |input: ParseStream<'_>| {
        let attributes = input.call(Attribute::parse_outer)?;
        let body = input.parse()?;
        Ok((attributes, body))
    };
    parser.parse2(item)
}

fn is_specdrs(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "specdrs")
}

fn parse_attribute(attribute: &Attribute) -> syn::Result<SpecdrsArgs> {
    let Meta::List(list) = &attribute.meta else {
        return Err(syn::Error::new_spanned(
            attribute,
            specdrs_requires_arguments(),
        ));
    };
    syn::parse2(list.tokens.clone())
}

fn render_documentation(annotations: &[SpecdrsArgs], is_impl: bool) -> Option<String> {
    let mut memberships = Vec::new();
    let mut seen_memberships = BTreeSet::new();
    let mut spans = Vec::new();
    let mut item_claims = Vec::new();

    for annotation in annotations {
        for directive in &annotation.directives {
            match directive {
                Directive::Span(span) => spans.push(span),
                Directive::InSpans(ids) => {
                    for id in ids {
                        if seen_memberships.insert(id) {
                            memberships.push(id.as_str());
                        }
                    }
                }
                Directive::Claims(claims) if !claims.claims.is_empty() => {
                    item_claims.push(claims);
                }
                Directive::Claims(_) => {}
            }
        }
    }

    if memberships.is_empty() && spans.is_empty() && item_claims.is_empty() {
        return None;
    }

    let mut output = String::from("## specdrs\n\n");
    if !memberships.is_empty() {
        output.push_str("Member of spans: ");
        output.push_str(&format_ids(&memberships));
        output.push_str(".\n\n");
    }
    if !spans.is_empty() {
        output.push_str("Declares spans: ");
        output.push_str(&format_ids(
            &spans
                .iter()
                .map(|span| span.id.as_str())
                .collect::<Vec<_>>(),
        ));
        output.push_str(".\n\n");
        if !is_impl {
            output.push_str("This item is a member of every span it declares.\n\n");
        }
    }

    if is_impl {
        output.push_str("Every item in this block is a member.\n\n");
    }

    for span in spans {
        render_span_claims(&mut output, span);
    }
    if !item_claims.is_empty() {
        output.push_str("### Item claims\n\n");
        for claims in item_claims {
            render_claims(&mut output, claims);
        }
    }

    Some(output.trim_end().to_owned())
}

fn render_span_claims(output: &mut String, span: &SpanArgs) {
    let Some(claims) = span
        .claims
        .as_ref()
        .filter(|claims| !claims.claims.is_empty())
    else {
        return;
    };
    output.push_str("### Span `");
    output.push_str(&escape_code(&span.id));
    output.push_str("` claims\n\n");
    render_claims(output, claims);
}

fn render_claims(output: &mut String, claims: &ClaimsArgs) {
    let mut previous_group = None;
    for claim in &claims.claims {
        let group = (claim.kind, claim.axis);
        if previous_group != Some(group) {
            output.push_str("#### ");
            output.push_str(&claim.kind.to_string());
            output.push_str(", ");
            output.push_str(&claim.axis.to_string());
            output.push_str("\n\n");
            previous_group = Some(group);
        }
        render_claim(output, claim);
    }
}

fn render_claim(output: &mut String, claim: &ClaimArgs) {
    output.push_str("- ");
    output.push_str(&claim.text.replace('\n', "\n  "));
    output.push('\n');
    for evidence in &claim.evidence {
        output.push_str("  - Evidence: `");
        output.push_str(&evidence.kind.to_string());
        output.push_str(" = ");
        output.push_str(&escape_code(&format_rust_tokens(&evidence.binder)));
        output.push_str("`\n");
    }
    output.push('\n');
}

fn format_ids(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| format!("`{}`", escape_code(id)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn escape_code(value: &str) -> String {
    value.replace('`', "\\`")
}

fn format_rust_tokens(value: &str) -> String {
    let mut value = value.to_owned();
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
        value = value.replace(from, to);
    }
    value
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{Expr, ExprLit, ItemFn, Lit};

    use super::*;

    #[test]
    fn appends_claims_and_evidence_after_authored_docs() {
        let expanded = expand(
            quote! {
                in_spans("checkout"),
                claims(
                    Constraints(
                        Invariants("Amount must be positive." as positive_amount),
                    ),
                    evidence(
                        positive_amount(Test = crate::tests::positive_amount),
                    ),
                )
            },
            quote! {
                #[doc = "Capture a payment."]
                pub fn capture() {}
            },
        )
        .expect("valid attribute should expand");
        let item: ItemFn = syn::parse2(expanded).expect("expanded function should parse");
        let docs = doc_values(&item.attrs);

        assert_eq!(docs[0], "Capture a payment.");
        assert_eq!(
            docs[1],
            concat!(
                "## specdrs\n\n",
                "Member of spans: `checkout`.\n\n",
                "### Item claims\n\n",
                "#### Constraint, Invariants\n\n",
                "- Amount must be positive.\n",
                "  - Evidence: `Test = crate::tests::positive_amount`",
            )
        );
    }

    #[test]
    fn combines_attributes_without_duplicate_sections() {
        let expanded = expand(
            quote! {
                span(
                    id = "checkout",
                    claims(Objectives(Job("Charge the customer." as purpose))),
                )
            },
            quote! {
                #[specdrs(
                    claims(Constraints(Interface("Accept cents." as accepts_cents)))
                )]
                pub fn charge() {}
            },
        )
        .expect("valid attributes should expand");
        let item: ItemFn = syn::parse2(expanded).expect("expanded function should parse");
        let docs = doc_values(&item.attrs);

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].matches("## specdrs").count(), 1);
        assert!(docs[0].contains("Declares spans: `checkout`."));
        assert!(docs[0].contains("This item is a member of every span it declares."));
        assert!(docs[0].contains("### Span `checkout` claims"));
        assert!(docs[0].contains("### Item claims"));
        assert!(!item.attrs.iter().any(is_specdrs));
    }

    #[test]
    fn rejects_invalid_syntax() {
        let error = expand(
            quote! { claims(Unknown(Job("No." as invalid))) },
            quote! { fn invalid() {} },
        )
        .expect_err("invalid syntax should fail expansion");

        assert!(error.to_string().contains("unknown claims group"));
    }

    #[test]
    fn membership_only_attributes_still_generate_docs() {
        let expanded = expand(
            quote! { in_spans("checkout", "payments") },
            quote! { fn member() {} },
        )
        .expect("membership should expand");
        let item: ItemFn = syn::parse2(expanded).expect("expanded function should parse");

        assert_eq!(
            doc_values(&item.attrs),
            ["## specdrs\n\nMember of spans: `checkout`, `payments`."]
        );
    }

    #[test]
    fn module_memberships_accept_only_in_spans() {
        assert!(validate_module_memberships(quote! { in_spans("checkout", "payments") }).is_ok());
        let error = validate_module_memberships(quote! {
            claims(Constraints(Job("Wrong scope." as wrong_scope)))
        })
        .expect_err("module declarations must contain only memberships");
        assert!(error.to_string().contains("requires one or more in_spans"));
    }

    #[test]
    fn impl_hosts_declare_spans_and_reject_claims() {
        let expanded = expand(
            quote! { span(id = "gateway", entry = self::Gateway::send) },
            quote! { impl Gateway { pub fn send(&self) {} } },
        )
        .expect("an impl host should expand");
        let item: syn::ItemImpl = syn::parse2(expanded).expect("the expansion is still an impl");
        let docs = doc_values(&item.attrs);
        assert!(
            docs[0].contains("Declares spans: `gateway`."),
            "{}",
            docs[0]
        );
        assert!(
            docs[0].contains("Every item in this block is a member."),
            "{}",
            docs[0]
        );
        assert!(!docs[0].contains("This item is a member"), "{}", docs[0]);

        let error = expand(
            quote! { claims(Constraints(Job("No owner." as no_owner))) },
            quote! { impl Gateway {} },
        )
        .expect_err("an impl block cannot own claims");
        assert!(
            error
                .to_string()
                .contains("an impl block cannot own claims"),
            "{error}"
        );
    }

    #[test]
    fn span_declarations_require_an_entry() {
        assert!(
            validate_span_declaration(quote! {
                id = "checkout",
                parent = "payments",
                entry = crate::checkout::run,
                claims(Constraints(Job("Charge once." as charge_once)))
            })
            .is_ok()
        );
        let error = validate_span_declaration(quote! { id = "checkout" })
            .expect_err("a free-standing span declaration must name its entry");
        assert!(error.to_string().contains("requires `entry`"));
        assert!(
            validate_span_declaration(quote! { entry = crate::checkout::run }).is_err(),
            "a span declaration still requires `id`"
        );
    }

    fn doc_values(attributes: &[Attribute]) -> Vec<String> {
        attributes
            .iter()
            .filter_map(|attribute| {
                if !attribute.path().is_ident("doc") {
                    return None;
                }
                let Meta::NameValue(meta) = &attribute.meta else {
                    return None;
                };
                let Expr::Lit(ExprLit {
                    lit: Lit::Str(value),
                    ..
                }) = &meta.value
                else {
                    return None;
                };
                Some(value.value())
            })
            .collect()
    }
}
