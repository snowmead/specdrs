# Rust

Follow repository-local Rust conventions first. Apply these defaults where the
repository is silent. Do not mix unrelated cleanup into a behavioral change.

## Imports

Write grouped imports vertically. Put each imported item on its own line. Use
braces as soon as a list contains more than one item.

Add an empty trailing `//` comment after the final item inside each brace group
when needed to make `rustfmt` keep the vertical layout. This applies to nested
groups. Keep the comment on the last item unless moving it would create needless
churn during a patch series.

```rust
use crate::{
    example1,
    example2::{
        example3,
        example4,
        example5, //
    },
    example6,
    example7,
    example8::example9, //
};
```

Do not introduce condensed grouped imports. Existing code may not be migrated
yet. Treat the vertical layout as the default, with exceptions only when the
code provides a concrete reason.

## Modules define scope

A module owns one responsibility. Split it when it owns more. Evidence that it
does:

- two type clusters that never reference each other;
- free helpers used by only one section of the file;
- an `impl` block fragmented by unrelated intervening items.

Do not split by length. A long module whose every item serves one responsibility
stays one module, and a cohesive type does not get cut apart to make files
smaller.

Cut along a seam the code already has: a pipeline stage, an external format
being decoded, a type with its private helpers. Name each module after the
responsibility it owns. Do not name a module `types`, `utils`, `helpers`,
`common`, or `misc`, for the same reason `Utils` is a bad type name.

Do not repeat the module name in its items. Write `scan::Scanner`, not
`scan::ScanScanner`, the way the standard library writes `io::Error` rather than
`io::IoError`.

Declare submodules in `foo.rs` beside a `foo/` directory. Do not add `mod.rs`
under `src/`. This form is more consistent and keeps a project from filling up
with files named `mod.rs`. Declaring both `foo.rs` and `foo/mod.rs` is an error.

`tests/` is the exception. Cargo compiles every top-level file in `tests/` as
its own integration test binary, so `tests/common.rs` becomes a target that
reports `running 0 tests`. Put a shared test helper in `tests/common/mod.rs`;
files in subdirectories of `tests/` are not compiled as separate crates.

Give each module its own file instead of an inline `mod foo { ... }` block.
`#[cfg(test)] mod tests` is the exception, and it goes last in the file.

Enforce the layout rather than remembering it. Both lints below are
`restriction` lints, so `clippy::all` does not include them and each must be
named on its own. Never enable the `restriction` group as a whole; its lints
contradict each other.

```toml
[lints.clippy]
inline_modules = "warn"
mod_module_files = "warn"
```

A split must not widen an API. Carving one file into three tempts you to promote
private items to `pub` so the siblings can reach each other, which enlarges the
crate's public surface as a side effect of an internal move. Remove the
visibility from every item you move, then add back only what the compiler
demands. Enable `unreachable_pub` to catch what got away; it flags `pub` items
that nothing outside the crate can reach.

```toml
[lints.rust]
unreachable_pub = "warn"
```

Work with two levels, private and `pub(crate)`. Reach for `pub(super)` only
when it records a real parent-child relationship, and do not use
`pub(in path)`. Leave `clippy::redundant_pub_crate` off: it is a nursery lint
that argues the opposite direction from `unreachable_pub`, so enabling both
makes an item oscillate between `pub` and `pub(crate)`.

The parent module keeps the entry points, the domain types callers name, and
the re-exports that hold existing paths stable. Machinery moves down. Internal
structure must not dictate the public API, so keep the new submodules private
and re-export what callers need. A split then changes no caller's import, and
rustdoc still lists the type on the crate front page instead of burying it a
level down.

```rust
// checkout.rs
//! Runs one commercial checkout from cart to receipt.

mod pricing;
mod receipt;
mod tax;

pub use receipt::Receipt;

pub struct CheckoutService<R> {
    orders: R,
}

impl<R: OrderRepository> CheckoutService<R> {
    pub fn checkout(&self, order_id: OrderId) -> Result<Receipt, CheckoutError> {
        let order = self.orders.find(order_id)?;
        let priced = pricing::Priced::new(order)?;
        let assessed = tax::assess(priced)?;
        Receipt::issue(assessed)
    }
}
```

Define a type's inherent `impl` blocks in the module that defines the type. An
inherent `impl` in a calling module hides part of the type's API from whoever
reads the definition. When the operation serves only one caller, write it as a
private function in that caller instead of an inherent method.

Move `#[cfg(test)]` tests with the code they cover.

## Functions need owners

A module full of functions that take the same state or dependencies is missing
a type. Put behavior in an `impl` when it:

- reads or changes a type's state;
- preserves a type's invariants;
- constructs or transitions a type; or
- represents an operation primarily performed by that type.

Use associated constructors and transformations such as `Type::new`,
`Type::parse`, and `Type::from_parts`. Keep helpers private inside the owning
`impl` when they exist only to support that type.

