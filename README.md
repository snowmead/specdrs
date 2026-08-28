# specdrs

[![Crates.io](https://img.shields.io/crates/v/specdrs.svg)](https://crates.io/crates/specdrs)
[![CI](https://github.com/snowmead/specdrs/actions/workflows/ci.yml/badge.svg)](https://github.com/snowmead/specdrs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Pronounced /ˈspɛkt ɑːr ɛs/ (*specked-R-S*).

An agent that writes code will invent decisions you never agreed to.
`specdrs` is the loop that stops that.

Problems we face post AI:

- Agents leave choices and decisions either in external documentation or in archived transcripts, increasingly harder to mine and synthesize without ambiguity
- Agents are lazy and will not ask the right questions to make informed decisions
- Agents cannot infer the meaning and intent of code for big and long running projects without rigorous analysis and token spend
- Humans lose the ability to verify code being shipped and the correctness of the agent's decisions due to a lack of decision making on their part

What this achieves:

- Agents and Humans align on intent and objectives
- Agents can read the decisions made that shaped the code directly from the source
- Accepted decisions become claims in the crate, next to the code they govern

If you haven't guessed, this replaces the basic grill-me skill with an engineering focused approach.

The `specdrs` skill is language-agnostic but it works best in the supported languages:

- Rust

Run `/specdrs` so the agent follows that loop for a design, a feature, or a bug
fix.

## Primitive decisions

The [engineer](.agents/skills/specdrs/references/engineer.md) reference treats
kinds and axes as the questions an engineer actually has to answer. A claim is
one falsifiable proposition with a kind and an axis. The agent recommends. The
human accepts, rejects, or rewrites. Nothing becomes a claim until then.

Kinds:

- **Objective:** an end state the implementation must satisfy. Objectives are
  not ranked.
- **Constraint:** a condition that must hold. A violation invalidates the
  design or the code.
- **Assumption:** a condition treated as true. If it is false, revisit.

Axes, each one a real decision:

- **Job:** the change in the world this work must cause.
- **Interface:** the semantic data contract crossing a boundary.
- **Effects:** state changed outside the boundary.
- **Invariants:** what must always, never, or eventually hold.
- **Assumptions:** facts treated as true outside the boundary.
- **State:** what is remembered, who owns it, and for how long.
- **Time:** deadlines, frequency, ordering, and freshness.
- **Failure:** what breaks, how it is detected, and what happens next.
- **Resources:** money, compute, storage, or human time, including how compute
  and memory grow.
- **Authority:** who may cause each effect.
- **Observation:** how success and failure are known.
- **Change:** requirements, data, callers, or deployment order expected to
  move.

Each axis on a span or item is specified, not applicable with a reason, or
unspecified. The agent does not fill an unspecified axis on its own.

A **span** is a named cut of responsibility, such as `checkout`. An **item** is
one Rust definition. Claims hang on either. **Evidence** points at the test,
type, lint, or proof that would falsify the claim.

## What it looks like

```rust
use specdrs::prelude::*;

specdrs_span!(
    id = "checkout",
    entrypoint = crate::checkout::run,
    claims(
        Objectives(
            Job(
                "Complete one commercial checkout." as complete_checkout,
            ),
        ),
        Constraints(
            Invariants(
                "One idempotency key creates at most one ledger row." as one_ledger_row,
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
        ),
    ),
);

#[specdrs(
    in_spans("checkout"),
    claims(
        Constraints(
            Failure(
                "A provider timeout is returned as retryable." as timeout_is_retryable,
            ),
        ),
        evidence(
            timeout_is_retryable(Test = crate::tests::provider_timeout),
        ),
    ),
)]
fn capture() {}
```

Claims nest kind, then axis. `Assumptions(Assumptions(...))` is kind
assumption on the Assumptions axis: a fact treated as true outside the
boundary. It is not a duplicate wrapper.

The scanner compiles that into this span tree. Claims hang on the span or on
a member item. Unspecified axes stay visible.

```text
checkout
├─ claims
│  ├─ Job / objective           complete_checkout
│  ├─ Invariants / constraint   one_ledger_row  → Test tests::duplicate_request
│  ├─ Assumptions / assumption  provider_idempotency
│  └─ State / n/a               no process-local state
├─ checkout::run                entrypoint
└─ capture
   └─ Failure / constraint      timeout_is_retryable  → Test tests::provider_timeout
```

`cargo specdrs how` is the authoring guide. Invalid annotations fail `cargo
check` and `cargo specdrs check`.

## Install

```console
bunx skills add snowmead/specdrs --skill specdrs
cargo install specdrs --locked
cargo add specdrs
```

Run `/specdrs` in an agent session. The skill loads:

- [`engineer.md`](.agents/skills/specdrs/references/engineer.md): grill
  decisions, record accepted claims, plan the change, and check the
  implementation against those claims.
- [`rust.md`](.agents/skills/specdrs/references/rust.md): default Rust
  conventions when the repository is silent.

## Commands

| Command | What it does | Contacts a model |
| --- | --- | --- |
| `how` | Prints the authoring guide. | No |
| `emit` | Writes the JSON map to `target/specdrs/<crate>.json`. | No |
| `check` | Validates syntax, span structure, references, and evidence. | No |
| `show` | Projects claims for one span or item, including inherited claims. | No |
| `analyze` | Compares claims with the selected source using `specdrs.toml`. | Yes |

`check` does not prove that prose matches code. Attributes declare
requirements. They do not enforce them. `analyze` is the optional semantic
audit.

Parser rules, map invariants, and analyzer behavior live in
[the design document](docs/DESIGN.md). Maintainers publishing crates should
read [the release process](docs/RELEASE.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
