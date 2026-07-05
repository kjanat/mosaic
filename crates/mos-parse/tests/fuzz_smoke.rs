//! Fuzz-smoke test: [`mos_parse::parse`] must never panic, hang, or emit
//! incoherent diagnostics on arbitrary valid-UTF-8 input.
//!
//! No fuzzing dependency is used; a small deterministic xorshift64* PRNG
//! with a fixed seed generates the inputs, so every run exercises the
//! exact same corpus. The corpus mixes random character soup, shuffled
//! `.mos`-flavoured fragments, and handwritten known-nasty cases.

use std::path::PathBuf;

use mos_core::CollectingSink;
use mos_parse::parse;

/// Deterministic xorshift64* PRNG. Fixed seed, zero dependencies.
struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        // xorshift state must be non-zero.
        Self(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A uniform-ish value in `0..bound` (`bound == 0` yields 0).
    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound.max(1)).unwrap_or(1);
        usize::try_from(self.next_u64() % bound).unwrap_or(0)
    }
}

/// Characters for random soup: ASCII structure/marker bytes, multibyte
/// scalars, combining marks, NBSP, soft hyphen, controls, and NUL (valid
/// UTF-8, so `&str` accepts it).
const CHAR_PALETTE: &[char] = &[
    'a', 'b', 'Z', '0', '9', ' ', ' ', '\t', '\n', '\n', '\r', '=', '-', '*', '`', '#', '@', '[',
    ']', '(', ')', '.', ':', ',', '"', '\\', '<', '>', '!', '/', 'é', 'µ', 'ß', '字', '🦀',
    '\u{0301}', '\u{0300}', '\u{20DD}', '\u{00A0}', '\u{00AD}', '\u{0000}', '\u{0007}', '\u{007F}',
    '\u{200B}', '\u{2028}', '\u{FEFF}',
];

/// `.mos`-flavoured fragments, including deliberately broken ones.
const FRAGMENTS: &[&str] = &[
    "= Heading\n",
    "== Sub heading <lbl>\n",
    "=== Deep\n",
    "====\n",
    "=",
    "plain paragraph text\n",
    "- item\n",
    "  - nested item\n",
    "1. ordered\n",
    "2. ordered two\n",
    "   continuation text\n",
    "*emphasis*",
    "**strong**",
    "`code span`",
    "*unclosed emphasis",
    "**",
    "``",
    "`",
    "#set text(size: 12pt)\n",
    "#set page(width: 210mm, height: 297mm)\n",
    "#set ",
    "#set x(",
    "#image(\"a.png\")\n",
    "#image(\"a.png\", width: 5em",
    "#image(",
    "#figure(\"b.png\", caption: \"c\")\n",
    "#figure(\n",
    "#bibliography(\"refs.bib\")\n",
    "#code[[fn main() {}]]\n",
    "#pre[[raw text",
    "#code[=[nested ]] brackets]=]\n",
    "#pre[[",
    "[@key]",
    "[@key2] tail",
    "[@",
    "[@key",
    "@label",
    "@label.",
    "@page(intro)",
    "@page(",
    "<label>",
    "<",
    "\\\\",
    "\\-",
    "\\",
    "\u{00AD}",
    "\u{00A0}",
    "#!/usr/bin/env mos\n",
    "#",
    "#\n",
    "#unknown(1)\n",
    "((((((((",
    "))))))))",
    "[[[[[[[[",
    "]]]]",
    "[=[",
    "]=]",
    "\r\n",
    "\r",
    "\n\n",
    "  \t ",
    "…",
    "🦀 crab 🦀",
    "e\u{0301}\u{0300}",
];

/// Handwritten known-nasty inputs.
const NASTY: &[&str] = &[
    "",
    "\n",
    "\r",
    "\r\n",
    "\u{0000}",
    "\u{FEFF}= BOM heading\n",
    "#!/bin/sh\n= after shebang\n",
    "#",
    "# ",
    "#set",
    "#set (\n",
    "#set text(size: 12p\n",
    "#image(\"unterminated\n",
    "#figure(\"a.png\", caption: \"unterminated\n= next heading\n",
    "#pre[[never closed\n= heading inside\n- list inside\n",
    "#code[=[closer mismatch ]]\n",
    "= *unclosed emphasis in heading\n",
    "= `unclosed code in heading\n",
    "*a**b*c`d[@e@f\\g",
    "[@]",
    "[@ key]",
    "@page()",
    "@page(a b)",
    "<>",
    "< label >",
    "<label",
    "- \n-\n -\n1.\n1.5\n99999999999999999999. huge marker\n",
    "1. a\n  1. b\n    1. c\n  back\nout\n",
    "para one\u{00A0}with\u{00AD}controls\\\\\nand a hard break\n",
    "=======\n",
    "== == ==\n",
    "text with lone \u{0301} combining mark start",
    "mixed\r\nline\rendings\nhere\r\n\r",
    "#set a(b: \"\u{0000}\")\n",
    "#set a(b: -12.5mm, c: 3em, d: 4pt, e: 9999999999999999999999)\n",
];

