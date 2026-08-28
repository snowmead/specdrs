# specdrs design

This crate reads `#[specdrs(...)]` attributes from Rust source and emits a JSON map. The map names semantic spans, claims, evidence, Rust item paths, and exact source ranges.

Rust remains the program. The generated JSON is disposable output for people, agents, and other tools.

## model

A span is a named semantic cut across Rust items. It has one reading entrypoint, optional parent, explicit direct members, and optional claims. A Rust item may belong to several spans and may own its own claims.

A claim has four fields:

- A stable alias within its claim block.
- One proposition of prose.
- One kind: objective, constraint, or assumption.
- One of twelve axes.

Objectives have no rank. Every objective is expected to hold. Source order is preserved inside each kind and axis group for human reading. It is not emitted as a separate rank field.

The axes are `Job`, `Interface`, `Effects`, `Invariants`, `Assumptions`, `State`, `Time`, `Failure`, `Resources`, `Authority`, `Observation`, and `Change`.

Each axis is `Specified`, `NotApplicable`, or `Unspecified`. `NotApplicable` requires a reason. `Unspecified` means nobody accounted for that axis.

## attribute syntax

Claims are organized by kind first, then axis. Each kind and axis pair appears once per block. A block whose groups are out of order, or which repeats a kind and axis pair, is rejected during parsing rather than reordered.

```rust
#[specdrs(
    span(
        id = "checkout",
        entrypoint = crate::checkout::run,
        claims(
            Objectives(
                Job(
                    "Complete one commercial checkout." as complete_checkout,
                    "Return a stable receipt to the caller." as stable_receipt,
                ),
            ),
            Constraints(
                Invariants(
                    "One idempotency key creates at most one ledger row." as one_ledger_row,
                ),
                Authority(
                    "Only the owning customer may submit payment." as owning_customer,
                ),
            ),
            Assumptions(
                Assumptions(
                    "The payment provider preserves idempotency keys." as provider_idempotency,
                ),
            ),
            NotApplicable(
                State = "Checkout retains no process-local state.",
            ),
            evidence(
                one_ledger_row(Test = crate::tests::duplicate_request),
                owning_customer(Test = crate::tests::wrong_customer),
            ),
        ),
    ),
)]
pub fn run(request: Request) -> Result<Receipt, Error> {
    // implementation
}
```

An attribute declaration makes its addressable host a direct member. Its resolved
entrypoint is also a direct member:

```rust
#[specdrs(span(id = "checkout", entrypoint = crate::checkout::run))]
mod checkout {}
```

This adds both `checkout` and `checkout::run`. `entrypoint` defaults to the host item's
def path, so the same item appears only once when no explicit entrypoint is given. Use
`specdrs_span!` instead of a dummy documentation host when no host should join.

Members and item claims are compact:

```rust
#[specdrs(
    in_spans("checkout", "payments"),
    claims(
        Constraints(
            Failure(
                "A provider timeout is returned as retryable." as timeout_is_retryable,
            ),
        ),
    ),
)]
fn capture() {}
```

One declaration can apply memberships to every item in a Rust module:

```rust
use specdrs::prelude::*;

specdrs_module!(in_spans("checkout", "payments"));
```

The declaration applies to the containing module and every inline or file-backed
descendant. Nested module declarations append memberships. Item-level
`in_spans(...)` directives append more memberships, and repeated IDs collapse to
one membership. `specdrs_module!` accepts only `in_spans(...)` directives.

A span can also be declared on a container, and the items inside that container
become its members:

```rust
#[specdrs(
    span(
        id = "gateway",
        parent = "checkout",
        entrypoint = self::Gateway::send,
        claims(
            Constraints(
                Interface("Every member takes the gateway by shared reference." as shared_reference_members),
            ),
        ),
    )
)]
impl Gateway {
    pub fn send(&self) {}
    pub fn retry(&self) {}
}
```

Both methods are members, including one carrying no attribute of its own. This lets a
claim describe a grouping rather than an item: "every member is crate-private" belongs
to neither the implemented type nor any single method. A host-free declaration plus
`in_spans` on each member states the same kind of claim when the grouping crosses a
container boundary.

`in_spans(...)` on a container distributes the same way and composes with the
declared span, so one method can gather memberships from its block, its module,
and its own attribute. An `impl` block has no name and no def path, so its
declaration names `entrypoint` explicitly; a `mod` supplies its own def path, becomes a
member, and supplies the default entrypoint.

An `impl` block owns no claims. A bare `claims(...)` there is rejected by both the
proc macro and the scanner, because a claim needs an addressable owner and an impl
block has no identity to be one. Declare `span(...)` on the block instead, or move
the claims to the implemented type or one method.

A container that seeds at least one member accepts no member from outside itself.
Cross-boundary grouping belongs to a span declared to be cross-boundary, which both
sides reference:

```text
wrong                                right
impl Foo  declares S                 specdrs_span!(id = T, ...)
  fn a  ─────► S                     impl Foo  declares S, parent = T
                                       fn a ─────► S ─────► T
other::b  ──► S   (in_spans)         other::b ──► T   (in_spans)
```

