//! Type 0 CID-keyed font emission for bundled embedded faces.
//!
//! Each embedded face used in a document is emitted as five indirect
//! objects:
//!
//! 1. `Font` dict (`Type 0`, `Identity-H`, descendant + `ToUnicode` refs)
//! 2. `CIDFont` dict (`CIDFontType2`, `/CIDSystemInfo`, `/W`,
//!    `/CIDToGIDMap /Identity`, descriptor ref)
//! 3. `FontDescriptor` dict (bbox, ascent, descent, italic angle,
//!    stem widths, `/FontFile2` ref)
//! 4. `/FontFile2` stream (the subsetted TTF, with `/Length1` =
//!    uncompressed size)
//! 5. `/ToUnicode` `CMap` (maps each subset CID back to the source
//!    cluster's UTF-8 codepoints)
//!
//! The subset is built per-document over the union of glyph IDs seen
//! across every run that uses the face. With `/CIDToGIDMap /Identity`
//! the CID in the content stream equals the GID inside the subset
//! font file. `subsetter::GlyphRemapper` provides the original-GID →
//! subset-GID mapping.

use std::collections::{BTreeMap, HashMap};

use mosaic_core::{CoreError, Diagnostic, DiagnosticCode, Result, Severity};
use mosaic_fonts::{EmbeddedFontId, ShapedGlyph};
use mosaic_layout::TextRun;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::writers::FontDescriptor;
use pdf_writer::{Finish, Name, Pdf, Rect, Ref, Str};
use subsetter::GlyphRemapper;

/// PDF objects emitted for one embedded face. The 5 refs are allocated
/// up front so the cross-references resolve.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddedRefs {
    pub font: Ref,
    pub cid_font: Ref,
    pub descriptor: Ref,
    pub font_file: Ref,
    pub to_unicode: Ref,
}

/// Per-face plan: which glyph IDs were used, the subset bytes, the
/// `GlyphRemapper` that maps original → subset GIDs, and the
/// gid-to-source-text map used to build `/ToUnicode`.
pub(crate) struct EmbeddedFontPlan {
    pub id: EmbeddedFontId,
    pub subset_bytes: Vec<u8>,
    pub remapper: GlyphRemapper,
    /// Original GID → source text for that glyph's cluster. For
    /// ligatures (1 glyph, N codepoints) the value is the multi-char
    /// cluster string. For 1:1 mappings (typical LTR) it's a
    /// single-char string. For one-codepoint-many-glyphs
    /// decompositions (rare), the first glyph carries the codepoint
    /// and later glyphs in the same cluster carry empty strings.
    pub gid_to_text: BTreeMap<u16, String>,
}

/// Plan every embedded face touched by `runs`. Returns one
/// [`EmbeddedFontPlan`] per face actually referenced, in stable
/// (`EmbeddedFontId`-sorted) order.
///
/// # Errors
///
/// Returns an error if subsetting fails for a face — only possible
/// with corrupted font data, which the bundled cuts are not.
pub(crate) fn plan_embedded(runs: &[TextRun]) -> Result<Vec<EmbeddedFontPlan>> {
    // gid set + gid → cluster text, per face.
    let mut per_face: HashMap<EmbeddedFontId, (Vec<u16>, BTreeMap<u16, String>)> = HashMap::new();
    for run in runs {
        let Some(id) = run.font.embedded() else {
            continue;
        };
        let entry = per_face.entry(id).or_default();
        accumulate_glyphs(&mut entry.0, &mut entry.1, &run.text, &run.glyphs);
    }
    let mut plans: Vec<EmbeddedFontPlan> = Vec::with_capacity(per_face.len());
    // Iterate ALL ids in fixed order so plan output is deterministic.
    for id in EmbeddedFontId::ALL {
        let Some((gids, gid_to_text)) = per_face.remove(&id) else {
            continue;
        };
        let font = id.data();
        let subset_bytes = mosaic_fonts::subset(font, &gids).map_err(|err| {
            CoreError::Diagnostic(Box::new(Diagnostic {
                severity: Severity::Error,
                code: DiagnosticCode("E091"),
                message: format!("font subsetting failed for {id:?}: {err}"),
                span: None,
                notes: Vec::new(),
                suggestions: Vec::new(),
            }))
        })?;
        let mut all = Vec::with_capacity(gids.len() + 1);
        all.push(0_u16);
        all.extend_from_slice(&gids);
        let remapper = GlyphRemapper::new_from_glyphs(&all);
        plans.push(EmbeddedFontPlan {
            id,
            subset_bytes,
            remapper,
            gid_to_text,
        });
    }
    Ok(plans)
}

