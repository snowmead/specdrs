# Engineer

Use this workflow to turn an idea into decisions, record those decisions as
engineering claims, plan the code changes, and check the implementation against
the claims. Load what the crate already declared before asking anything new.

## Split the work by semantic scope

A **span** is a stable feature or responsibility, such as `checkout` or
`checkout.refund`. A span may cross modules. It has a stable ID, one Rust item
as its reading entrypoint, optional parent, explicit member items, and claims that
apply to the whole semantic cut.

An **item** is one Rust item identified by its definition path. Item claims
apply only to that item. Calling a function does not make it a span member.
Membership is explicit. A span declared with `#[specdrs(span(...))]` makes its
addressable host and resolved entrypoint direct members. `specdrs_span!` has no host,
so only its resolved entrypoint joins automatically. An `impl` has no definition
path; its resolved entrypoint and methods become direct members instead.

Choose the smallest span that contains the decision. Split independent jobs
before grilling them. The human owns that scope choice.

## Model decisions as claims

Each claim has:

- a stable alias within its owner;
- one independently falsifiable proposition;
- one kind: objective, constraint, or assumption;
- one semantic axis.

The kinds mean:

- **Objective:** an end state the implementation must satisfy. Objectives are
  not ranked. Source order exists for reading, not priority.
- **Constraint:** a condition that must hold. A violation makes the design or
  implementation invalid.
- **Assumption:** a condition treated as true. If it is false, revisit the
  design.

Use only axes that expose a real decision:

- **Job:** the change in the world this work must cause because of some reason.
- **Interface:** the semantic data contract crossing a boundary.
- **Effects:** state changed outside the boundary.
- **Invariants:** what must always, never, or eventually hold.
- **Assumptions:** facts treated as true outside the boundary.
- **State:** what is remembered, who owns it, and for how long.
- **Time:** deadlines, frequency, ordering, and freshness.
- **Failure:** what breaks, how it is detected, and what happens next.
- **Resources:** money, compute, storage, or human time consumed, including how
  compute time and memory grow with input size.
- **Authority:** who may cause each effect.
- **Observation:** how success and failure are known.
- **Change:** requirements, data, callers, or deployment order expected to move.

Put algorithmic time and space complexity under Resources. Use Time for
latency, deadlines, frequency, ordering, and freshness. Use State for what is
remembered, who owns it, and how long it survives.

Do not restate a Rust signature, ownership rule, or `Result` type as prose.
Mark an axis not applicable only with a concrete reason. Leave an unexamined
axis unspecified.

## Load existing claims first

Do not grill, plan, or implement from a blank page when the crate already has
specdrs annotations. Those annotations are the current design record.

The CLI is the composed view. File reads are the implementation. Use both, in
that order. Hover and grep are not the composed contract: they miss parent-span
claims, `specdrs_span!` declarations in other files, and members enrolled by
`specdrs_module!` or a container.

1. Run `cargo specdrs emit --stdout` (or read the default map under
   `target/specdrs/`) to list spans, parents, entries, members, and which items
   already own claims. Do not paste the whole map into the conversation.
2. For each span or item in the work's scope, run
   `cargo specdrs show <span-or-item>`. That output is the obligation set:
   inherited parent-span claims plus local claims, ancestor-first.
3. Run `cargo specdrs check` for claims without evidence and unspecified axes
   on owners that already engaged.
4. Open the `source` file and line range `show` or `emit` printed, and read
   that body. That is the secondary tool. Do not reread the crate to rediscover
   memberships the map already computed.

Seed the working design record from that output. Existing claims are accepted
decisions until the human amends them. Do not re-ask an axis that already has a
claim. Do not propose a new claim that conflicts with one still in the source.
Name conflicts the same way as later in this file.

If emit fails because nothing is annotated yet, there is no prior specdrs
record. Grill from the code. If emit or check fails with a diagnostic, follow
it. Do not invent a workaround that disagrees with the map.

After writing or changing an annotation, `show` that owner again before treating
the composed view as current.

## Grill in decision rounds

