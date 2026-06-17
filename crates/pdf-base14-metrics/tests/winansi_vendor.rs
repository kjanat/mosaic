//! Vendor-equivalence check: the hand-curated `WINANSI_CHAR_MAP`
//! transcribed from PDF 1.7 Annex D.2 Table D.2 must match, byte for
//! byte, the same map re-derived from the Adobe Glyph List at test
//! time. AGL is the oracle; if the two diverge, fix the hand-curated
//! table; the AGL one is the authority for what each glyph name
//! resolves to in Unicode.
//!
//! The AGL data file (`data/agl/glyphlist.txt`, BSD-3-Clause) lives
//! in the crate's repo for this test, but is excluded from the
//! Cargo `include` manifest: it does NOT ship to crates.io. The
//! build script no longer reads it either, so production builds carry
//! no BSD-3-Clause dependency.

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use pdf_base14_metrics::{__WINANSI_CHAR_MAP, winansi_glyph_name};

type TestResult = Result<(), Box<dyn Error>>;

fn agl_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("agl")
        .join("glyphlist.txt")
}

/// Parse the Adobe Glyph List file into a `glyph name → char` map.
/// Skips comment/blank lines and any entry that maps to multiple
/// codepoints (those are out of scope for `WinAnsiEncoding`, which
/// only references single-scalar glyph names).
fn load_agl() -> Result<HashMap<String, char>, Box<dyn Error>> {
    let path = agl_path();
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, hex)) = line.split_once(';') else {
            continue;
        };
        let hex = hex.trim();
        if hex.contains(' ') {
            continue;
        }
        let Ok(scalar) = u32::from_str_radix(hex, 16) else {
            continue;
        };
        let Some(ch) = char::from_u32(scalar) else {
            continue;
        };
        map.insert(name.to_owned(), ch);
    }
    Ok(map)
}

/// Resolve a PostScript glyph name to its Unicode scalar per AGL
/// Specification §2 (single-codepoint cases only; all the `WinAnsi`
/// glyph names are single-component, so the compound-name branches
/// are unreachable here but kept for spec faithfulness).
fn resolve_glyph_name(name: &str, agl: &HashMap<String, char>) -> Option<char> {
    let stripped = name.split_once('.').map_or(name, |(s, _)| s);
    if stripped.contains('_') {
        return None;
    }
    if let Some(&ch) = agl.get(stripped) {
        return Some(ch);
    }
    if let Some(hex) = stripped.strip_prefix("uni") {
        let is_four_upper_hex = hex.len() == 4
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
        if is_four_upper_hex {
            match u32::from_str_radix(hex, 16) {
                Ok(scalar)
                    if (0x0000..=0xD7FF).contains(&scalar)
                        || (0xE000..=0xFFFF).contains(&scalar) =>
                {
                    return char::from_u32(scalar);
                }
                _ => {}
            }
        }
    }
    if let Some(hex) = stripped.strip_prefix('u') {
        let is_upper_hex = (4..=6).contains(&hex.len())
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
        if is_upper_hex {
            match u32::from_str_radix(hex, 16) {
                Ok(scalar)
                    if (0x0000..=0xD7FF).contains(&scalar)
                        || (0xE000..=0x0010_FFFF).contains(&scalar) =>
                {
                    return char::from_u32(scalar);
                }
                _ => {}
            }
        }
    }
    None
}

#[test]
fn hand_curated_matches_agl_vendor() -> TestResult {
    let agl = load_agl()?;
    let mut diffs: Vec<String> = Vec::new();
    for byte in 0u8..=u8::MAX {
        let agl_ch = winansi_glyph_name(byte).and_then(|name| resolve_glyph_name(name, &agl));
        let hand = __WINANSI_CHAR_MAP[byte as usize];
        if hand != agl_ch {
            diffs.push(format!(
                "  0x{byte:02X}: hand-curated = {hand:?}, AGL-derived = {agl_ch:?}"
            ));
        }
    }
    if !diffs.is_empty() {
        return Err(format!(
            "hand-curated WinAnsi map diverges from AGL-derived oracle at {} byte(s):\n{}",
            diffs.len(),
            diffs.join("\n"),
        )
        .into());
    }
    Ok(())
}

#[test]
fn winansi_gaps_match_pdf_spec() {
    // PDF 1.7 Annex D.2 gaps the encoding leaves unassigned. Both
    // the hand-curated table and `winansi_glyph_name` must agree
    // these slots are `None`; this is a sanity check that catches
    // "off-by-one in the gap region" before the broader equivalence
    // test even runs.
    for &gap in &[0x7Fu8, 0x81, 0x8D, 0x8F, 0x90, 0x9D] {
        assert_eq!(
            __WINANSI_CHAR_MAP[gap as usize], None,
            "hand-curated table should leave 0x{gap:02X} unassigned",
        );
        assert_eq!(
            winansi_glyph_name(gap),
            None,
            "WINANSI_TABLE should leave 0x{gap:02X} unassigned",
        );
    }
    // C0 controls 0x00..=0x1F are also unassigned in PDF WinAnsi.
    for byte in 0u8..=0x1F {
        assert_eq!(__WINANSI_CHAR_MAP[byte as usize], None);
        assert_eq!(winansi_glyph_name(byte), None);
    }
}

#[test]
fn winansi_aliases_collapse_to_ascii() {
    // PDF 1.7 Annex D.2 explicitly aliases 0xA0 → `space` and
    // 0xAD → `hyphen` (the glyph names, which AGL resolves to ASCII
    // U+0020 and U+002D; not the Latin-1 NBSP/SHY one might expect
    // from CP1252). This is the single most likely place for a
    // transcription error in the hand-curated table.
    assert_eq!(__WINANSI_CHAR_MAP[0xA0], Some(' '));
    assert_eq!(__WINANSI_CHAR_MAP[0xAD], Some('-'));
    assert_eq!(winansi_glyph_name(0xA0), Some("space"));
    assert_eq!(winansi_glyph_name(0xAD), Some("hyphen"));
}
