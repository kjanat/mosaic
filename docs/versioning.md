# Versioning and release policy

Decision record for how workspace crate versions are managed, and the playbook for the day part of
it changes. Applies to everything `cargo metadata` lists as a workspace member (16 crates;
`zed-mosaic` is excluded from the workspace and does not publish to crates.io).

## Policy: lockstep, until further notice

All workspace crates share one version, inherited from `workspace.package.version` in the root
`Cargo.toml`. Internal dependency requirements in `[workspace.dependencies]` pin that same version.
The release tag `vX.Y.Z` must match it (CI enforces this against the `mos` crate).

Why lockstep is the right call at `0.0.x`:

- Cargo caret semantics make every `0.0.x` requirement exact (`"0.0.1"` means `=0.0.1`), so any bump
  forces republishing all 16 crates anyway. Independent versions would buy nothing except
  bookkeeping.
- One version means one bump commit, one tag, and a release workflow that can derive every crate's
  version from the tag.
- The publish workflow (`.github/workflows/crates-io.yml`) is built on this assumption: the
  `cargo publish` step probes and publishes every crate at `${RELEASE_TAG#v}`.

Accepted cost: unchanged crates get republished byte-identical under the new version each release.
At this crate count that churn is cheap.

## How to release

1. `just bump 0.0.2` — updates `workspace.package.version`, all `[workspace.dependencies]` pins, and
   the lockfile in one shot (wraps `cargo set-version --workspace`). Note `set-version` refuses to
   downgrade: undoing a mistyped bump means editing the root `Cargo.toml` versions by hand and
   rerunning `cargo update --workspace`.
2. Cut `CHANGELOG.md`: move `[Unreleased]` into a new version section.
3. Commit, then tag the work commit itself (no dedicated bump commit for the tag to chase):
   `git tag -s v0.0.2 -m "v0.0.2 — <summary>"` and push the tag.
4. The `crates-release` workflow takes it from there. Two guard rails to know about:
   - **Staleness gate**: fails the release if a crate's directory changed since the previous tag
     while its version stayed the same and that version is already live on crates.io. Under lockstep
     this never fires (the shared version always bumps); it exists to catch the decoupled future
     below.
   - **Resumable publish**: crates already live at the tag version are skipped, so re-running a
     partially failed release is safe.

## The two non-`mos` crates

`adobe-font-metrics` and `pdf-base14-metrics` are standalone libraries that happen to live here:
general-purpose names, their own lower MSRV (1.85 vs the workspace's product MSRV), and APIs far
more stable than the compiler's. Lockstep gives their version numbers zero signal — they tick
because the compiler released.

**Current stance: they ride along anyway.** Meaningless-but-harmless version churn is a smaller cost
than maintaining a second release cadence during pre-alpha.

**Decouple when** one of them is ready to promise stability that the compiler can't — in practice:
the first time you want to publish `adobe-font-metrics 0.1.0` while `mos` is still `0.0.x`, or an
external consumer shows up.

## Playbook: decoupling a crate from lockstep

Small change, but it touches the release workflow. Steps for each crate being decoupled:

1. In the crate's `Cargo.toml`, replace `version.workspace = true` with an explicit
   `version = "X.Y.Z"`.
2. In root `[workspace.dependencies]`, the crate's `version = "..."` pin now tracks the crate's own
   version. It only needs editing when that crate bumps.
3. In `.github/workflows/crates-io.yml`, the `cargo publish` step assumes every crate publishes at
   the tag version (`version="${RELEASE_TAG#v}"`). Rework `publish_crate`/`crate_status` to take the
   per-crate version from `cargo metadata` — the staleness gate's TSV loop (name, version, manifest
   dir) already does exactly this; reuse that shape.
4. Nothing else changes: the tag check only inspects `mos`, and the staleness gate is already
   per-crate — it becomes the thing that *enforces* "you touched a decoupled crate, bump it".
5. In the justfile, add `--exclude <crate>` to the `bump` recipe's `cargo set-version` call.
   Verified against cargo-edit 0.13: a bare `--workspace` bump rewrites explicit versions too, so
   without the exclude the decoupled crate silently snaps back into lockstep. With the exclude, both
   the crate and its `[workspace.dependencies]` pin stay untouched.
6. Decide where the decoupled crate's changelog lives (own `CHANGELOG.md` in the crate dir, or a
   section in the root one) before its first independent release.

Bumping a decoupled crate alone is `cargo set-version -p adobe-font-metrics 0.1.0`; it updates the
root `[workspace.dependencies]` pin too (verified against cargo-edit 0.13). Careful with the same
command **before** decoupling: `-p` on a crate that still inherits `version.workspace = true`
resolves to the workspace version and bumps the entire workspace.

## Rejected: release-plz / fully independent versions

Automated per-crate versioning from conventional commits is a real option (the commit style here
already qualifies), but it takes over tagging and changelog generation and solves a coordination
problem that does not exist at this contributor count. Revisit if the number of independently
versioned crates grows past the two above.
