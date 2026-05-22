# mos-packages

`mos-packages` owns the current Mosaic project manifest schema for `mosaic.toml`.

Today it is intentionally small: it parses TOML into typed Rust structs and reports disk/parse
errors with manifest paths. It does not resolve packages, generate lockfiles, fetch registries, or
drive the `mos` CLI build pipeline yet.

## Scope

- Defines `ProjectManifest`, `ProjectSection`, and `DocumentSection`.
- Parses `mosaic.toml` with `ProjectManifest::load(&Path)`.
- Supports direct `toml::from_str::<ProjectManifest>(...)` parsing.
- Rejects unknown fields through `serde(deny_unknown_fields)`.
- Preserves `[dependencies]` as a `BTreeMap<String, String>` only.

## Manifest Schema

Minimal file:

```toml
[project]
name    = "demo"
version = "0.1.0"
entry   = "main.mos"
```

Full current shape:

```toml
[project]
name    = "hello"
version = "0.1.0"
entry   = "main.mos"

[document]
language = "en"
output   = ["pdf"]

[dependencies]
```

Fields:

| TOML path           | Rust field                                  | Required | Default |
| ------------------- | ------------------------------------------- | -------- | ------- |
| `project.name`      | `ProjectSection::name: String`              | yes      | none    |
| `project.version`   | `ProjectSection::version: String`           | yes      | none    |
| `project.entry`     | `ProjectSection::entry: String`             | yes      | none    |
| `document.language` | `DocumentSection::language: Option<String>` | no       | `None`  |
| `document.output`   | `DocumentSection::output: Vec<String>`      | no       | `[]`    |
| `dependencies`      | `BTreeMap<String, String>`                  | no       | `{}`    |

The crate does not validate version syntax, output backend names, dependency formats, or whether
`project.entry` exists on disk. It only deserializes the schema.

## API Behavior

```rust
use std::path::Path;

use mos_packages::{ManifestError, ProjectManifest};

fn read_manifest(path: &Path) -> Result<ProjectManifest, ManifestError> {
    ProjectManifest::load(path)
}
```

`ProjectManifest::load`:

- reads the given path as UTF-8 text via `std::fs::read_to_string`;
- returns `ManifestError::Io { path, source }` when reading fails;
- parses with `toml::from_str`;
- returns `ManifestError::Parse { path, source }` when TOML or schema parsing fails.

## Boundary

This crate sits near `mos-core` as a project/package foundation crate. It should stay boring until
real integration lands. The current CLI still accepts explicit `.mos` entry paths; do not assume
`mosaic.toml` controls `mos build`, output backends, or dependency loading.

## Known Non-Goals

- No registry access.
- No lockfile model or lockfile generation.
- No dependency resolution.
- No package fetching, cache population, or source checkout.
- No CLI orchestration or build output selection.
- No validation beyond TOML deserialization and unknown-field rejection.
