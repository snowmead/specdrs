# Release process

`specdrs` uses release-plz. Normal code changes never publish directly. A push
to `main` updates a release pull request when packaged files changed. Publishing
starts only after that release pull request is reviewed and merged.

## What gets released

The workspace contains three public crates:

1. `specdrs-syntax`
2. `specdrs-macros`
3. `specdrs`

release-plz publishes them in dependency order. The two internal crates do not
create Git tags or GitHub Releases. The root `specdrs` crate creates the single
product tag `v<version>` and the corresponding GitHub Release.

Only files included by `cargo package` count as release input. Rust source,
manifests, the README, the bundled skill, and files under `docs/` count. Changes
limited to GitHub workflows or repository maintenance do not force a crate
release.

## Normal release

1. Merge a reviewed pull request into `main`.
2. The release workflow runs the same checks as `prek.toml`.
3. `release-plz release` exits successfully unless the merged pull request is a
   release-plz release pull request containing unpublished versions.
4. `release-plz release-pr` exits successfully when no packaged files changed.
   Otherwise it opens or updates the release pull request with Cargo version,
   lockfile, and changelog changes.
5. Review the generated version and changelog.
6. Merge the release pull request with a merge commit. Do not squash it.
7. The next `main` workflow publishes the unpublished crates, creates
   `v<version>`, and creates the GitHub Release.

release-plz derives the next semantic version from commit messages and API
compatibility checks. Use `feat:` for a feature, `fix:` for a fix, and a
`BREAKING CHANGE:` footer for an intentional breaking change. Other packaged
changes receive a patch release.

Do not edit versions, create release tags, or run `cargo publish` during the
normal process. Fix the release pull request or the automation instead.

## Authentication and permissions

The release workflow uses two short-lived identities:

- A repository-scoped GitHub App installation token creates and updates release
  pull requests, pushes tags, and creates GitHub Releases. Store the App ID in
  `APP_ID` and its private key in `APP_PRIVATE_KEY`.
- crates.io Trusted Publishing exchanges the workflow's GitHub OIDC identity for
  a temporary registry token. Do not store `CARGO_REGISTRY_TOKEN` after the
  bootstrap release.

The GitHub App needs `Contents: read and write` and
`Pull requests: read and write`. Install it only on this repository. Configure
Trusted Publishing on all three crates with:

- Repository: `snowmead/specdrs`
- Workflow: `release-plz.yml`
- Environment: `release`

The `release` environment must allow deployments only from `main`.

## Initial 0.1.0 bootstrap

crates.io cannot use Trusted Publishing for a crate's first version. Bootstrap
once with a scoped token, then remove it.

1. Merge the release automation while `RELEASES_ENABLED` is unset.
2. Check out the exact `main` commit that will become `v0.1.0`.
3. Create a crates.io token with `publish-new` and `publish-update` scopes.
4. Publish and wait for each dependency to appear in the registry:

   ```console
   cargo publish --locked -p specdrs-syntax
   cargo search specdrs-syntax --limit 1
   cargo publish --locked -p specdrs-macros
   cargo search specdrs-macros --limit 1
   cargo publish --locked -p specdrs
   ```

5. Create `v0.1.0` and a GitHub Release from the same commit.
6. Configure Trusted Publishing for all three crates.
7. Delete the bootstrap registry token.
8. Create the GitHub App secrets and the protected `release` environment.
9. Set the repository Actions variable `RELEASES_ENABLED` to `true`.
10. Dispatch the release workflow once. It must finish without publishing because
    all `0.1.0` packages already exist.

## Required repository rules

Protect `main` with these rules:

- Require a pull request.
- Require one approval.
- Require the `CI / checks` status check.
- Allow the repository owner to bypass the rules when an explicit force merge is
  necessary.

Release pull requests are authored by the GitHub App, so the repository owner can
review them and CI runs normally.

## Failure recovery

- If CI fails, fix the code. Do not bypass a release with failing checks.
- If no packaged files changed, a successful no-op is correct.
- If one internal crate publishes and a later crate fails, rerun the workflow.
  release-plz skips versions already present in crates.io and resumes with the
  unpublished crate.
- If a tag or GitHub Release exists but publication failed, do not reuse or move
  the tag manually. Fix the failure and rerun release-plz.
- If the generated version is wrong, edit the release pull request before
  merging it.
