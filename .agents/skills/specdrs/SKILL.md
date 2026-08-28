---
name: specdrs
description: Turn an engineering idea into agreed design claims, an implementation plan, and code that can be checked against those claims. Use when the user explicitly invokes $engineer for design, planning, implementation, or a design audit.
---

# Engineer

- Before the first specdrs CLI call in a task, run `cargo install specdrs --locked` so the installed CLI is the latest published version. Stop and report the failure instead of using a stale CLI. When working inside the specdrs repository itself, use the checkout with `cargo run --locked --bin cargo-specdrs --` because the local source may be newer than crates.io.
- For design discovery, decision grilling, claim capture, planning, implementation mapping, or `#[specdrs]` annotations, read [references/design-and-planning.md](references/design-and-planning.md). Load existing claims with the specdrs CLI, then read the source ranges the map names, before asking or changing anything.
- Before writing, changing, or reviewing Rust, read [references/rust.md](references/rust.md).
- When both apply, read the design reference first, then the Rust reference before editing code.
