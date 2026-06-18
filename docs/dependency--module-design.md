# Dependency And Module Design

Status: design report. This is not shipped compiler behavior.

Current truth: `mos-packages` parses `mosaic.toml`; `mos` uses `[project].entry` and `[output].pdf`
for directory builds. `[dependencies]`, `mosaic.lock`, package fetch, package resolution, package
imports, `--frozen`, and reproducible package builds are not implemented yet.

This document proposes the long-term dependency and module model for Mosaic. It intentionally aims
past the current pre-alpha slice. Mosaic may be small today; the package system should not encode
smallness as destiny.

## Goals

- Put full, stable dependency paths in `.mos` source files, Go-style.
- Keep version policy in `mosaic.toml`, not inline in prose files.
- Keep exact selected versions and content hashes in `mosaic.lock`.
- Use a registry and proxy model with immutable artifacts.
- Support reproducible release builds without turning day-to-day authoring into paperwork.
- Default packages to deterministic, sandboxable behavior.
- Allow Mosaic to grow into a large ecosystem without a resolver rewrite.

## Non-Goals

- This is not an implementation plan for the current parser slice.
- This does not require package imports to ship before section imports, cache work, or lockfile I/O.
- This does not make `manifest.md` shipped truth.
- This does not add arbitrary package install scripts. Npm demon stays in cave.

## Domain And Namespace Recommendation

Package import paths need a stable DNS-owned prefix. `mosaiclang.dev` is the project-owned primary
domain as of 2026-06-18. First-pass RDAP/whois checks on the same date showed the defensive options
below as available-looking, but registration availability can change and must be confirmed at
purchase time.

Primary domain:

```text
mosaiclang.dev
```

Why:

- Clear language/tool identity.
- `.dev` requires HTTPS, which fits package registry and docs hosting.
- Short enough for import paths.
- Supports clean subdomains: `pkg.mosaiclang.dev`, `sum.mosaiclang.dev`, `proxy.mosaiclang.dev`,
  `docs.mosaiclang.dev`.

Strong defensive buys:

| Domain              | Suggested role                         | Status                                      |
| ------------------- | -------------------------------------- | ------------------------------------------- |
| `mosaiclang.dev`    | Primary website and ecosystem identity | Purchased                                   |
| `mosaictypeset.dev` | Product-positioning redirect           | Google Registry RDAP 404, available-looking |
| `mosaictypeset.org` | Community/docs redirect                | RDAP/whois not found, available-looking     |
| `mosaictypeset.com` | Defensive redirect                     | Whois no match, available-looking           |
| `moslang.com`       | Short defensive redirect               | Whois no match, available-looking           |
| `mosaicpkg.dev`     | Future package tooling redirect        | Google Registry RDAP 404, available-looking |

Already registered or less attractive from checks:

| Domain              | Reason to avoid as primary                       |
| ------------------- | ------------------------------------------------ |
| `mosaic.dev`        | Registered                                       |
| `mosaic.org`        | Registered                                       |
| `mosaiclang.org`    | Registered                                       |
| `mosaic-lang.org`   | Registered                                       |
| `mosaic-lang.dev`   | Registered                                       |
| `getmosaic.dev`     | Registered                                       |
| `mosaic.ink`        | Registered                                       |
| `mos.ink`           | Registered                                       |
| `mosaic.pub`        | Registered                                       |
| `mos.pub`           | Registered                                       |
| `mosaic.tools`      | Registered                                       |
| `mos.tools`         | Registered                                       |
| `mosaic.run`        | Registered                                       |
| `mos.run`           | Registered                                       |
| `mosaic.build`      | Registered                                       |
| `mos.build`         | Registered                                       |
| `mosc.dev` / `.org` | Registered and also too compiler-command-looking |

The examples below use `pkg.mosaiclang.dev` as the package root.

## Go Modules: Useful Mechanics

Go's package system is worth studying because it makes imports readable and routing simple.

Core terms:

| Go term     | Meaning                                                                             |
| ----------- | ----------------------------------------------------------------------------------- |
| Module      | Versioned source tree with a `go.mod` file.                                         |
| Package     | A directory inside a module.                                                        |
| Import path | Full path in source, usually `domain/module/subdir`.                                |
| `go.mod`    | Human-edited module manifest with module path and minimum dependency versions.      |
| `go.sum`    | Hash ledger for downloaded module metadata and archives. Not a lockfile.            |
| Proxy       | Static HTTP service that serves module versions, metadata, manifests, and archives. |

