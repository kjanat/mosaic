# DOCS KNOWLEDGE BASE

## OVERVIEW

`docs/` holds developer design notes and human mirrors of compiler contracts. Code truth first,
manifest dreams last.

## WHERE TO LOOK

| Task                | Location                              | Notes                                      |
| ------------------- | ------------------------------------- | ------------------------------------------ |
| Diagnostic catalog  | `diagnostic-codes.md`                 | Mirrors `mos-core::codes`; drift-tested.   |
| Labels/references   | `labels-and-references.md`            | Current resolver boundary and next slices. |
| Page refs/fixpoints | `page-reference-fixpoint-boundary.md` | Layout coupling risks.                     |
| Formatter trivia    | `formatter-trivia-requirements.md`    | Formatter contract, not implementation.    |
| Incremental deps    | `incremental-dependencies.md`         | Cache/dependency design intent.            |

## TRUTH RULES

- Current code and tests beat docs.
- README Status beats old roadmap prose.
- `manifest.md` is product direction, not shipped behavior.
- Design notes may describe boundaries or contracts before implementation; label that clearly.

## DIAGNOSTIC CATALOG

- Registry truth is `crates/mos-core/src/codes.rs`.
- Catalog mirror is `docs/diagnostic-codes.md`.
- Drift guard is `crates/mos/tests/catalog.rs`.
- Add or change a code by editing registry and catalog together.
- Numeric code values are opaque stable IDs; do not group meaning by number range.

## ANTI-PATTERNS

- Do not overclaim shipped compiler behavior from design docs.
- Do not update the catalog without the registry, or registry without catalog.
- Do not use docs to smuggle broad MVP scope into a narrow code task.