fn accumulate_glyphs(
    gids: &mut Vec<u16>,
    gid_to_text: &mut BTreeMap<u16, String>,
    source: &str,
    glyphs: &[ShapedGlyph],
) {
    // Walk glyphs, grouping by cluster, so multi-codepoint clusters
    // (ligatures) map their full text to the first glyph and an
    // empty string to subsequent glyphs in the same cluster.
    let mut i = 0;
    while i < glyphs.len() {
        let cluster = glyphs[i].cluster as usize;
        let mut j = i + 1;
        while j < glyphs.len() && glyphs[j].cluster as usize == cluster {
            j += 1;
        }
        let next_cluster = if j < glyphs.len() {
            glyphs[j].cluster as usize
        } else {
            source.len()
        };
        let cluster_str = source.get(cluster..next_cluster).unwrap_or("");
        for (k, g) in glyphs[i..j].iter().enumerate() {
            gids.push(g.gid);
            // GID 0 is `.notdef` — rustybuzz emits it for codepoints
            // the face doesn't cover. Recording a Unicode mapping for
            // it would round-trip every unsupported character back to
            // whichever source text happened to land on GID 0 first
            // (e.g. `日本` shaped against a Latin-only face would
            // round-trip `.notdef` glyphs to `日`). Leaving it out of
            // `gid_to_text` keeps the CMap silent on `.notdef`, which
            // is the right behaviour: PDF readers treat a missing
            // bfchar entry as "no Unicode equivalent".
            if g.gid == 0 {
                continue;
            }
            gid_to_text.entry(g.gid).or_insert_with(|| {
                if k == 0 {
                    cluster_str.to_owned()
                } else {
                    String::new()
                }
            });
        }
        i = j;
    }
}