Go routing in one pass:

1. Source imports a full path such as `example.com/acme/report/pdf`.
2. The Go command finds a module in the build list whose module path prefixes the import path.
3. The remaining path is a package directory inside that module.
4. If no current module provides the package, Go queries proxies or VCS for possible module
   prefixes.
5. If one provider wins, Go adds the module to the build list.
6. If none or multiple providers win, the build fails.

Go dependency selection:

- `go.mod` records minimum required versions.
- Minimal Version Selection keeps the highest minimum version required by the graph.
- Newer versions are not selected unless requested.
- There is no normal lockfile; Go argues the selected build list is deterministic from `go.mod`.

Go integrity:

- `go.sum` stores hashes for module manifests and module archives.
- The checksum database gives public transparency for module hashes.
- Proxies can be untrusted because content is verified.
- Private paths need explicit config, or the public proxy/checksum DB can see private import names.

Go local development:

- `replace` maps a module path to a local path or fork.
- `replace` only affects the main module or workspace.
- `go.work` lets multiple local modules be developed together.
- Vendor mode copies selected packages into `vendor/` for offline builds.

Go ideas Mosaic should copy:

- Full import paths in source files.
- Import path is semantic identity, not just download URL.
- Registry/proxy decoupled from source hosting.
- Immutable release artifacts.
- Content hashes checked on every fetch.
- Root-only local overrides.
- Explicit private package routing config.

Go ideas Mosaic should not copy directly:

- No lockfile for final documents. Mosaic needs stronger archival reproducibility than Go libraries.
- MVS-only dependency selection. Mosaic should allow Cargo-like full resolution as the ecosystem
  grows.
- VCS-first identity. Mosaic packages should be registry artifacts first; VCS is source provenance.

## Mosaic Model

Mosaic should combine four systems:

| Source | Borrowed idea                                                               |
| ------ | --------------------------------------------------------------------------- |
| Go     | Full path imports, immutable modules, proxy/checksum discipline.            |
| Cargo  | Human manifest plus machine lockfile with exact graph.                      |
| Typst  | Simple author-facing package imports.                                       |
| npm    | Integrity hashes in lock metadata, but not scripts. Absolutely not scripts. |

Recommended shape:

```text
mosaic.toml      human dependency intent and version constraints
mosaic.lock      exact resolved graph, sources, hashes, capabilities
global cache     immutable verified package artifacts
registry/proxy   package path to versions, metadata, archives
workspace file   optional local multi-package development only
vendor bundle    optional offline/archive copy of resolved artifacts
```

## Terms

Project:

The root document build. It has a `mosaic.toml`, one or more source files, assets, and outputs. The
project is the authority for local overrides and frozen/release policy.

Package:

A versioned distribution artifact published under a stable package path. A package is the unit that
appears in the resolver graph and lockfile.

Module:

An importable unit inside a package or project. A module may be a `.mos` file, a style bundle, a
template, a bibliography style, or another exported Mosaic unit. A package contains zero or more
modules.

Package path:

The stable DNS-rooted identity of a package, for example:

```text
pkg.mosaiclang.dev/std/report
pkg.mosaiclang.dev/clinical/vancouver
packages.myhospital.org/radiology/case-template
```

Module path:

The package path plus an optional subpath naming an importable unit inside the package:

```text
pkg.mosaiclang.dev/std/report
pkg.mosaiclang.dev/std/report/templates/case-report
pkg.mosaiclang.dev/clinical/vancouver/citation-style
```

Repository:

The VCS location where package source lives. Repository URL is provenance, not package identity. A
single repository can publish multiple package paths. A package can move repository without changing
identity if registry policy allows it.

Registry:

The service that owns package metadata, versions, signatures, and archive URLs for a package path.

Proxy:

A cacheable HTTP facade over one or more registries. Proxies can enforce company policy, mirror
public packages, and serve offline CI. They do not need to be trusted if hashes/signatures are
verified.

## Import Syntax Direction

The source should contain full module paths. This is the Go-like part the design should preserve.

Future syntax sketch:

