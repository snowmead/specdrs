use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Result, Token, TypePath, parenthesized};

mod diagnostics;
pub use diagnostics::*;

#[derive(Debug, Clone)]
pub struct SpecdrsArgs {
    pub directives: Vec<Directive>,
}

#[derive(Debug, Clone)]
pub enum Directive {
    Span(SpanArgs),
    InSpans(Vec<String>),
    Claims(ClaimsArgs),
}

#[derive(Debug, Clone)]
pub struct SpanArgs {
    pub id: String,
    pub parent: Option<String>,
    pub entry: Option<String>,
    pub claims: Option<ClaimsArgs>,
}

#[derive(Debug, Clone)]
pub struct ClaimsArgs {
    pub claims: Vec<ClaimArgs>,
    pub not_applicable: Vec<NotApplicableArgs>,
}

#[derive(Debug, Clone)]
pub struct ClaimArgs {
    pub id: String,
    pub axis: Axis,
    pub kind: ClaimKind,
    pub text: String,
    pub evidence: Vec<EvidenceArgs>,
}

#[derive(Debug, Clone)]
pub struct EvidenceArgs {
    pub kind: EvidenceKind,
    pub binder: String,
}

#[derive(Debug, Clone)]
pub struct NotApplicableArgs {
    pub axis: Axis,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    Job,
    Interface,
    Effects,
    Invariants,
    Assumptions,
    State,
    Time,
    Failure,
    Resources,
    Authority,
    Observation,
    Change,
}

impl Axis {
    const ALL: [Self; 12] = [
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
}

impl fmt::Display for Axis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl FromStr for Axis {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|axis| axis.to_string() == value)
            .ok_or_else(|| unknown_axis(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    Objective,
    Constraint,
    Assumption,
}

impl fmt::Display for ClaimKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceKind {
    Type,
    Test,
    Fuzz,
    Proof,
    Lint,
}

impl fmt::Display for EvidenceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

struct RawClaim {
    id: String,
    axis: Axis,
    kind: ClaimKind,
    text: String,
}

impl Parse for SpecdrsArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut directives = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let content;
            parenthesized!(content in input);
            match name.to_string().as_str() {
                "span" => directives.push(Directive::Span(content.parse()?)),
                "in_spans" => directives.push(Directive::InSpans(parse_span_ids(&content)?)),
                "claims" => directives.push(Directive::Claims(content.parse()?)),
                _ => {
                    return Err(syn::Error::new(name.span(), unknown_directive()));
                }
            }
            parse_comma(input)?;
        }
        Ok(Self { directives })
    }
}

impl Parse for SpanArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut id = None;
        let mut parent = None;
        let mut entry = None;
        let mut claims = None;

        while !input.is_empty() {
            let name: Ident = input.parse()?;
            match name.to_string().as_str() {
                "id" if id.is_none() => id = Some(parse_string_assignment(input, &name)?),
                "parent" if parent.is_none() => {
                    parent = Some(parse_string_assignment(input, &name)?);
                }
                "entry" if entry.is_none() => {
                    input.parse::<Token![=]>()?;
                    let path: TypePath = input.parse()?;
                    entry = Some(path.to_token_stream().to_string());
                }
                "claims" if claims.is_none() => {
                    let content;
                    parenthesized!(content in input);
                    claims = Some(content.parse()?);
                }
                "id" | "parent" | "entry" | "claims" => {
                    return Err(syn::Error::new(name.span(), duplicate_span_field()));
                }
                _ => return Err(syn::Error::new(name.span(), unknown_span_field())),
            }
            parse_comma(input)?;
        }

        Ok(Self {
            id: id.ok_or_else(|| input.error(span_requires_id()))?,
            parent,
            entry,
            claims,
        })
    }
}

impl Parse for ClaimsArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut raw_claims = Vec::new();
        let mut not_applicable = Vec::new();
        let mut evidence = BTreeMap::<String, Vec<EvidenceArgs>>::new();
        let mut seen_groups = BTreeSet::new();
        let mut stage = 0_u8;

        while !input.is_empty() {
            let name: Ident = input.parse()?;
            let group = name.to_string();
            let content;
            parenthesized!(content in input);
            match group.as_str() {
                "Objectives" => {
                    require_group(&mut seen_groups, &name, &group, stage, 0)?;
                    raw_claims.extend(parse_kind_group(&content, ClaimKind::Objective)?);
                }
                "Constraints" => {
                    require_group(&mut seen_groups, &name, &group, stage, 1)?;
                    stage = 1;
                    raw_claims.extend(parse_kind_group(&content, ClaimKind::Constraint)?);
                }
                "Assumptions" => {
                    require_group(&mut seen_groups, &name, &group, stage, 2)?;
                    stage = 2;
                    raw_claims.extend(parse_kind_group(&content, ClaimKind::Assumption)?);
                }
                "NotApplicable" => {
                    require_group(&mut seen_groups, &name, &group, stage, 3)?;
                    stage = 3;
                    not_applicable = parse_not_applicable_group(&content)?;
                }
                "evidence" => {
                    require_group(&mut seen_groups, &name, &group, stage, 4)?;
                    stage = 4;
                    evidence = parse_evidence_group(&content)?;
                }
                _ => return Err(syn::Error::new(name.span(), unknown_claims_group())),
            }
            parse_comma(input)?;
        }

        let aliases: BTreeSet<_> = raw_claims.iter().map(|claim| claim.id.as_str()).collect();
        if aliases.len() != raw_claims.len() {
            return Err(input.error(unique_claim_aliases()));
        }
        if let Some(unknown) = evidence
            .keys()
            .find(|alias| !aliases.contains(alias.as_str()))
        {
            return Err(input.error(unknown_evidence_alias(unknown)));
        }

        let claims = raw_claims
            .into_iter()
            .map(|claim| ClaimArgs {
                evidence: evidence.remove(&claim.id).unwrap_or_default(),
                id: claim.id,
                axis: claim.axis,
                kind: claim.kind,
                text: claim.text,
            })
            .collect();
        Ok(Self {
            claims,
            not_applicable,
        })
    }
}

