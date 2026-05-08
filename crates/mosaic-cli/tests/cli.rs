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
fn build_succeeds_at_lowering_then_fails_until_pdf_lands() {
    // `mos build` parses + lowers the entry, so a clean source should
    // surface no diagnostics. It then exits 1 because the PDF backend
    // (manifest §6 stage 9 / §21.1) isn't implemented yet.
    let dir = temp_dir("mos-build");
    write_file(dir.path(), "main.mos", "= Title\n\nbody\n");
    let (code, _stdout, stderr) = run(&["build", "main.mos"], dir.path());
    assert_eq!(code, 1);
    assert!(stderr.contains("not yet implemented"), "stderr={stderr:?}");
    assert!(!stderr.contains("error["), "stderr={stderr:?}");
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