When several operations share dependencies but have no existing receiver,
introduce a context or service that owns those dependencies. Repeated parameter
bundles are evidence of a missing type.

```rust
pub struct CheckoutService<R> {
    orders: R,
}

impl<R: OrderRepository> CheckoutService<R> {
    pub fn checkout(&self, order_id: OrderId) -> Result<Receipt, CheckoutError> {
        let order = self.orders.find(order_id)?;
        order.checkout()
    }
}
```

Do not invent an empty struct to create a namespace. Avoid vague owners such as
`Manager`, `Utils`, and `Helpers`. Name the state or capability, such as
`Parser`, `Planner`, `Store`, or `TransactionValidator`.

Keep a free function when the operation is stateless, symmetric across several
types, or a general algorithm with no honest receiver.

```rust
fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}
```

## Traits define boundaries

Introduce a trait when a consuming layer must depend on a capability without
naming the providing layer's concrete type. Define the trait near the consumer
unless it is a public domain contract. Make the lower-layer adapter implement
it, then inject that implementation through a constructor or explicit context.

```rust
pub trait OrderRepository {
    fn find(&self, order_id: OrderId) -> Result<Order, RepositoryError>;
}

pub struct PostgresOrderRepository {
    pool: PgPool,
}

impl OrderRepository for PostgresOrderRepository {
    fn find(&self, order_id: OrderId) -> Result<Order, RepositoryError> {
        self.pool.find_order(order_id)
    }
}
```

Use a trait for a real substitution point: multiple implementations, a test
substitute, a platform adapter, or an intentional dependency boundary. A trait
with one implementation is justified when the boundary prevents domain code
from depending on a database client, HTTP client, filesystem handle, SDK type,
or other external mechanism.

Keep traits narrow and capability-based. Do not mirror a concrete type's entire
inherent API or add speculative extension points. Do not add a trait merely
because a concrete type has methods.

Use a generic parameter for compile-time substitution when it stays local. Use
`dyn Trait` when runtime substitution, heterogeneous storage, or compile-time
isolation justifies dynamic dispatch. Seal a public trait when downstream
implementations are not part of the supported contract.

## Types and APIs

Keep visibility as narrow as possible. Treat `pub`, re-exports, and generic
types crossing crate boundaries as API design decisions.

Order files for top-down reading. Put public entry points and domain types
before private machinery. Group behavior with the state and invariants it
governs.

Accept borrowed forms such as `&str`, `&Path`, and `&[T]` instead of `&String`,
`&PathBuf`, and `&Vec<T>`. Take ownership only when the implementation stores,
consumes, or returns the value.

Use newtypes and enums when they eliminate invalid states, ambiguous units,
cross-domain identifier mixups, or boolean mode flags. Do not wrap a primitive
unless the wrapper gains an invariant or domain meaning.

Implement `Default` only when one value is genuinely the default. Do not hide a
dummy or invalid state behind `Default`.

## Use standard conversion traits

Before adding `to_*`, `from_*`, `into_*`, `as_*`, `convert_*`, or `parse_*`,
check whether the operation matches Rust's standard conversion vocabulary.

| Contract | Mechanism |
|---|---|
| Infallible owned conversion | `From` |
| Fallible owned conversion | `TryFrom` |
| Cheap borrowed view | `AsRef` or `AsMut` |
| Equivalent collection lookup view | `Borrow` |
| Canonical text parsing | `FromStr` |
| Collection construction or growth | `FromIterator` or `Extend` |
| Lossy, policy-driven, or contextual operation | Named method |

Implement `From<Source> for Target` when the conversion has one obvious
meaning, rejects no input, and loses no meaningful information. Implement
`TryFrom<Source> for Target` when validation, range checks, or malformed input
can reject the source.

Implement `From` and `TryFrom`, not `Into` and `TryInto`; the latter receive
blanket implementations. Use `Into`, `TryInto`, or `AsRef` as a parameter bound
only when callers genuinely need several input forms. Prefer a concrete
parameter for internal functions and APIs with one expected input type.

Use `AsRef` and `AsMut` only for cheap reference-to-reference views. Use
`Borrow` only when the borrowed and owned forms have equivalent `Eq`, `Ord`,
and `Hash` behavior and must work as collection lookup keys.

Implement `FromStr` for a type's canonical textual representation. Implement
`FromIterator` and `Extend` for collection-like types. Use `From` for direct
error conversions that preserve the source and make `?` propagation correct.

Keep a named method when a conversion is lossy, performs I/O, needs policy or
configuration, has several reasonable meanings, or may surprise the caller.
Name that operation honestly, such as `to_string_lossy`, `open`, `decode_with`,
or `into_inner`.

Do not implement conversion traits between semantically unrelated types merely
because their fields match. Do not keep a custom conversion function beside an
equivalent standard trait implementation.

## Ownership and control flow