```mos
#import "pkg.mosaiclang.dev/std/report"
#import "pkg.mosaiclang.dev/clinical/vancouver/v2"
#import "packages.myhospital.org/radiology/case-template"

#import("pkg.mosaiclang.dev/std/report")
#import("pkg.mosaiclang.dev/clinical/vancouver/v2")
#import("packages.myhospital.org/radiology/case-template")

#import(
  "pkg.mosaiclang.dev/std/report"
  "pkg.mosaiclang.dev/clinical/vancouver/v2"
  "packages.myhospital.org/radiology/case-template"
)
```

All three forms are valid by design:

| Form               | Role                                                                  |
| ------------------ | --------------------------------------------------------------------- |
| `#import "path"`   | Declaration-style import, preferred for one simple import.            |
| `#import("path")`  | Directive-call form, consistent with existing Mosaic directive calls. |
| `#import(` ... `)` | Go-style import block, preferred for multiple imports.                |

The forms are semantically identical. Formatters may choose one canonical house style later, but the
parser should accept all three because source authors will reasonably expect all three.

Import blocks should start simple:

- each entry is a string literal package/module path;
- entries are separated by newlines;
- trailing commas may be accepted for formatter friendliness, but they are not required;
- aliases and selective imports are deferred until real ergonomics demand them.

Rules:

- The string is a module path, not a version constraint.
- The string is stable enough to read outside the project context.
- The path starts with a DNS name controlled by the package ecosystem owner.
- The manifest decides which versions are allowed.
- The lockfile decides which exact versions are used.
- Relative local imports may exist separately, but they are not package imports.

Example manifest:

```toml
[project]
name    = "pe-report"
version = "0.1.0"
entry   = "main.mos"

[dependencies]
"pkg.mosaiclang.dev/std/report"                   = "^1.4"
"pkg.mosaiclang.dev/clinical/vancouver/v2"        = "^2.0"
"packages.myhospital.org/radiology/case-template" = ">=0.8, <1.0"
```

Example source:

```mos
#import "pkg.mosaiclang.dev/std/report/templates/case-report"
#import("pkg.mosaiclang.dev/std/report/templates/case-report")
#import(
  "pkg.mosaiclang.dev/clinical/vancouver/v2"
  "packages.myhospital.org/radiology/case-template"
)

#heading("Pulmonary embolism report")
```

The import path remains readable even without the manifest. The manifest supplies policy. The
lockfile supplies exactness.

## Import Routing

Package import resolution should use longest-prefix matching against the resolved lock graph.

Input:

```text
pkg.mosaiclang.dev/std/report/templates/case-report
```

Resolved package in `mosaic.lock`:

```text
pkg.mosaiclang.dev/std/report @ 1.4.2
```

Routed module subpath:

```text
templates/case-report
```

Resolution algorithm:

1. Parse the import string into a normalized module path.
2. Reject empty paths, rooted filesystem paths, `..` escapes, and platform-specific path syntax.
3. If the path is relative, resolve it under the current source-file or project-local import rules.
4. If the path starts with a DNS name, treat it as a package/module import.
5. Find all package paths in the lock graph that prefix the module path at path-segment boundaries.
6. Select the longest matching package path.
7. Fail if there is no match in frozen mode.
8. In non-frozen mode, query registry metadata for declared dependency roots that could satisfy the
   import, update the lock graph if allowed, then retry.
9. Fail if two distinct packages would provide the same module path.
10. Map the remaining subpath to an exported module inside the package archive.

The project manifest should normally declare dependency roots. Source imports should not silently
add new root dependencies during release builds. Surprise network because a paragraph imported
something? Bad rock.

## Versioning

Mosaic should use semantic versions for packages.

Package identity should include the path and major compatibility line. Recommended rule:

- `v0` and `v1` use the base path.
- `v2+` use a Go-style major suffix in the package path.
- The suffix is part of the package path and appears in source imports.

Examples:

```text
pkg.mosaiclang.dev/clinical/vancouver      # v0/v1 line
pkg.mosaiclang.dev/clinical/vancouver/v2   # v2 line
pkg.mosaiclang.dev/clinical/vancouver/v3   # v3 line
```

Why path-major versions fit Mosaic:

- Source files reveal breaking API lines.
- Different major versions can coexist in one project.
- The resolver does not need to pretend incompatible APIs are the same package.
- Lockfiles stay clear when transitive packages need old and new major lines.

The resolver can still use full semver constraints inside each major line.

## Resolver Policy

Mosaic should target a Cargo-like resolver, not Go MVS.