Ask the human for decisions, not facts the repository already contains, and not
axes the map already specifies.

Build a dependency graph of open decisions. The frontier is every open decision
whose prerequisites are settled. Ask the complete frontier in one numbered
round. Do not put a question and a question that depends on its answer in the
same round.

For each question:

1. Name the span or item, axis, and proposed kind.
2. State the decision in concrete terms.
3. Recommend an answer and name the mechanism behind it.
4. Ask the human to accept, reject, or rewrite it.

Use this shape:

```text
Round 2
Frontier: checkout State, checkout Failure, capture Time

Q1. span:checkout / State / constraint
Decision: Is the idempotency record retained after a failed capture?
Recommendation: Keep it for 24 hours so a retry cannot create a second charge.

Q2. item:payments::stripe::capture / Time / objective
Decision: What retry latency must the capture path satisfy?
Recommendation: Complete 95 percent of retries within 2 seconds.
```

After each answer, convert accepted decisions into atomic claims. Surface new
branches and conflicts. Then ask the next frontier.

Maintain a working design record in the conversation. Group it by owner, then
by `Objectives`, `Constraints`, and `Assumptions`. Group each kind by axis. Keep
claims in accepted order, but do not assign ranks. Record unresolved conflicts
and experiments beside the affected owner.

```text
span:checkout
  entrypoint: payments::checkout::run
  Objectives
    Job
      complete_checkout: Complete one commercial checkout.
  Constraints
    Invariants
      one_ledger_row: One idempotency key creates at most one ledger row.
  Assumptions
    Assumptions
      provider_idempotency: The provider preserves idempotency keys.

item:payments::stripe::capture
  Constraints
    Failure
      timeout_is_retryable: A provider timeout is returned as retryable.

conflicts: none
experiments: none
```

Update this record after every round. Keep loaded specdrs claims in it unless
the human amended them. Use its accepted state as the input to planning. Write
it to a separate file only when the user asks for a persisted design artifact.
During implementation, run `cargo specdrs how` and follow its instructions to
record the accepted claims beside the Rust items they govern. Then `show` the
owner.

A **conflict** is two claims that cannot both hold as written. Name both claim
aliases and the implementation mechanism that makes them conflict. Recommend a
side. The human decides whether to change a claim.

If the human accepts recommendations without qualification, test the design
with the strongest concrete counterexample. Do not manufacture disagreement.

## Stop questions that require evidence

"I don't know" is a valid answer. Keep the unknown as an explicit assumption
when the risk permits it. Otherwise name the smallest prototype, measurement,
or failure drill that would answer it and stop that branch.

Split the span when the session keeps growing without closing branches.

The design is ready when:

- in-scope claims were loaded from the map and either kept, amended, or named
  as conflicts;
- every hot branch has been visited;
- every accepted claim has an owner, alias, kind, axis, and one proposition;
- every conflict is resolved or recorded as unresolved;
- every remaining unknown is an assumption or a named experiment;
- cold axes remain unspecified or have a reason to be not applicable.

Do not keep grilling after these conditions hold. Move to planning when the user
asks for a plan. Move to implementation when the user asks to build it.

## Plan from the accepted claims

Start from the loaded specdrs record plus claims accepted in this session.
Write the implementation plan around claims, not files alone. For each plan
step, name:

- the claim aliases it satisfies;
- the span or item that owns them;
- the Rust items to add or change;
- the evidence to add or update;
- the command that proves the step is complete.

Order steps by dependency. Do not infer priority from objective order. Every
objective remains required.

When implementation starts, use the accepted claims as the contract. If code
forces a design change, stop and amend the affected claim with the human. Do
not silently make the code and annotation disagree.

## Use the specdrs CLI

Before writing specdrs macros or running any specdrs command, run:

```text
cargo specdrs how
```

Treat its output as the source of truth for how to author spans and claims.
If `cargo check` or `cargo specdrs check` fails, follow the diagnostic; it names
the failure and the pattern to write. Do not duplicate those instructions here.
Run `how` again when the installed CLI changes.
