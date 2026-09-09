# Semantic diagnostic identifiers

Decision for #131: publish semantic identifiers across the CLI and LSP, preserving every existing
numeric alias. This implements the selected scheme in the registry and all presentation consumers.

## Naming and stability

`DiagnosticDef::id()` returns `namespace.kebab-case-slug`. The initial namespace matches the
registered category in lowercase (`syntax`, `semantic`, `layout`, `text`, `pdf`, `io`, `internal`).
It is an explicit registry literal, independent of category metadata. Moving a rule to another phase
or crate must preserve its namespace and slug. Severity changes also preserve identity. Existing
slugs are promoted unchanged, including the `internal-` prefix on internal rules.

Names describe a condition, omit severity, and use lowercase ASCII words and digits separated by
single hyphens. New rules receive a unique semantic ID and the next unused numeric alias. Neither
identity may be reused for a different condition. A rename requires a separate compatibility
decision; changing a category or owner is never grounds for an automatic rename.

## Rust consumers

Existing `codes::MOS0033`, `DiagnosticDef::code()`, `DiagnosticCode` display/equality/hash, and
`slug()` retain their behavior. Compiler emit sites continue referencing the same registered static
definitions. `id()` provides the canonical display/machine identity and `documentation_url()`
provides the catalog link. `codes::lookup("semantic.label-missing")` and `codes::lookup("MOS0033")`
resolve to that exact same static definition. Lookup rejects bare slugs, case variants, whitespace,
and alternate number padding. No numeric alias removal is scheduled.

## CLI migration

Before: `error[MOS0033]: ...`

After: `error[semantic.label-missing (MOS0033)]: ...`

Both `check` and `build` use this format, including spanless diagnostics. Severity, messages, source
spans, suggestions, and phase exit behavior are unchanged. Tools parsing the old bracket format must
update; retaining the numeric token does not preserve that old text format.

## Editor migration

Published diagnostics contain:

```json
{
	"code": "semantic.label-missing",
	"codeDescription": {
		"href": "https://github.com/kjanat/mosaic/blob/master/docs/diagnostic-codes.md#semantic.label-missing"
	},
	"data": { "legacyCode": "MOS0033" }
}
```

This example omits the unchanged range, severity, source, and message fields. Clients matching
`Diagnostic.code` must migrate to semantic IDs or read `data.legacyCode`. Quick-fix titles show
`semantic.label-missing (MOS0033): ...`; edits and applicability are unchanged. Documentation links
use explicit per-rule anchors, and become available on the default branch when this change merges.

## Validation and scope

Registry invariants check identifier grammar, uniqueness, exact lookup of both spellings, and
identity stability under metadata changes. Catalog drift tests cover both identities, slugs,
severity, owner, summary, category placement, and explicit link anchors. CLI tests exercise the new
format; LSP projection and real-process protocol tests verify the wire identity and metadata.
Existing compiler tests continue asserting stable numeric definitions where no display is involved.

This change does not add configurable severity, suppression, new diagnostic conditions, or an
explanation subcommand. Future configuration can use the shared lookup API for either spelling.