Resolver inputs:

- Root `mosaic.toml` dependencies.
- Package manifests from candidate package versions.
- Existing `mosaic.lock`, when present.
- Root-only overrides.
- Registry metadata.
- Capability/security policy.
- Target engine version and package format version.

Resolver output:

- One selected version per package path and major line, unless different package paths intentionally
  name different major lines.
- Exact sources and archive hashes.
- Complete transitive dependency graph.
- Capability class and required permissions for each package.

Update behavior:

- If `mosaic.lock` exists and all locked versions still satisfy constraints, reuse them.
- If dependencies are added, removed, or constraints changed, update only the affected graph when
  possible.
- `mos package update` may update all allowed packages.
- `mos package update <path>` may update one dependency subtree.
- A future `--minimal-versions` mode can test lower bounds, like Cargo does for libraries.

Selection behavior:

- Prefer the newest compatible version when no lock entry constrains the choice.
- Respect upper bounds when declared.
- Reject incompatible capability requirements unless the root project explicitly allows them.
- Reject yanked or retracted versions unless already locked and explicitly allowed.
- Preserve lockfile choices for unrelated packages to avoid dependency churn.

Overrides:

- Local path overrides are root-only.
- Registry replacement is root-only.
- Overrides must be recorded in lockfile metadata.
- Publishing a package that requires a local override should fail.

## Manifest Shape

Current `ProjectManifest` stores `[dependencies]` as `BTreeMap<String, String>`. The future schema
can keep the simple string form while allowing structured dependency records.

Simple dependency:

```toml
[dependencies]
"pkg.mosaiclang.dev/std/report" = "^1.4"
```

Structured dependency:

```toml
[dependencies."pkg.mosaiclang.dev/std/report"]
version          = "^1.4"
default-features = true
features         = ["pdf"]
```

Local development override:

```toml
[patch."pkg.mosaiclang.dev"]
"std/report" = { path = "../mosaic-std/report" }
```

Private registry source:

```toml
[registries]
hospital = "https://packages.myhospital.org"

[dependencies]
"packages.myhospital.org/radiology/case-template" = "^0.8"
```

The dependency key is always the package path. Alias names should be deferred unless real source
ergonomics demand them.

## Package Manifest Shape

A published package should include its own manifest. The exact filename can be `mosaic.toml` inside
the package archive, but it should use a package section distinct from project-only fields.

Sketch:

```toml
[package]
path        = "pkg.mosaiclang.dev/std/report"
version     = "1.4.2"
description = "Standard report templates"
license     = "MIT OR Apache-2.0"

[exports]
"."                     = "src/lib.mos"
"templates/case-report" = "templates/case-report.mos"
"styles/default"        = "styles/default.mos"

[dependencies]
"pkg.mosaiclang.dev/std/core" = "^1.1"

[capabilities]
class  = "pure"
assets = ["templates/**", "styles/**"]
```

Package manifest invariants:

- `package.path` must match the registry path being published.
- `package.version` must match the published version.
- Export paths must be relative, normalized, and inside the package archive.
- Package contents must not depend on absolute source paths.
- Pure packages cannot request native code, network, time, environment, or arbitrary filesystem
  access.

## Lockfile Shape

`mosaic.lock` should be machine-written and committed for real projects. Release and frozen builds
should require it.

Sketch:

```toml
version  = 1
resolver = "mosaic-resolver-1"

[[package]]
path              = "pkg.mosaiclang.dev/std/core"
version           = "1.1.3"
source            = "registry+https://pkg.mosaiclang.dev"
checksum          = "blake3-256:2e5f..."
manifest_checksum = "blake3-256:8c21..."
capability_class  = "pure"

[[package]]
path              = "pkg.mosaiclang.dev/std/report"
version           = "1.4.2"
source            = "registry+https://pkg.mosaiclang.dev"
checksum          = "blake3-256:b773..."
manifest_checksum = "blake3-256:f021..."
capability_class  = "pure"
dependencies      = [
  { path = "pkg.mosaiclang.dev/std/core", version = "1.1.3" },
]
```

Required lockfile semantics:

- It records the exact resolved graph.
- It records package content hashes.
- It records package manifest hashes separately from full archive hashes.
- It records source registry/proxy identity.
- It records local overrides when used.
- It records capabilities accepted by the root project.
- It is deterministic in ordering and formatting.

