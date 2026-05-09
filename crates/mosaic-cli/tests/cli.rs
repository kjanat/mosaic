//! Black-box tests for the `mos` binary. They exercise the manifest
//! §15.1 subcommands by spawning `cargo run -p mosaic-cli --` with a
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
fn check_unterminated_set_fails() {
    let dir = temp_dir("mos-check-err");
    write_file(dir.path(), "main.mos", "#set page(\nunclosed\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[E012]"), "stderr={stderr:?}");
}

#[test]
fn check_warns_on_unterminated_emphasis_but_succeeds() {
    let dir = temp_dir("mos-check-warn");
    write_file(dir.path(), "main.mos", "= Title\n\n*unclosed\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 0);
    assert!(stderr.contains("warning[W021]"), "stderr={stderr:?}");
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
    assert!(stderr.contains("warning[W021]"), "stderr={stderr:?}");
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
    write_file(
        dir.path(),
        "main.mos",
        "= Title\n\nbody paragraph with *italic* word\n",
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
fn build_renders_section_numbers_and_resolves_references() {
    // End-to-end MVP 1: a multi-section doc with an `@ref` produces a
    // PDF whose content stream contains the rendered section number
    // (`1.`) ahead of the heading text *and* the resolved reference
    // text (the target's number) instead of the bare label.
    let dir = temp_dir("mos-build-xref");
    write_file(
        dir.path(),
        "main.mos",
        "= Introduction <intro>\n\n= Methods\n\nsee @intro for context\n",
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
    // The bare label must NOT appear — that would mean the resolver
    // didn't run.
    assert!(has(b"(1)"), "expected resolved reference text `1`");
    assert!(
        !has(b"(intro)") && !has(b"(?intro?)"),
        "reference left unresolved in PDF stream"
    );
}

#[test]
fn check_reports_unknown_label() {
    // E042 surfaces through `mos check` so editor integration sees
    // unresolved references without having to drive the build.
    let dir = temp_dir("mos-check-e042");
    write_file(dir.path(), "main.mos", "see @no:such\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[E042]"), "stderr={stderr:?}");
}

#[test]
fn check_reports_duplicate_label() {
    let dir = temp_dir("mos-check-e041");
    write_file(dir.path(), "main.mos", "= A <dup>\n\n= B <dup>\n");
    let (code, _stdout, stderr) = run(&["check", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("error[E041]"), "stderr={stderr:?}");
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
fn build_creates_output_directory() {
    // Sanity: the `build/` directory shouldn't need to pre-exist.
    let dir = temp_dir("mos-build-mkdir");
    write_file(dir.path(), "main.mos", "= Title\n\nbody\n");
    let (code, _stdout, _stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 0);
    assert!(dir.path().join("build").is_dir(), "build/ not created");
    assert!(dir.path().join("build/main.pdf").exists());
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
