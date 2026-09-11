# Semantic diagnostic identifiers

Decision for #131: publish semantic identifiers across the CLI and LSP, preserving every existing
numeric alias. This implements the selected scheme in the registry and all presentation consumers.

## Naming and stability

`DiagnosticDef::id()` derives `category-prefix.kebab-case-slug` from the category and slug. A single
category declaration defines each enum variant and its canonical lowercase prefix; the display label
is generated from the variant name. Registry entries supply the category and condition slug, with no
separate prefix or stored ID. CLI diagnostics, LSP codes, quick-fix titles, and catalog URLs consume
this derived ID. Lookup compares the same category prefix and slug without allocating an ID string.

Changing a category changes the descriptive ID and its catalog anchor. Such a change must update
consumers and document migration. Numeric `MOS####` aliases remain stable. Severity and owning-crate
changes do not change either identity.

The initial PR's resolution-check IDs used an incorrect prefix. This correction replaces that prefix
outright; those spellings are not retained as aliases. Numeric `MOS####` aliases remain supported.
Consumers of the initial IDs must update their strings and catalog links to `resolution.*`.

Names describe a condition, omit severity, and use lowercase ASCII words and digits separated by
single hyphens. New rules receive a unique semantic ID and the next unused numeric alias. Neither
identity may be reused for a different condition. A rename requires a separate compatibility
decision. Category changes therefore require the same migration care as slug changes.

## Rust consumers

Existing `codes::MOS0033`, `DiagnosticDef::code()`, `DiagnosticCode` display/equality/hash, and
`slug()` retain their behavior. Compiler emit sites continue referencing the same registered static
definitions. `id()` now returns an owned `String` derived from category and slug rather than a
stored static string; it is no longer a const accessor. `documentation_url()` provides the catalog
link. `codes::lookup("resolution.label-missing")` and `codes::lookup("MOS0033")` resolve to that
exact same static definition. Lookup rejects bare slugs, case variants, whitespace, and alternate
number padding. No numeric alias removal is scheduled.

## CLI migration

Before: `error[MOS0033]: ...`

After: `error[resolution.label-missing (MOS0033)]: ...`

Both `check` and `build` use this format, including spanless diagnostics. Severity, messages, source
spans, suggestions, and phase exit behavior are unchanged. Tools parsing the old bracket format must
update; retaining the numeric token does not preserve that old text format.

## Editor migration

Published diagnostics contain:

```json
{
  "code": "resolution.label-missing",
  "codeDescription": {
    "href": "https://github.com/kjanat/mosaic/blob/master/docs/diagnostic-codes.md#resolution.label-missing"
  },
  "data": { "legacyCode": "MOS0033" }
}
```

This example omits the unchanged range, severity, source, and message fields. Clients matching
`Diagnostic.code` must migrate to semantic IDs or read `data.legacyCode`. Quick-fix titles show
`resolution.label-missing (MOS0033): ...`; edits and applicability are unchanged. Documentation
links use explicit per-rule anchors, and become available on the default branch when this change
merges.

## Validation and scope

Registry invariants check identifier grammar, uniqueness, exact lookup of both spellings, and
category-derived IDs and preservation of numeric aliases across category changes. Catalog drift
tests cover both identities, slugs, severity, owner, summary, category placement, and explicit link
anchors. CLI tests exercise the new format; LSP projection and real-process protocol tests verify
the wire identity and metadata. Existing compiler tests continue asserting stable numeric
definitions where no display is involved.

This change does not add configurable severity, suppression, new diagnostic conditions, or an
explanation subcommand. Future configuration can use the shared lookup API for either spelling.