No separate `mos.sum` is required for the first implementation. Go needs `go.sum` because it does
not use a normal lockfile. Mosaic can put selected-graph integrity in `mosaic.lock`. A future
checksum transparency log can reuse the same content hash lines without adding a second project
file.

## Frozen And Release Builds

Modes:

| Mode          | Lock behavior                                   | Network behavior                  |
| ------------- | ----------------------------------------------- | --------------------------------- |
| Dev build     | May create or update `mosaic.lock` when allowed | May fetch missing packages        |
| `--locked`    | Requires lockfile and refuses graph changes     | May fetch exact locked packages   |
| `--frozen`    | Requires lockfile and refuses graph changes     | Refuses network and missing cache |
| Release build | Same graph strictness as `--locked` at minimum  | Policy decides cache/network      |

Failure cases:

- Missing lockfile in `--locked`, `--frozen`, or release mode.
- Manifest dependencies changed but lockfile was not updated.
- Locked package hash does not match fetched or cached artifact.
- Locked package requires capabilities not accepted by the root project.
- Package path in source cannot be mapped to a locked package.
- Registry metadata says a locked version was yanked and policy forbids it.

## Registry And Proxy Protocol

The registry/proxy protocol should be static-HTTP-friendly, similar to Go.

Possible endpoints:

```text
/<package-path>/@v/list
/<package-path>/@v/<version>.info
/<package-path>/@v/<version>.toml
/<package-path>/@v/<version>.tar.zst
/<package-path>/@v/<version>.hash
/<package-path>/@latest
```

Metadata file example:

```json
{
	"path": "pkg.mosaiclang.dev/std/report",
	"version": "1.4.2",
	"archive": "https://pkg.mosaiclang.dev/std/report/@v/1.4.2.tar.zst",
	"checksum": "blake3-256:b773...",
	"manifestChecksum": "blake3-256:f021...",
	"publishedAt": "2026-06-18T00:00:00Z",
	"yanked": false
}
```

Protocol rules:

- Published `(path, version)` artifacts are immutable.
- Deleting or replacing a version is forbidden after publication.
- Bad versions are yanked or retracted, not mutated.
- Archives are canonicalized before hashing.
- Hashes cover normalized archive contents, not incidental compression metadata.
- Proxies may cache forever by `(path, version, checksum)`.
- Private package paths must not be sent to public proxies unless configured.

## Cache Layout

The global package cache should be immutable and separate from `.mos-cache/` incremental build data.

Suggested roles:

| Cache                 | Purpose                                                 |
| --------------------- | ------------------------------------------------------- |
| Global package cache  | Verified package archives and extracted package trees.  |
| Project `.mos-cache/` | Incremental compiler/layout artifacts.                  |
| Vendor directory      | Optional checked-in or bundled resolved package source. |

Rules:

- Package cache entries are keyed by package path, version, source, and checksum.
- Extracted packages are read-only.
- Tooling never edits cached package contents.
- Cache hits still verify hash metadata at the boundary where trust matters.
- Incremental layout cache keys may depend on package identity and hashes, not package cache paths.

## Private Packages

Private routing must be first-class, not a footnote.

Config concepts:

```text
MOSPRIVATE=packages.myhospital.org,*.corp.example
MOSPROXY=https://proxy.corp.example,https://pkg.mosaiclang.dev,direct
MOSNOPROXY=packages.myhospital.org
MOSNOSUMDB=packages.myhospital.org
```

Rules:

- Private import paths must not be leaked to public proxies or checksum services by default once
  marked private.
- CI should be able to set package policy through environment or config.
- Lockfiles may contain private package paths because source already does.
- Error messages should mention when a path is private and which policy blocked lookup.

## Security Model

Default package class:

```text
pure package:
  deterministic
  no native code execution
  no network access
  no environment access
  no filesystem access except declared package assets
  no wall-clock time
  cacheable
```

Optional package class:

```text
trusted package:
  may request explicit capabilities
  requires root project consent
  appears in lockfile
  should be disabled in frozen archival mode unless policy permits it
```

Capability examples:

```text
read:project/data/**
read:package-assets
write:build/generated/**
network:https://api.example.org
native:wasm-component
```

V1 should avoid trusted packages unless a concrete feature forces them. Pure packages are enough for
styles, templates, bibliography styles, static assets, and deterministic functions.

## Reproducibility Boundary

