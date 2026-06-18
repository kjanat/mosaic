//! Black-box tests for the `mos` binary. They exercise the manifest
//! §15.1 subcommands by spawning `cargo run -p mos --` with a
//! temporary working directory.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration tests panic loudly on setup failure"
)]

use std::path::Path;
use std::process::Command;

fn mos_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mos")
}

fn run(args: &[&str], cwd: &Path) -> (i32, String, String) {
    let output = Command::new(mos_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn mos binary");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn write_file(dir: &Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write fixture");
    path
}

fn temp_dir(label: &str) -> tempdir::Dir {
    tempdir::Dir::new(label)
}

#[test]
fn check_clean_source_succeeds() {
    let dir = temp_dir("mos-check-ok");
    write_file(dir.path(), "main.mos", "= Title\n\nbody paragraph\n");
    let (code, stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.starts_with("ok:"), "stdout={stdout:?}");
}

#[test]
fn check_directory_uses_manifest_entry() {
    let dir = temp_dir("mos-check-dir-manifest");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(
        &dir.path().join("doc"),
        "mosaic.toml",
        "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"chapter.mos\"\n",
    );
    write_file(&dir.path().join("doc"), "chapter.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["check", "doc"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.starts_with("ok:"), "stdout={stdout:?}");
}

#[test]
fn check_directory_without_manifest_uses_main_mos() {
    let dir = temp_dir("mos-check-dir-main");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(&dir.path().join("doc"), "main.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["check", "doc"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.starts_with("ok:"), "stdout={stdout:?}");
}

#[test]
fn check_accepts_many_entries_and_skips_non_mos_files() {
    let dir = temp_dir("mos-check-many");
    std::fs::create_dir(dir.path().join("one")).expect("create first fixture dir");
    std::fs::create_dir(dir.path().join("two")).expect("create second fixture dir");
    write_file(dir.path(), "AGENTS.md", "not a Mosaic source\n");
    write_file(&dir.path().join("one"), "main.mos", "= One\n\nbody\n");
    write_file(&dir.path().join("two"), "main.mos", "= Two\n\nbody\n");

    let (code, stdout, stderr) = run(&["check", "AGENTS.md", "one", "two"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert_eq!(stdout.matches("ok:").count(), 2, "stdout={stdout:?}");
    assert!(stderr.is_empty(), "stderr={stderr:?}");
}

#[test]
fn check_many_entries_fails_if_any_entry_fails() {
    let dir = temp_dir("mos-check-many-fail");
    write_file(dir.path(), "good.mos", "= Good\n\nbody\n");
    write_file(dir.path(), "bad.mos", "#set page(\nunclosed\n");

    let (code, stdout, stderr) = run(&["check", "good.mos", "bad.mos"], dir.path());

    assert_eq!(code, 1, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("ok:"), "stdout={stdout:?}");
    assert!(stderr.contains("error[MOS0016]"), "stderr={stderr:?}");
}

#[test]
fn check_unterminated_set_fails() {
    let dir = temp_dir("mos-check-err");
    write_file(dir.path(), "main.mos", "#set page(\nunclosed\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0016]"), "stderr={stderr:?}");
}

#[test]
fn check_warns_on_unterminated_emphasis_but_succeeds() {
    let dir = temp_dir("mos-check-warn");
    write_file(dir.path(), "main.mos", "= Title\n\n*unclosed\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 0);
    assert!(stderr.contains("warning[MOS0031]"), "stderr={stderr:?}");
    assert!(
        stderr.contains("help: insert `*`"),
        "unterminated emphasis should render its structured insertion fix: {stderr:?}"
    );
}

#[test]
fn check_missing_file_fails() {
    let dir = temp_dir("mos-check-missing");
    let (code, _stdout, stderr) = run(&["check", "no-such.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("cannot read"), "stderr={stderr:?}");
}

#[test]
fn check_crlf_source_does_not_leak_carriage_return() {
    // Regression: the rendered source line under a diagnostic must
    // not include the trailing `\r` from CRLF line endings, which
    // would mangle alignment when stderr handles `\r` as a column
    // reset.
    let dir = temp_dir("mos-check-crlf");
    write_file(dir.path(), "main.mos", "= Title\r\n*unclosed\r\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 0);
    assert!(stderr.contains("warning[MOS0031]"), "stderr={stderr:?}");
    // The line above the caret should be the bare paragraph text,
    // with no CR character anywhere in the diagnostic frame.
    assert!(
        !stderr.contains('\r'),
        "stderr leaked a carriage return: {stderr:?}"
    );
}

#[test]
fn build_emits_pdf() {
    // `mos build` parses, lowers, lays out, and writes
    // `build/<stem>.pdf`. The output must be a syntactically valid
    // PDF that lopdf can parse, with at least one page and the
    // heading text visible somewhere in the content streams.
    let dir = temp_dir("mos-build");
    // Pin Helvetica so the content stream emits ASCII byte strings
    // that the byte-grep assertions below can find. The default font
    // family is Noto Sans (embedded TTF, hex CID encoding): see
    // `cyrillic_routes_through_embedded_font` below for the embedded
    // path's smoke test.
    write_file(
        dir.path(),
        "main.mos",
        "#set text(font: \"Helvetica\")\n\
         = Title\n\nbody paragraph with *italic* word\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("wrote "), "stdout={stdout:?}");

    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path).expect("pdf written");
    assert!(bytes.starts_with(b"%PDF-"), "missing header");
    assert!(
        bytes.windows(5).any(|w| w == b"%%EOF"),
        "missing %%EOF marker"
    );

    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf parse");
    let pages = doc.get_pages();
    assert!(!pages.is_empty(), "no pages");

    // Decode each page's content streams and confirm the heading
    // text shows up at least once. We grep raw bytes because lopdf's
    // text extraction over standard-14 fonts requires more setup
    // than the smoke test needs.
    let mut found_title = false;
    for &page_id in pages.values() {
        let content = doc.get_page_content(page_id).expect("content");
        if content.windows(b"(Title)".len()).any(|w| w == b"(Title)") {
            found_title = true;
            break;
        }
    }
    assert!(found_title, "Title text not found in any content stream");
}

#[test]
fn build_ignores_byte_zero_shebang() {
    let dir = temp_dir("mos-build-shebang");
    write_file(
        dir.path(),
        "main.mos",
        "#!/usr/bin/env -S mos build --open\n\
         #set text(font: \"Helvetica\")\n\
         = Scripted\n\nbody paragraph\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("wrote "), "stdout={stdout:?}");

    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path);
    assert!(
        bytes.is_ok(),
        "pdf written at {}: {:?}",
        pdf_path.display(),
        bytes.as_ref().err()
    );
    let bytes = bytes.unwrap_or_default();
    let doc = lopdf::Document::load_mem(&bytes);
    assert!(doc.is_ok(), "lopdf parse: {:?}", doc.as_ref().err());
    let doc = doc.unwrap_or_else(|_| lopdf::Document::with_version("1.5"));
    let mut combined: Vec<u8> = Vec::new();
    for &page_id in doc.get_pages().values() {
        let content = doc.get_page_content(page_id);
        assert!(
            content.is_ok(),
            "page content: {:?}",
            content.as_ref().err()
        );
        combined.extend(content.unwrap_or_default());
    }

    let has = |needle: &[u8]| combined.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"(Scripted)"), "heading not rendered in PDF stream");
    assert!(
        !has(b"usr/bin/env") && !has(b"mos build") && !has(b"#!"),
        "shebang rendered into PDF stream"
    );
}

#[test]
fn build_renders_section_numbers_and_resolves_references() {
    // End-to-end MVP 1: a multi-section doc with an `@ref` produces a
    // PDF whose content stream contains the rendered section number
    // (`1.`) ahead of the heading text *and* the resolved reference
    // text (the target's number) instead of the bare label.
    let dir = temp_dir("mos-build-xref");
    write_file(
        dir.path(),
        "main.mos",
        "#set text(font: \"Helvetica\")\n\
         = Introduction <intro>\n\n= Methods\n\nsee @intro for context\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path).expect("pdf written");
    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf parse");
    let pages = doc.get_pages();

    // Concatenate every page's content so we can grep for the
    // rendered tokens regardless of which page they ended up on.
    let mut combined: Vec<u8> = Vec::new();
    for &page_id in pages.values() {
        combined.extend(doc.get_page_content(page_id).expect("content"));
    }

    let has = |needle: &[u8]| combined.windows(needle.len()).any(|w| w == needle);
    assert!(
        has(b"(1.)") && has(b"(Introduction)"),
        "expected `1.` and `Introduction` runs in PDF stream"
    );
    assert!(
        has(b"(2.)") && has(b"(Methods)"),
        "expected `2.` and `Methods` runs in PDF stream"
    );
    // `@intro` resolves to section 1, so the rendered reference is `1`.
    // The bare label must NOT appear; that would mean the resolver
    // didn't run.
    assert!(has(b"(1)"), "expected resolved reference text `1`");
    assert!(
        !has(b"(intro)") && !has(b"(?intro?)"),
        "reference left unresolved in PDF stream"
    );
}

#[test]
fn build_resolves_page_references_to_page_numbers() {
    // End-to-end #72: `@page(label)` renders the target's printed page number,
    // resolved through the resolve↔layout fixpoint. The intro lands on page 1,
    // so the rendered run is `(1)`: section numbers render as `(1.)`, so this
    // does not collide, and the `?intro?` placeholder must be gone.
    let dir = temp_dir("mos-build-pageref");
    write_file(
        dir.path(),
        "main.mos",
        "#set text(font: \"Helvetica\")\n\
         = Intro <intro>\n\nSee the intro on page @page(intro).\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path).expect("pdf written");
    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf parse");
    let mut combined: Vec<u8> = Vec::new();
    for &page_id in doc.get_pages().values() {
        combined.extend(doc.get_page_content(page_id).expect("content"));
    }
    let has = |needle: &[u8]| combined.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"(1)"), "expected resolved page number `1` run");
    assert!(
        !has(b"(?intro?)"),
        "page reference left unresolved in PDF stream"
    );
}

#[test]
fn check_reports_unknown_page_reference_label() {
    // An undeclared label in `@page(...)` is MOS0033 at check time, without
    // laying the document out: same as a bad `@ref`.
    let dir = temp_dir("mos-check-pageref-mos0033");
    write_file(dir.path(), "main.mos", "see page @page(no:such)\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0033]"), "stderr={stderr:?}");
}

#[test]
fn check_reports_unknown_label() {
    // MOS0033 surfaces through `mos check` so editor integration sees
    // unresolved references without having to drive the build.
    let dir = temp_dir("mos-check-mos0033");
    write_file(dir.path(), "main.mos", "see @no:such\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0033]"), "stderr={stderr:?}");
}

#[test]
fn check_renders_unknown_label_suggestion() {
    let dir = temp_dir("mos-check-mos0033-suggestion");
    write_file(dir.path(), "main.mos", "= Intro <intro>\n\nsee @intrdo\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0033]"), "stderr={stderr:?}");
    assert!(
        stderr.contains("help: replace `@intrdo` with `@intro`"),
        "stderr={stderr:?}"
    );
}

#[test]
fn check_reports_duplicate_label() {
    let dir = temp_dir("mos-check-mos0030");
    write_file(dir.path(), "main.mos", "= A <dup>\n\n= B <dup>\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0030]"), "stderr={stderr:?}");
    assert!(
        stderr.contains("help: replace `dup` with `dup-2`"),
        "stderr={stderr:?}"
    );
}

#[test]
fn build_renders_structured_suggestions_before_phase_exit() {
    let dir = temp_dir("mos-build-suggestion");
    write_file(dir.path(), "main.mos", "= Intro <intro>\n\nsee @intrdo\n");
    let (code, _stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[MOS0033]"), "stderr={stderr:?}");
    assert!(
        stderr.contains("help: replace `@intrdo` with `@intro`"),
        "stderr={stderr:?}"
    );
}

#[test]
fn build_honors_set_directives() {
    // End-to-end MVP 1.5: `#set page(paper: "A5")` shrinks the page
    // and `#set document(title: ..., author: ...)` populates the
    // PDF Info dictionary.
    let dir = temp_dir("mos-build-set");
    write_file(
        dir.path(),
        "main.mos",
        concat!(
            "#set document(title: \"Hello\", author: \"Kaj\")\n",
            "#set page(paper: \"A5\", margin: 30mm)\n",
            "#set text(size: 14pt)\n",
            "\n",
            "= Title\n",
            "\nbody\n",
        ),
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path).expect("pdf written");
    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf parse");

    // Page 1 must have the A5 MediaBox: 148 × 210 mm = 419.5 × 595.3 pt.
    let pages = doc.get_pages();
    let &first_page_id = pages.values().next().expect("at least one page");
    let mb = doc
        .get_object(first_page_id)
        .and_then(|o| o.as_dict())
        .and_then(|d| d.get(b"MediaBox"))
        .and_then(|m| m.as_array())
        .expect("MediaBox array");
    let width = read_mediabox_dim(&mb[2]);
    let height = read_mediabox_dim(&mb[3]);
    let expected_w = 148.0_f32 * 72.0 / 25.4;
    let expected_h = 210.0_f32 * 72.0 / 25.4;
    assert!(
        (width - expected_w).abs() < 1.0,
        "MediaBox width = {width}, expected ~{expected_w}"
    );
    assert!(
        (height - expected_h).abs() < 1.0,
        "MediaBox height = {height}, expected ~{expected_h}"
    );

    // Info dictionary populated. lopdf parses /Info as a Reference; we
    // grep the raw bytes for the title/author payloads to keep the
    // assertion backend-agnostic.
    assert!(
        bytes.windows(b"Hello".len()).any(|w| w == b"Hello"),
        "title not found in PDF"
    );
    assert!(
        bytes.windows(b"Kaj".len()).any(|w| w == b"Kaj"),
        "author not found in PDF"
    );
}

/// Read a `MediaBox` numeric entry as `f32`. `lopdf` returns either a
/// `Real` (already `f32`) or an `Integer`; we range-check the integer
/// form so the cast is exact.
fn read_mediabox_dim(value: &lopdf::Object) -> f32 {
    if let Ok(f) = value.as_float() {
        return f;
    }
    let n = value.as_i64().expect("MediaBox dim is numeric");
    assert!(
        (-(1_i64 << 24)..=(1_i64 << 24)).contains(&n),
        "MediaBox dim {n} outside f32-exact range"
    );
    #[allow(
        clippy::cast_precision_loss,
        reason = "range-checked above to fit in 24 bits"
    )]
    let f = n as f32;
    f
}