Establish ownership once, then borrow. Do not add `clone()`, `collect()`,
`Arc`, or `Box` merely to silence the borrow checker. Each allocation or shared
owner needs a lifetime, concurrency, storage, or measured performance reason.

Prefer guard clauses, `?`, and `let else` over nested control flow. Use iterator
combinators when they express the computation directly. Switch to `for`, `if`,
or `match` when a combinator chain hides branching or error policy.

Validate before mutating. A fallible operation must not leave partially updated
state unless partial progress is its documented contract.

Use typed errors at reusable boundaries. Preserve error sources. Add
operational context where an error crosses into an application boundary.

Prefer a fallible API returning `Result` over a panic. Reserve `unwrap()` and
`expect()` for locally proven invariants or intentional process termination. An
`expect()` message states the invariant that was violated.

## Comments and documentation

Write normal `//` comments as Markdown even though rustdoc does not render
them. Capitalize the beginning of each sentence and end it with a period. Apply
the same rule to tagged comments.

```rust
// `object` is ready to be handled now.
handle(object);

// FIXME: The error should be handled properly.
```

Use comments for implementation details, especially non-local reasons,
invariants, hazards, and constraints that names and types cannot express. Do
not use `//` to document an item for its callers.

Documentation is the default for modules and meaningful items regardless of
visibility. Use `//!` for modules and `///` for structs, enums, traits,
functions, methods, and other items whose contract or role is not completely
obvious. Document private modules and private items with the same care as the
public API. Trivial trait method implementations that add no contract beyond
the trait, and obvious test helpers, may omit duplicated documentation.

Use the `missing_docs` lint to verify public API coverage. A passing
`missing_docs` check is not evidence that documentation is complete because the
lint does not enforce private-item documentation. Separately audit private
modules, types, functions, and methods. When the repository supports it, render
rustdoc with `--document-private-items` and deny broken intra-doc links.

Put a comment about a specific documentation line beside that line. Put other
implementation comments after the item's documentation.

```rust
/// Returns a new [`Foo`].
///
/// [`Foo`]: crate::Foo
///
/// # Examples
///
// TODO: Find a better example.
/// ```
/// let foo = f(42);
/// ```
// FIXME: Use a fallible approach.
pub fn f(x: i32) -> Foo {
    Foo::new(x)
}

/// Performs the private operation.
// TODO: Replace the temporary representation.
fn private_operation() {}
```

Start item documentation with one short sentence stating what the item does.
Put explanations, constraints, and background in later paragraphs. Link Rust
items with reference-style intra-doc links. Every symbolic link must have an
explicit definition in the same rustdoc block that points to the canonical item
path. Do not rely on shortcut links or implicit name resolution.

```rust
/// Scans every module reachable from [`Scanner`].
///
/// [`Scanner`]: crate::build::Scanner
fn scan_crate(scanner: &mut Scanner) {}
```

Keep the visible label concise when the canonical path is long. For example,
write ``[`parse`]`` with ``[`parse`]: crate::syntax::Parser::parse``. Use this
form in both `///` item documentation and `//!` module documentation.

Add an executable `# Examples` section when an example materially clarifies
correct use. Add `# Errors` when callers need to understand meaningful failure
cases. Add `# Panics` whenever a public function can panic, and state the exact
triggering conditions.

Unsafe functions and traits must have a `# Safety` section that states every
obligation imposed on callers or implementors. Every unsafe block, including
one inside an unsafe function, must be immediately preceded by a `// SAFETY:`
comment. That comment proves why the specific operation satisfies every
relevant precondition. Never omit the proof because it appears obvious; doing
so hides implicit constraints from review.

`# Safety` defines the external contract. `// SAFETY:` proves that a specific
block, call, or implementation meets that contract.

```rust
/// Returns the contained [`Some`] value, consuming `self`, without checking
/// that the value is not [`None`].
///
/// # Safety
///
/// Calling this method on [`None`] is *[undefined behavior]*.
///
/// [undefined behavior]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html
/// [`None`]: std::option::Option::None
/// [`Some`]: std::option::Option::Some
///
/// # Examples
///
/// ```
/// let x = Some("air");
/// assert_eq!(unsafe { x.unwrap_unchecked() }, "air");
/// ```
pub unsafe fn unwrap_unchecked(self) -> T {
    match self {
        Some(value) => value,
        // SAFETY: The caller guarantees that `self` is not `None`, so this
        // arm is unreachable.
        None => unsafe { hint::unreachable_unchecked() },
    }
}
```

## Tests

Test behavior through stable entry points. Keep fixtures minimal so each test
makes its condition and expected result obvious. Assert `Err` or `None`
directly. Use a panic test only when panic is the API contract.

Test successful conversions and rejected `TryFrom` inputs. Test round trips
only when the conversion contract promises reversibility. Use documentation
tests for public examples so the compiler checks them.