A reproducible Mosaic build depends on:

- Source file bytes.
- `mosaic.toml` dependency constraints and settings.
- `mosaic.lock` exact package graph.
- Package archive hashes.
- Asset hashes.
- Bibliography file hashes.
- Font identities and hashes, where licenses permit bundling or pinning.
- Engine version.
- Package format version.
- Layout policy version.
- Backend version.
- Accepted package capabilities.

It must not depend on:

- Absolute filesystem paths.
- File mtimes.
- Usernames.
- Hostname.
- Locale unless explicitly configured.
- Network response order.
- Registry freshness during frozen builds.
- Hash map iteration order.

This aligns with `docs/incremental-dependencies.md`: package dependency IDs should eventually be
registry-qualified package path plus resolved version plus manifest/content hash. Build and layout
cache keys should not include mutable cache locations.

## Diagnostics

Future diagnostics should cover package failures with stable `MOS####` codes minted in
`mos-core::codes` and mirrored in `docs/diagnostic-codes.md`.

Likely diagnostic categories:

| Category                       | Example                                                                  |
| ------------------------------ | ------------------------------------------------------------------------ |
| Invalid import path            | `#import("../pkg")` used where package path is required.                 |
| Undeclared dependency          | Source imports a package not allowed by `mosaic.toml`.                   |
| Lock missing                   | `--frozen` build has no `mosaic.lock`.                                   |
| Lock stale                     | Manifest constraints and lock graph disagree.                            |
| Package not found              | Registry/proxy cannot find path or version.                              |
| Hash mismatch                  | Fetched artifact does not match lockfile.                                |
| Capability denied              | Package requests trusted behavior not approved by root.                  |
| Ambiguous provider             | Two locked packages could provide one module path.                       |
| Private policy leak prevention | Lookup blocked because path is private and no private source configured. |

Package resolution should collect enough errors to be useful, but builds should stop before parsing
package source if the resolved graph is not trustworthy.

## Commands

Future command behavior:

```bash
mos package resolve        # create/update mosaic.lock from mosaic.toml
mos package update         # update all dependencies within constraints
mos package update <path>  # update one dependency subtree
mos package graph          # print resolved package graph
mos package vendor         # copy locked package artifacts into vendor/
mos build --locked         # refuse lockfile changes
mos build --frozen         # refuse lockfile changes and network/cache misses
```

Current reality caveat: `mos package` is a stub and `--frozen` is parsed but not implemented. Do not
document these as shipped commands until code exists.

## Rollout Plan

Phase 1: typed schema only.

- Keep `mos-packages` as the manifest/lockfile schema crate.
- Add typed dependency specs without fetching anything.
- Add lockfile structs and parser/writer.
- Add tests for unknown fields, string dependencies, structured dependencies, and deterministic
  lockfile formatting.

Phase 2: path and package identity.

- Define package path parser and normalized module path type.
- Add package identity to `mos-cache` only after path/version/hash rules are stable.
- Add diagnostics for invalid package paths.
- Implement `#import "path"`, `#import("path")`, and grouped `#import(...)` forms.

Phase 3: resolver without network.

- Resolve against local fixture registries.
- Support exact versions, semver ranges, lock reuse, and stale lock detection.
- Add `--locked` and `--frozen` behavior at CLI boundaries.
- Keep package source out of parser/eval until graph trust is solved.

Phase 4: fetch, cache, and registry proxy.

- Implement static HTTP metadata and archive fetch.
- Verify hashes before extraction.
- Add global immutable package cache.
- Add private package routing config.

Phase 5: package imports.

- Add parser support for package import syntax.
- Add evaluator source-provider abstraction so package files are loaded from verified package roots.
- Route imports by longest locked package path prefix.
- Add cycle detection and clear package/module diagnostics.

Phase 6: publishing and ecosystem polish.

- Implement package validation and publish checks.
- Add yank/retract metadata.
- Add package signing or checksum transparency.
- Add vendor/bundle support for archival builds.

## Open Questions

- Alias support: needed in v1, or defer until import ergonomics prove painful?
- Feature flags: include in v1 resolver, or start with capabilities only?
- Checksum transparency log: build now, or leave protocol space for later?
- Package archive format: `.tar.zst`, `.zip`, or custom bundle?
- Public standard library path: `pkg.mosaiclang.dev/std/*` or `mosaiclang.dev/std/*`?