#[test]
fn build_fails_on_layout_error_does_not_emit_pdf() {
    // MOS0017 (unknown paper) is a layout-level Error. The CLI must
    // surface it and exit non-zero rather than writing a "successful"
    // PDF with broken config.
    let dir = temp_dir("mos-build-layout-error");
    write_file(
        dir.path(),
        "main.mos",
        "#set page(paper: \"Foolscap\")\n\n= T\n\nbody\n",
    );
    let (code, _stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 1, "stderr={stderr:?}");
    assert!(stderr.contains("error[MOS0017]"), "stderr={stderr:?}");
    assert!(
        !dir.path().join("build").join("main.pdf").exists(),
        "PDF should not be written on layout error"
    );
}

#[test]
fn build_creates_output_directory() {
    // Sanity: the `build/` directory shouldn't need to pre-exist.
    let dir = temp_dir("mos-build-mkdir");
    write_file(dir.path(), "main.mos", "= Title\n\nbody\n");
    let (code, _stdout, _stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0);
    assert!(dir.path().join("build").is_dir(), "build/ not created");
    assert!(dir.path().join("build/main.pdf").exists());
}

#[test]
fn build_directory_uses_manifest_entry() {
    let dir = temp_dir("mos-build-dir-manifest");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(
        &dir.path().join("doc"),
        "mosaic.toml",
        "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"chapter.mos\"\n",
    );
    write_file(&dir.path().join("doc"), "chapter.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "doc"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("build/chapter.pdf"), "stdout={stdout:?}");
    assert!(dir.path().join("doc/build/chapter.pdf").exists());
    assert!(!dir.path().join("doc/demo.pdf").exists());
    assert!(!dir.path().join("build/chapter.pdf").exists());
}

#[test]
fn build_directory_uses_declared_pdf_output() {
    let dir = temp_dir("mos-build-output-pdf");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(
        &dir.path().join("doc"),
        "mosaic.toml",
        "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"main.mos\"\n\n[output]\npdf = \"demo.pdf\"\n",
    );
    write_file(&dir.path().join("doc"), "main.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "doc"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("doc/demo.pdf"), "stdout={stdout:?}");
    assert!(dir.path().join("doc/demo.pdf").exists());
    assert!(!dir.path().join("doc/build/main.pdf").exists());
}

#[test]
fn build_rejects_manifest_output_outside_project() {
    let dir = temp_dir("mos-build-output-bad");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(
        &dir.path().join("doc"),
        "mosaic.toml",
        "[project]\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"main.mos\"\n\n[output]\npdf = \"../demo.pdf\"\n",
    );
    write_file(&dir.path().join("doc"), "main.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "doc"], dir.path());

    assert_eq!(code, 1, "stdout={stdout} stderr={stderr}");
    assert!(stdout.is_empty(), "stdout={stdout:?}");
    assert!(
        stderr.contains("invalid PDF output path"),
        "stderr={stderr:?}"
    );
}

#[test]
fn build_many_project_directories_uses_declared_outputs() {
    let dir = temp_dir("mos-build-many-projects");
    for name in ["one", "two"] {
        let project_dir = dir.path().join(name);
        std::fs::create_dir(&project_dir).expect("create fixture dir");
        write_file(
            &project_dir,
            "mosaic.toml",
            &format!(
                "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"main.mos\"\n\n[output]\npdf = \"{name}.pdf\"\n"
            ),
        );
        write_file(&project_dir, "main.mos", &format!("= {name}\n\nbody\n"));
    }

    let (code, stdout, stderr) = run(&["build", "one", "two"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    for name in ["one", "two"] {
        assert!(dir.path().join(name).join(format!("{name}.pdf")).exists());
        assert!(!dir.path().join(name).join("build/main.pdf").exists());
    }
}

#[test]
fn build_directory_without_manifest_writes_inside_that_directory() {
    let dir = temp_dir("mos-build-dir-main");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(&dir.path().join("doc"), "main.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "doc"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("doc/build/main.pdf"), "stdout={stdout:?}");
    assert!(dir.path().join("doc/build/main.pdf").exists());
    assert!(!dir.path().join("build/main.pdf").exists());
}

#[test]
fn build_file_path_writes_next_to_source_file() {
    let dir = temp_dir("mos-build-file-parent");
    std::fs::create_dir(dir.path().join("doc")).expect("create fixture dir");
    write_file(&dir.path().join("doc"), "main.mos", "= Title\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "doc/main.mos"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("doc/build/main.pdf"), "stdout={stdout:?}");
    assert!(dir.path().join("doc/build/main.pdf").exists());
    assert!(!dir.path().join("build/main.pdf").exists());
}

#[test]
fn build_accepts_many_entries() {
    let dir = temp_dir("mos-build-many");
    write_file(dir.path(), "one.mos", "= One\n\nbody\n");
    write_file(dir.path(), "two.mos", "= Two\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "one.mos", "two.mos"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(dir.path().join("build/one.pdf").exists());
    assert!(dir.path().join("build/two.pdf").exists());
}

#[test]
fn build_skips_non_mos_files_when_multiple_entries_provided() {
    let dir = temp_dir("mos-build-skip-non-mos");
    write_file(dir.path(), "README.md", "not a source\n");
    write_file(dir.path(), "one.mos", "= One\n\nbody\n");

    let (code, stdout, stderr) = run(&["build", "README.md", "one.mos"], dir.path());

    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stderr.is_empty(), "stderr={stderr:?}");
    assert!(dir.path().join("build/one.pdf").exists());
}

/// Build a 4×3 RGBA PNG by hand without depending on an `image` crate
/// in this crate's dev-dependencies. The bytes here are a minimal
/// PNG: 8-byte signature, IHDR, IDAT (with a zlib-wrapped
/// uncompressed deflate block), and IEND. The colour space is
/// RGBA8.
fn write_tiny_png(path: &Path) {
    // Construct the IDAT body: for a 4×3 image with one filter byte
    // per row + 4 RGBA bytes per pixel × 4 pixels = 17 bytes per row.
    let width: u32 = 4;
    let height: u32 = 3;
    let mut raw: Vec<u8> = Vec::new();
    for y in 0..height {
        raw.push(0); // filter byte: None
        for x in 0..width {
            // Quasi-random pattern; we only care that the bytes
            // decode losslessly.
            let r = ((x * 50 + y * 30) & 0xff) as u8;
            let g = ((x * 30 + y * 70) & 0xff) as u8;
            let b = ((x * 90 + y * 20) & 0xff) as u8;
            raw.extend_from_slice(&[r, g, b, 255]);
        }
    }
    // zlib wrap + uncompressed deflate block (BTYPE=00).
    let mut zlib_body: Vec<u8> = Vec::new();
    zlib_body.push(0x78);
    zlib_body.push(0x01); // CMF + FLG (no preset dict, fastest)
    // Deflate uncompressed blocks: BFINAL=1, BTYPE=00, then LEN (u16
    // LE), NLEN, and the raw data. The PNG spec requires a single
    // zlib stream; a single stored block is enough for tiny inputs.
    zlib_body.push(0x01); // BFINAL=1, BTYPE=00
    let len = u16::try_from(raw.len()).unwrap();
    zlib_body.extend_from_slice(&len.to_le_bytes());
    zlib_body.extend_from_slice(&(!len).to_le_bytes());
    zlib_body.extend_from_slice(&raw);
    // Adler-32 of `raw`.
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    zlib_body.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png: Vec<u8> = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let write_chunk = |out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(u32::try_from(data.len()).unwrap()).to_be_bytes());
        out.extend_from_slice(ty);
        out.extend_from_slice(data);
        let mut crc = 0xffff_ffff_u32;
        for &b in ty.iter().chain(data.iter()) {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        out.extend_from_slice(&(crc ^ 0xffff_ffff).to_be_bytes());
    };
    // IHDR: width(4), height(4), bit-depth(1), colour-type(1),
    // compression(1), filter(1), interlace(1). 6 = RGBA.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut png, b"IHDR", &ihdr);
    write_chunk(&mut png, b"IDAT", &zlib_body);
    write_chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).expect("write PNG fixture");
}

#[test]
fn build_emits_pdf_with_image_xobject() {
    // End-to-end: a Mosaic source with `#image(...)` + `#figure(...)`
    // produces a PDF whose Image XObject carries the expected
    // /Width, /Height, /ColorSpace, and /Filter entries.
    let dir = temp_dir("mos-build-image");
    write_tiny_png(&dir.path().join("scan.png"));
    write_file(
        dir.path(),
        "main.mos",
        "#set text(font: \"Helvetica\")\n\
         = Picture\n\n\
         #image(\"scan.png\")\n\n\
         #figure(image: \"scan.png\", caption: \"A tiny scan.\")\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    let pdf_path = dir.path().join("build").join("main.pdf");
    let bytes = std::fs::read(&pdf_path).expect("pdf written");
    let doc = lopdf::Document::load_mem(&bytes).expect("lopdf parse");
    // The dedup pass should fold two `scan.png` references into a
    // single Image XObject.
    let mut image_streams = 0;
    let mut dims = (0_i64, 0_i64);
    for obj in doc.objects.values() {
        if let lopdf::Object::Stream(s) = obj
            && let Ok(lopdf::Object::Name(n)) = s.dict.get(b"Subtype")
            && n == b"Image"
        {
            image_streams += 1;
            dims.0 = s.dict.get(b"Width").unwrap().as_i64().unwrap();
            dims.1 = s.dict.get(b"Height").unwrap().as_i64().unwrap();
        }
    }
    assert_eq!(image_streams, 1, "expected one dedup'd Image XObject");
    assert_eq!(dims, (4, 3), "Image XObject /Width and /Height mismatch");
    // The page content streams reference the image via /Im0 Do.
    let pages = doc.get_pages();
    let mut found_do = false;
    for &page_id in pages.values() {
        let content = doc.get_page_content(page_id).expect("content");
        if content.windows(b"/Im0 Do".len()).any(|w| w == b"/Im0 Do") {
            found_do = true;
            break;
        }
    }
    assert!(found_do, "/Im0 Do operator not found in content stream");
}

#[test]
fn build_fails_when_image_path_is_missing() {
    // MOS0012 from the resolver: missing image file => non-zero exit.
    let dir = temp_dir("mos-build-missing-img");
    write_file(dir.path(), "main.mos", "#image(\"does-not-exist.png\")\n");
    let (code, _stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_ne!(code, 0);
    assert!(stderr.contains("MOS0012"), "stderr={stderr:?}");
}

#[test]
fn check_collects_multiple_errors_in_one_pass() {
    // Phase-barrier fail-fast collects every diagnostic in a phase before
    // stopping: it does not bail on the first error. Two malformed `#set`
    // directives must both surface as MOS0022, not just the first.
    let dir = temp_dir("mos-check-multi-err");
    write_file(dir.path(), "main.mos", "#set a(x: -)\n#set b(y: -)\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_ne!(code, 0, "stderr={stderr:?}");
    assert_eq!(
        stderr.matches("error[MOS0022]").count(),
        2,
        "both malformed directives should be reported in one pass: {stderr:?}"
    );
}

#[test]
fn build_substituted_font_emits_notice_and_succeeds() {
    // An unknown font family is a Notice (MOS0018), not a failure: the
    // build substitutes bundled Noto Sans, prints `notice[MOS0018]`, and
    // still exits 0 with a PDF written.
    let dir = temp_dir("mos-build-notice-font");
    write_file(
        dir.path(),
        "main.mos",
        "#set text(font: \"Nonesuch\")\n= Title\n\nbody\n",
    );
    let (code, stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0, "notice must not fail the build; stderr={stderr:?}");
    assert!(
        stderr.contains("notice[MOS0018]"),
        "expected a notice for the substituted font, got {stderr:?}"
    );
    assert!(stdout.starts_with("wrote "), "stdout={stdout:?}");
}

mod tempdir {
    //! Tiny scoped tempdir helper. Avoids pulling in a runtime
    //! dependency for one test file.

    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct Dir {
        path: PathBuf,
    }

    impl Dir {
        pub(crate) fn new(label: &str) -> Self {
            let mut p = std::env::temp_dir();
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            // Combine timestamp + per-process atomic counter + PID so
            // parallel tests inside the same binary cannot collide on
            // a coarse clock, and so one test's `Drop` cannot nuke
            // another's fixtures.
            let seq = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            p.push(format!("{label}-{n}-{seq}-{}", std::process::id()));
            std::fs::create_dir(&p).expect("create temp test dir");
            Self { path: p }
        }

        pub(crate) fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
