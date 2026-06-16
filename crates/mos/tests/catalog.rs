//! Drift test: the diagnostic catalog (`docs/diagnostic-codes.md`) must
//! stay in lock-step with the registry (`mos_core::codes::ALL`).
//!
//! Rather than parse the markdown tables strictly (brittle against the
//! `dprint` formatter's column alignment), both directions normalise
//! whitespace and compare:
//!
//! 1. **Every registered code has a row.** For each `DiagnosticDef`, the
//!    canonical `| code | slug | severity | owner | summary |` row must
//!    appear in the doc, so renaming a slug, re-severitising a code, or
//!    changing a summary without updating the doc fails CI.
//! 2. **Every documented code is registered.** Any `| MOSxxxx …` table
//!    row in the doc whose code is absent from the registry fails CI:
//!    so a stale row can't linger after a code is retired.

use mos_core::codes;

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/diagnostic-codes.md"
));

/// Collapse every run of ASCII whitespace to a single space so the
/// comparison is immune to `dprint`'s table-column padding.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn code_cell(trimmed: &str) -> &str {
    let row = trimmed.trim_start_matches('|');
    let delimiter = row.find('|');
    assert!(
        delimiter.is_some(),
        "docs/diagnostic-codes.md has a malformed code row without a closing first cell: {trimmed}"
    );
    row[..delimiter.unwrap_or(row.len())].trim()
}

#[test]
fn every_registered_code_has_a_catalog_row() {
    // The catalog organises codes by category; each section's table omits
    // the `Category` column because the section header carries it. The
    // drift check therefore expects the same column shape humans see:
    // `| code | slug | severity | owner | summary |`. The category itself
    // is not blind-matched (the `### Syntax` heading would be too easy to
    // satisfy with a stray substring); instead we re-check below that the
    // matching row sits under the section heading for its category.
    let haystack = normalize(CATALOG);
    for def in codes::ALL {
        let row = format!(
            "| {} | {} | {:?} | {} | {} |",
            def.code(),
            def.slug(),
            def.default_severity(),
            def.owner(),
            def.summary(),
        );
        assert!(
            haystack.contains(&normalize(&row)),
            "docs/diagnostic-codes.md is missing (or disagrees on) the row for {}:\n  expected: {row}",
            def.code(),
        );
    }

    // Section-membership check: walk the catalog top to bottom tracking
    // the current `### <Category>` heading, then verify each code row
    // appears under the heading matching its registered category.
    let mut current_section: Option<String> = None;
    let mut placement: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for line in CATALOG.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("### ") {
            current_section = Some(rest.trim().to_owned());
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with("| MOS") {
            continue;
        }
        let code = code_cell(trimmed).to_owned();
        if let Some(section) = &current_section {
            let previous = placement.get(&code);
            assert!(
                previous.is_none(),
                "docs/diagnostic-codes.md duplicates `{code}` under sections `{}` and `{section}`",
                previous.map_or("", String::as_str),
            );
            placement.insert(code, section.clone());
        }
    }

    for def in codes::ALL {
        let code = def.code().to_string();
        let want = def.category().to_string();
        let got = placement.get(&code).cloned().unwrap_or_default();
        assert_eq!(
            got, want,
            "docs/diagnostic-codes.md lists `{code}` under section `{got}` but its registered category is `{want}`"
        );
    }
}

#[test]
fn every_catalog_code_is_registered() {
    let known: std::collections::BTreeSet<String> =
        codes::ALL.iter().map(|d| d.code().to_string()).collect();

    for line in CATALOG.lines() {
        let trimmed = line.trim_start();
        // Only inspect table data rows that open with a code cell.
        if !trimmed.starts_with("| MOS") {
            continue;
        }
        let code = code_cell(trimmed);
        assert!(
            known.contains(code),
            "docs/diagnostic-codes.md documents `{code}`, which is not in mos_core::codes::ALL"
        );
    }
}