An `impl` that seeds no methods contributes no container members because it has no
def path. Its entrypoint still joins and its span stays open. Use `specdrs_span!` instead
when the empty block would exist only to carry the declaration.

A span can be declared without any host item:

```rust
use specdrs::prelude::*;

specdrs_span!(
    id = "checkout",
    entrypoint = crate::checkout::run,
    claims( /* same claim grammar as span(...) */ ),
);
```

Use this when no Rust item should carry the declaration. `entrypoint` is required,
because the invocation has no def path to default to; the same rule applies to a
span declared on an `impl` block. Relative paths in `entrypoint` and in evidence binders
resolve against the module containing the invocation.

The resolved entrypoint becomes a member of the span exactly as it does for an
attribute declaration. The invocation has no host and contributes no Rust item, so
nothing synthetic appears in `items`.

Rules enforced by the parser and map builder:

- One span declaration for each span ID, whether from an attribute or `specdrs_span!`.
- Every span entrypoint resolves to a scanned Rust item.
- An attribute-declared span makes its addressable host and resolved entrypoint direct members.
- A declaration with no host def path names its entrypoint explicitly: `specdrs_span!` and any `impl` block.
- A span declared on a container applies to every item inside that container.
- A container that seeded members accepts no member from outside itself.
- An `impl` block owns no claims.
- Every parent resolves and parent links contain no cycles.
- At most one `claims(...)` block per Rust item.
- Container, module-wide, and item-level memberships are combined recursively.
- Each claim kind and each axis appears once per claim block.
- Claim aliases are unique inside a block.
- Evidence refers to a claim alias in the same block.
- Each item contains its Rust def path and exact source range.

The source scanner and proc macro use the same parser. Syntax errors therefore fail `cargo check` as well as `cargo specdrs check`.

## rustdoc hovers

The attribute macro appends an `## specdrs` rustdoc section to its item. Rust-analyzer displays that section on hover. Authored `///` documentation stays first.

The generated section contains locally authored metadata:

- Direct `in_spans(...)` memberships.
- Spans declared by that attribute.
- Automatic membership of a non-impl host in every span it declares.
- Claims owned by the item or by a span declared on the item.
- Authored evidence kinds and binders.

The macro does not read other source files. A member hover names its spans but does not copy claims declared on another item or inherited from a parent span. Use `cargo specdrs show <item>` for that projected view.

`specdrs_module!` and `specdrs_span!` emit no Rust item. Memberships and
span claims declared through them appear in the generated map and CLI output, not in any
rustdoc hover.

A declaration on an `impl` block does produce a hover. Rustdoc renders it on the
implemented type's page inside that block's section, and on the trait's page for a
trait implementation. The generated section's heading depth follows its host, so it
is not an `h3` outside a standalone item page.

Evidence in rustdoc is unresolved source metadata. The cargo command resolves a binder to `Linked` or `Unavailable` and neither result appears in hover text. `Passed` and `Failed` are declared for a future evidence runner; nothing computes them today, because no command executes evidence.

## JSON map

The emitted map uses schema `3`:

```json
{
  "schema": 3,
  "crate": "payments",
  "spans": [
    {
      "id": "checkout",
      "parent": null,
      "entrypoint": "payments::checkout::run",
      "members": ["payments::checkout::run"],
      "axes": { "Job": { "status": "Specified", "claims": [] }, "...": "all twelve axes" }
    }
  ],
  "items": {
    "payments::checkout::run": {
      "source": {
        "file": "src/checkout.rs",
        "start": { "line": 12, "column": 0 },
        "end": { "line": 38, "column": 1 }
      },
      "signature": "pub fn run(request: Request) -> Result<Receipt, Error>",
      "spans": ["checkout"],
      "axes": { "Job": { "status": "Unspecified", "claims": [] }, "...": "all twelve axes" }
    }
  }
}
```

`items` indexes every scanned Rust item -- every function, type, trait, module,
const, static, and impl method in the selected lib target -- whether or not a span
claims it. Associated consts and types, `use` items, `macro_rules!` definitions, the
impl blocks themselves, and other targets are not scanned and so do not appear. `spans` are the intentional cuts, so `item.spans` is empty for
code nobody has grouped yet. Every resolved evidence binder names an emitted item.

The source range covers the complete Rust item. An agent can use the JSON as a map, jump to the file and lines, then inspect the implementation body.

## commands

```text
cargo specdrs how
cargo specdrs emit [--stdout | --output <path>] [--manifest-path <path>] [-p <package>]
cargo specdrs check [--manifest-path <path>] [-p <package>]
cargo specdrs show <span-or-item> [--group-by kind,axis,owner] [--json] [--manifest-path <path>] [-p <package>]
cargo specdrs analyze [--span <id>]... [--item <path>]... [--jobs <count>] [--json] [--manifest-path <path>] [-p <package>]
```