/// Parse `src` and check the output is coherent: parse returns, the tree
/// targets the file we passed, and every diagnostic/suggestion span stays
/// inside `src` on UTF-8 boundaries.
fn check_parse(src: &str) {
    let mut sink = CollectingSink::new();
    let file = PathBuf::from("fuzz.mos");
    let result = parse(src, &file, &mut sink);
    assert!(result.is_ok(), "parse structurally aborted on {src:?}");
    if let Ok(tree) = result {
        assert_eq!(tree.file, file, "tree file mismatch on {src:?}");
    }
    for diagnostic in sink.diagnostics() {
        let spans = diagnostic
            .span()
            .into_iter()
            .chain(diagnostic.suggestions().iter().map(|fix| &fix.span));
        for span in spans {
            assert!(
                span.end() <= src.len(),
                "span {}..{} out of bounds (len {}) on {src:?}",
                span.start(),
                span.end(),
                src.len()
            );
            assert!(
                src.is_char_boundary(span.start()) && src.is_char_boundary(span.end()),
                "span {}..{} not on char boundaries on {src:?}",
                span.start(),
                span.end()
            );
        }
    }
}

/// Random valid-UTF-8 soup drawn from [`CHAR_PALETTE`].
fn gen_soup(rng: &mut Rng) -> String {
    let len = rng.below(400);
    let mut out = String::with_capacity(len * 4);
    for _ in 0..len {
        out.push(CHAR_PALETTE[rng.below(CHAR_PALETTE.len())]);
    }
    out
}

/// Random concatenation of `.mos`-flavoured [`FRAGMENTS`].
fn gen_fragments(rng: &mut Rng) -> String {
    let count = rng.below(40);
    let mut out = String::new();
    for _ in 0..count {
        out.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
        // Occasionally glue fragments with soup characters instead of
        // clean boundaries.
        if rng.below(4) == 0 {
            out.push(CHAR_PALETTE[rng.below(CHAR_PALETTE.len())]);
        }
    }
    out
}

/// Truncate `src` at an arbitrary char boundary, simulating cut-off input.
fn truncate_at_boundary<'src>(src: &'src str, rng: &mut Rng) -> &'src str {
    let mut cut = rng.below(src.len() + 1);
    while !src.is_char_boundary(cut) {
        cut -= 1;
    }
    &src[..cut]
}

#[test]
fn parse_survives_handwritten_nasty_corpus() {
    for src in NASTY {
        check_parse(src);
    }
}

#[test]
fn parse_survives_random_soup() {
    let mut rng = Rng::new(0x5EED_0001);
    for _ in 0..200 {
        check_parse(&gen_soup(&mut rng));
    }
}

#[test]
fn parse_survives_fragment_mixes() {
    let mut rng = Rng::new(0x5EED_0002);
    for _ in 0..200 {
        let doc = gen_fragments(&mut rng);
        check_parse(&doc);
        check_parse(truncate_at_boundary(&doc, &mut rng));
    }
}

#[test]
fn parse_survives_deeply_nested_lists() {
    // Nested lists are linear since the `ListItem::children` subtree
    // mirror was dropped (each nested list lives only in `blocks`).
    // Depth is still bounded by parser recursion (`parse_list_at` ↔
    // `parse_list_item` recurse once per level), so this pins a depth
    // comfortably below any default-stack limit rather than probing it.
    let mut nested = String::new();
    for depth in 0..256 {
        nested.push_str(&" ".repeat(depth * 2));
        nested.push_str("- item\n");
    }
    check_parse(&nested);
}

#[test]
fn parse_survives_long_runs() {
    // One very long line, one long run of emphasis markers, one long
    // heading, and a long raw block.
    check_parse(&"long line ".repeat(2_000));
    check_parse(&"*".repeat(4_000));
    check_parse(&format!("= {}\n", "深".repeat(3_000)));
    check_parse(&format!("#pre[[{}]]\n", "x\n".repeat(3_000)));
}

#[test]
fn parse_survives_deep_unterminated_nesting() {
    check_parse(&format!("#set a({}", "(".repeat(2_000)));
    check_parse(&"[".repeat(2_000));
}

#[test]
fn parse_survives_crlf_heavy_document() {
    check_parse(&"= h\r\npara\rword\r\n- item\r".repeat(200));
}