/// Emit the 5 PDF objects for one embedded face. Caller allocates the
/// refs and ensures they're cross-referenced from each page's `/Font`
/// resource dict.
pub(crate) fn emit_embedded(pdf: &mut Pdf, plan: &EmbeddedFontPlan, refs: EmbeddedRefs) {
    let font = plan.id.data();
    let subset_tag = subset_tag(&plan.subset_bytes);
    let base_font = format!("{subset_tag}+{}", font.postscript_name);
    let base_font_bytes = base_font.as_bytes();

    // 1. Type 0 font dict.
    let mut type0 = pdf.type0_font(refs.font);
    type0.base_font(Name(base_font_bytes));
    type0.encoding_predefined(Name(b"Identity-H"));
    type0.descendant_font(refs.cid_font);
    type0.to_unicode(refs.to_unicode);
    type0.finish();

    // 2. CIDFont dict.
    let mut cid = pdf.cid_font(refs.cid_font);
    cid.subtype(CidFontType::Type2);
    cid.base_font(Name(base_font_bytes));
    cid.system_info(SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    });
    cid.font_descriptor(refs.descriptor);
    cid.default_width(0.0);
    cid.cid_to_gid_map_predefined(Name(b"Identity"));
    {
        let mut widths = cid.widths();
        // /W array: for each used subset GID, emit its advance in
        // 1/1000 em. `pdf-writer`'s `Widths::consecutive(first, ws)`
        // emits `first [w1 w2 ...]` runs. We group consecutive
        // subset GIDs.
        let upem = f32::from(font.units_per_em);
        let mut entries: Vec<(u16, f32)> = plan
            .gid_to_text
            .keys()
            .filter_map(|&orig_gid| {
                let subset_gid = plan.remapper.get(orig_gid)?;
                let advance_units = font.advance_units(orig_gid);
                let advance_1000 = f32::from(advance_units) * 1000.0 / upem;
                Some((subset_gid, advance_1000))
            })
            .collect();
        entries.sort_by_key(|e| e.0);
        let mut i = 0;
        while i < entries.len() {
            let start = entries[i].0;
            let mut j = i + 1;
            while j < entries.len() && entries[j].0 == entries[j - 1].0 + 1 {
                j += 1;
            }
            widths.consecutive(start, entries[i..j].iter().map(|e| e.1));
            i = j;
        }
    }
    cid.finish();

    // 3. FontDescriptor.
    let mut desc = pdf.font_descriptor(refs.descriptor);
    write_font_descriptor(&mut desc, font, base_font_bytes, refs.font_file);
    desc.finish();

    // 4. /FontFile2 stream — the subsetted TTF.
    {
        let mut stream = pdf.stream(refs.font_file, &plan.subset_bytes);
        let length1 = i32::try_from(plan.subset_bytes.len()).unwrap_or(i32::MAX);
        stream.pair(Name(b"Length1"), length1);
    }

    // 5. ToUnicode CMap.
    let system_info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"UCS"),
        supplement: 0,
    };
    let mut cmap: UnicodeCmap<u16> = UnicodeCmap::new(Name(b"Adobe-Identity-UCS"), system_info);
    // Iterate gid_to_text in subset-GID order so the CMap entries are
    // byte-stable across runs.
    let mut by_subset: Vec<(u16, &str)> = plan
        .gid_to_text
        .iter()
        .filter_map(|(orig, text)| plan.remapper.get(*orig).map(|sub| (sub, text.as_str())))
        .collect();
    by_subset.sort_by_key(|e| e.0);
    for (subset_gid, text) in by_subset {
        // Skip empty mappings: pdf-writer's `pair_with_multiple` with
        // zero codepoints would emit `<XX> <>`, which some readers
        // reject. Trailing glyphs of decompositions inherit through
        // the missing-mapping convention.
        if text.is_empty() {
            continue;
        }
        cmap.pair_with_multiple(subset_gid, text.chars());
    }
    let cmap_bytes = cmap.finish();
    let mut cmap_writer = pdf.cmap(refs.to_unicode, &cmap_bytes);
    cmap_writer.name(Name(b"Adobe-Identity-UCS"));
    cmap_writer.system_info(system_info);
}

fn write_font_descriptor(
    desc: &mut FontDescriptor<'_>,
    font: &mosaic_fonts::EmbeddedFont,
    base_font: &[u8],
    font_file: Ref,
) {
    let scale = 1000.0 / f32::from(font.units_per_em);
    desc.name(Name(base_font));
    desc.flags(FontFlags::from_bits_truncate(font.flags));
    desc.bbox(Rect::new(
        f32::from(font.bbox.0) * scale,
        f32::from(font.bbox.1) * scale,
        f32::from(font.bbox.2) * scale,
        f32::from(font.bbox.3) * scale,
    ));
    desc.italic_angle(font.italic_angle);
    desc.ascent(f32::from(font.ascender) * scale);
    desc.descent(f32::from(font.descender) * scale);
    desc.cap_height(f32::from(font.cap_height) * scale);
    desc.stem_v(f32::from(font.stem_v));
    desc.font_file2(font_file);
}

/// Six-letter uppercase subset tag derived deterministically from the
/// subset bytes. Required by PDF 1.7 §9.6.4 for embedded subsets:
/// the `/BaseFont` and `FontDescriptor` `/FontName` must start with
/// `<6 uppercase letters>+`.
fn subset_tag(subset_bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in subset_bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    let mut tag = String::with_capacity(6);
    for _ in 0..6 {
        let r = (hash % 26) as u8;
        tag.push(char::from(b'A' + r));
        hash /= 26;
    }
    tag
}

/// Encode a run's shaped glyphs as big-endian `u16` pairs for the PDF
/// content stream. Each `ShapedGlyph::gid` is remapped through
/// `plan.remapper` to its subset GID, then written as two bytes.
pub(crate) fn encode_glyph_run(plan: &EmbeddedFontPlan, glyphs: &[ShapedGlyph]) -> Vec<u8> {
    let mut out = Vec::with_capacity(glyphs.len() * 2);
    for g in glyphs {
        let cid = plan.remapper.get(g.gid).unwrap_or(0);
        out.push((cid >> 8) as u8);
        out.push((cid & 0xFF) as u8);
    }
    out
}