`how` prints the authoring guide: when to use each macro and how to compose spans
and claims. Parser and map-builder failures name the broken rule and the pattern
to write instead. `how` accepts no arguments.

`emit` with neither `--stdout` nor `--output` writes
`<package>/target/specdrs/<crate>.json`, creating that directory if needed.

`show` includes inherited parent-span claims for item queries. Its default projection is kind, axis, then owner. `--group-by` changes the projection without changing stored JSON. Its `--json` output carries its own schema number, `1`, independent of the map schema.

`check` validates syntax, span structure, references, and evidence resolution. It does not compare two builds, and it does not ask a model to judge prose against code.

`check` reports every claim without evidence, of every kind, and counts unspecified axes only for owners that engaged with at least one axis. An owner with no claims has twelve unspecified axes by construction, and counting those would report the complete item index as a completeness gap.

## analyzer

`analyze` is the semantic audit. It creates two distinct job types:

- A span job audits claims owned by that span against the source of that span's direct members. Descendant-span members are not included; each child span has its own job.
- An item job audits claims owned by that item against that item source, and supplies every claim inherited from its span chain as context.

An item job splits its claims in two. `claims` holds the item's own claims, which the implementation must satisfy. `context` holds the claims inherited from enclosing spans, ancestor-first. A context claim fails only when the implementation actively contradicts it: a member function is not required to satisfy the whole span objective by itself, and partial implementation is not a failure. An item that owns no claim is still audited against its context.

Requests use temperature `0`. The prompt tells the model that `#[specdrs]` attributes declare requirements and do not enforce them.

By default `analyze` audits every span and item with a claim. `--span` and `--item` are repeatable and select exactly what they name, with no expansion to members or ancestors. A selected target that is absent from the map, or that owns and inherits no claim, fails the run rather than being skipped.

The model must return one of:

- `pass`
- `fail`, with concrete findings
- `indeterminate`, with the missing context

Analysis reports use schema `2`. Each result has a typed `span` or `item` target, every audited source range, and a completed verdict or execution error.

A failure names `claim_conflict` when declared claims cannot all hold. It names `implementation_violation` when the Rust implementation contradicts a claim. Every finding must contain claim IDs, source ranges, and a reason. Missing context is indeterminate, not a failure. Any failure, indeterminate result, request error, timeout, or invalid model JSON makes the command exit nonzero.

Gemma output is checked for internal consistency. An `indeterminate` response that names a supplied claim ID and explicitly says the implementation violates, contradicts, or fails to enforce that claim is emitted as an `implementation_violation`. Every audited source range becomes the finding range, so a span job reports each direct member. This prevents a wrong enum choice from hiding a failure.

The checked-in configuration targets the user's local Gemma model:

```toml
[analyze]
provider = "ollama"
model = "gemma4:26b"
base_url = "http://localhost:11434"
max_concurrency = 1
timeout_seconds = 300
max_output_tokens = 8192
```

Package-specific settings override the workspace table:

```toml
[packages.payments.analyze]
model = "gemma4:26b"
max_concurrency = 2
```

The pipeline is:

```text
Rust source
  -> syntax scanner
  -> schema and reference validation
  -> schema 3 knowledge map with source ranges
  -> show projection or static check
  -> Ollama span and item audits
  -> pass, fail, indeterminate, or execution error report
```

The analyzer does not run during `how`, `emit`, `check`, or `show`. Only `cargo specdrs analyze` contacts Ollama.

## dogfood map

The crate documents itself with this span hierarchy:

```text
specdrs
├── attribute-parsing
├── knowledge-map-model
├── knowledge-map-build
│   ├── knowledge-map-build.source-scanning
│   ├── knowledge-map-build.map-assembly
│   ├── knowledge-map-build.item-identity
│   └── knowledge-map-build.evidence-resolution
├── claim-projection
├── semantic-analysis
└── command-line-interface
```

The root span is declared at the crate root with `specdrs_span!`, so no item hosts it.

Span claims describe subsystem behavior. Important functions also own narrower claims. Membership-only annotations identify supporting implementation without pretending every helper has a separate semantic contract.

`specdrs-syntax` owns the attribute grammar used by the source scanner and proc macro. The root crate keeps a small adapter so its dogfood map covers that parser boundary.

`specdrs-macros` is a separate proc-macro package. Rust does not allow a proc-macro crate to apply its own procedural attribute, so it cannot appear in its own generated map.

`specdrs-syntax` cannot appear either. Carrying the attribute would require depending on `specdrs-macros`, which already depends on it, so the graph would cycle. Its grammar reaches the map only through the adapter in `src/attribute.rs`, which is why `attribute-parsing`'s members are mirror types and conversions rather than the parser itself.

Every statement in the rustdoc hovers section above is implemented in one of these two crates, so no claim in the map can own it. The macro crate carries its own unit tests and `tests/rustdoc.rs` covers the rendered output. `specdrs-syntax` has no tests of its own: its grammar is exercised through the adapter's tests in `src/attribute.rs`.