fn require_group(
    seen: &mut BTreeSet<String>,
    name: &Ident,
    group: &str,
    current_stage: u8,
    group_stage: u8,
) -> Result<()> {
    if group_stage < current_stage {
        return Err(syn::Error::new(name.span(), claims_group_order()));
    }
    if !seen.insert(group.to_owned()) {
        return Err(syn::Error::new(name.span(), duplicate_claims_group()));
    }
    Ok(())
}

fn parse_kind_group(input: ParseStream<'_>, kind: ClaimKind) -> Result<Vec<RawClaim>> {
    let mut claims = Vec::new();
    let mut axes = BTreeSet::new();
    while !input.is_empty() {
        let axis_name: Ident = input.parse()?;
        let axis = Axis::from_str(&axis_name.to_string())
            .map_err(|message| syn::Error::new(axis_name.span(), message))?;
        if !axes.insert(axis) {
            return Err(syn::Error::new(
                axis_name.span(),
                duplicate_axis_in_kind_group(),
            ));
        }
        let content;
        parenthesized!(content in input);
        if content.is_empty() {
            return Err(content.error(empty_axis_group()));
        }
        while !content.is_empty() {
            let text: LitStr = content.parse()?;
            content.parse::<Token![as]>()?;
            let id: Ident = content.parse()?;
            claims.push(RawClaim {
                id: id.to_string(),
                axis,
                kind,
                text: text.value(),
            });
            parse_comma(&content)?;
        }
        parse_comma(input)?;
    }
    if claims.is_empty() {
        return Err(input.error(empty_kind_group()));
    }
    Ok(claims)
}

fn parse_not_applicable_group(input: ParseStream<'_>) -> Result<Vec<NotApplicableArgs>> {
    let mut entries = Vec::new();
    let mut axes = BTreeSet::new();
    while !input.is_empty() {
        let axis_name: Ident = input.parse()?;
        let axis = Axis::from_str(&axis_name.to_string())
            .map_err(|message| syn::Error::new(axis_name.span(), message))?;
        if !axes.insert(axis) {
            return Err(syn::Error::new(
                axis_name.span(),
                duplicate_not_applicable_axis(),
            ));
        }
        let reason = parse_string_assignment(input, &axis_name)?;
        entries.push(NotApplicableArgs { axis, reason });
        parse_comma(input)?;
    }
    if entries.is_empty() {
        return Err(input.error(empty_not_applicable()));
    }
    Ok(entries)
}

fn parse_evidence_group(input: ParseStream<'_>) -> Result<BTreeMap<String, Vec<EvidenceArgs>>> {
    let mut entries = BTreeMap::new();
    while !input.is_empty() {
        let alias: Ident = input.parse()?;
        let content;
        parenthesized!(content in input);
        if entries.contains_key(&alias.to_string()) {
            return Err(syn::Error::new(alias.span(), duplicate_evidence_alias()));
        }
        let mut links = Vec::new();
        let mut unique = BTreeSet::new();
        while !content.is_empty() {
            let link = parse_evidence(&content)?;
            if !unique.insert((link.kind, link.binder.clone())) {
                return Err(content.error(duplicate_evidence_link()));
            }
            links.push(link);
            parse_comma(&content)?;
        }
        if links.is_empty() {
            return Err(content.error(empty_evidence_alias()));
        }
        entries.insert(alias.to_string(), links);
        parse_comma(input)?;
    }
    if entries.is_empty() {
        return Err(input.error(empty_evidence_group()));
    }
    Ok(entries)
}

fn parse_evidence(input: ParseStream<'_>) -> Result<EvidenceArgs> {
    let value: Ident = input.parse()?;
    let kind = match value.to_string().as_str() {
        "Type" => EvidenceKind::Type,
        "Test" => EvidenceKind::Test,
        "Fuzz" => EvidenceKind::Fuzz,
        "Proof" => EvidenceKind::Proof,
        "Lint" => EvidenceKind::Lint,
        _ => return Err(syn::Error::new(value.span(), unknown_evidence_kind())),
    };
    input.parse::<Token![=]>()?;
    let binder: TypePath = input.parse()?;
    Ok(EvidenceArgs {
        kind,
        binder: binder.to_token_stream().to_string(),
    })
}

fn parse_span_ids(input: ParseStream<'_>) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    while !input.is_empty() {
        let id: LitStr = input.parse()?;
        ids.push(id.value());
        parse_comma(input)?;
    }
    if ids.is_empty() {
        return Err(input.error(empty_in_spans()));
    }
    Ok(ids)
}

fn parse_string_assignment(input: ParseStream<'_>, name: &Ident) -> Result<String> {
    input.parse::<Token![=]>()?;
    let value: LitStr = input
        .parse()
        .map_err(|_| syn::Error::new(name.span(), string_literal_required(&name.to_string())))?;
    Ok(value.value())
}

fn parse_comma(input: ParseStream<'_>) -> Result<()> {
    if input.is_empty() {
        return Ok(());
    }
    input.parse::<Token![,]>()?;
    Ok(())
}
