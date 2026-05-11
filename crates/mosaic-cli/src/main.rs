//! `mos` — command-line interface for the Mosaic typesetting engine.
//!
//! Subcommands mirror manifest §15.1. MVP 0 wires `mos check` end-to-end
//! (read source → parse → lower → report diagnostics); the remaining
//! subcommands stay stubbed until layout (MVP 2) and the PDF backend
//! (MVP 0 §6 stage 9) land.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mosaic_core::{Diagnostic, Severity, SourceSpan, linecol};

#[derive(Parser, Debug)]
#[command(
    name = "mos",
    bin_name = "mos",
    version,
    about = "Mosaic — semantic, incremental typesetting compiler",
    long_about = "Mosaic compiles `.mos` source files to PDF, HTML, and EPUB.\n\
                  See manifest.md in the repository root for the full design."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialise a new Mosaic project in the current directory.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Build the project to its declared outputs.
    Build {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
        /// Refuse to update dependencies (manifest §15.3).
        #[arg(long)]
        frozen: bool,
        /// Make the build deterministic (manifest §24).
        #[arg(long)]
        reproducible: bool,
    },

    /// Watch sources and rebuild on change (manifest §8).
    Watch {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Type-check and validate without producing output.
    Check {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Format `.mos` sources (manifest §18).
    Fmt {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Run document and package tests (manifest §28).
    Test,

    /// Profile a build and report layout hot spots (manifest §16).
    Profile {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Remove build artefacts and the local cache.
    Clean,

    /// Bundle a project into a `.mosaicbundle` archive (manifest §15.3).
    Package {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { entry } => run_check(&entry),
        Command::Build {
            entry,
            frozen: _,
            reproducible: _,
        } => run_build(&entry),
        Command::Init { .. } => unimplemented_subcommand("init"),
        Command::Watch { .. } => unimplemented_subcommand("watch"),
        Command::Fmt { .. } => unimplemented_subcommand("fmt"),
        Command::Test => unimplemented_subcommand("test"),
        Command::Profile { .. } => unimplemented_subcommand("profile"),
        Command::Clean => unimplemented_subcommand("clean"),
        Command::Package { .. } => unimplemented_subcommand("package"),
    }
}

fn unimplemented_subcommand(name: &str) -> ExitCode {
    eprintln!("mos {name}: not yet implemented (see manifest §30 MVP roadmap)");
    ExitCode::FAILURE
}

/// `mos check` — parse + lower the entry file and report diagnostics.
/// Exits 0 if no errors (warnings still print); 1 otherwise.
fn run_check(entry: &Path) -> ExitCode {
    let src = match std::fs::read_to_string(entry) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("mos check: cannot read `{}`: {err}", entry.display());
            return ExitCode::FAILURE;
        }
    };

    let result = mosaic_eval::lower(&src, entry);
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    for diag in &result.diagnostics {
        render_diagnostic(diag, &src);
    }

    if errors == 0 {
        println!(
            "ok: {} node(s), {warnings} warning(s)",
            result.document.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("mos check: {errors} error(s), {warnings} warning(s)");
        ExitCode::FAILURE
    }
}

/// `mos build` — read source, parse, lower, lay out, and emit a PDF
/// to `build/<entry-stem>.pdf`. MVP 0 produces a fixed-A4 document
/// using the standard PDF base fonts (no embedding). Layout warnings
/// (e.g. non-ASCII substitutions) print but don't fail the build.
fn run_build(entry: &Path) -> ExitCode {
    let src = match std::fs::read_to_string(entry) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("mos build: cannot read `{}`: {err}", entry.display());
            return ExitCode::FAILURE;
        }
    };

    let started = std::time::Instant::now();
    let result = mosaic_eval::lower(&src, entry);
    for diag in &result.diagnostics {
        render_diagnostic(diag, &src);
    }
    if result.has_errors() {
        return ExitCode::FAILURE;
    }

    let layout = mosaic_layout::LayoutEngine::new().layout(&result.document);
    for diag in &layout.diagnostics {
        render_diagnostic(diag, &src);
    }
    // Layout can now produce real errors (E023 unknown paper, E025
    // geometrically invalid margin/leading). Don't ship a PDF with
    // broken config under a success exit code.
    if layout
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return ExitCode::FAILURE;
    }

    let stem = entry.file_stem().map_or_else(
        || std::ffi::OsString::from("out"),
        std::ffi::OsStr::to_os_string,
    );
    let mut out = PathBuf::from("build");
    out.push(format!("{}.pdf", stem.to_string_lossy()));

    let metadata = mosaic_pdf::PdfMetadata {
        title: result.metadata.title.clone(),
        author: result.metadata.author.clone(),
        language: result.metadata.language,
    };
    match mosaic_pdf::emit(&layout.graph, &metadata, &out) {
        Ok(pdf_diagnostics) => {
            for diag in &pdf_diagnostics {
                render_diagnostic(diag, &src);
            }
            if pdf_diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
            {
                return ExitCode::FAILURE;
            }
        }
        Err(err) => {
            match err {
                mosaic_core::CoreError::Diagnostic(d) => render_diagnostic(&d, &src),
                mosaic_core::CoreError::Unimplemented(msg) => {
                    eprintln!("mos build: {msg}");
                }
            }
            return ExitCode::FAILURE;
        }
    }

    println!(
        "wrote {} in {} ms",
        out.display(),
        started.elapsed().as_millis()
    );
    ExitCode::SUCCESS
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
        Severity::Help => "help",
    }
}

fn render_diagnostic(diag: &Diagnostic, src: &str) {
    let label = severity_label(diag.severity);
    if let Some(span) = &diag.span {
        let (line, col) = linecol(src, span.start);
        eprintln!(
            "{label}[{code}]: {msg}\n  --> {file}:{line}:{col}",
            code = diag.code.0,
            msg = diag.message,
            file = span.file.display(),
        );
        render_span_caret(src, span);
    } else {
        eprintln!(
            "{label}[{code}]: {msg}",
            code = diag.code.0,
            msg = diag.message
        );
    }
    for note in &diag.notes {
        eprintln!("  note: {}", note.message);
    }
    for sug in &diag.suggestions {
        eprintln!("  help: {}", sug.message);
    }
}

fn clamp_to_char_boundary(src: &str, mut offset: usize) -> usize {
    offset = offset.min(src.len());
    while offset > 0 && !src.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn render_span_caret(src: &str, span: &SourceSpan) {
    let (line_no, col) = linecol(src, span.start);
    let span_start = clamp_to_char_boundary(src, span.start);
    let line_start = src[..span_start].rfind('\n').map_or(0, |p| p + 1);
    let raw_line_end = src[line_start..]
        .find('\n')
        .map_or(src.len(), |p| line_start + p);
    // CRLF sources keep the trailing `\r` inside `[line_start, '\n')`;
    // strip it so the caret line lines up with what stderr actually
    // prints.
    let line_end = if raw_line_end > line_start && src.as_bytes()[raw_line_end - 1] == b'\r' {
        raw_line_end - 1
    } else {
        raw_line_end
    };
    let line_text = &src[line_start..line_end];
    // Convert byte offsets into char counts so multibyte UTF-8
    // sequences (e.g. `µ`, `é`) line up with the source above. Clamp
    // both ends to char boundaries first; otherwise a span that
    // straddles a multibyte sequence would panic the slice below.
    let span_byte_end = clamp_to_char_boundary(src, span.end.min(line_end));
    let span_byte_start = clamp_to_char_boundary(src, span_start.min(span_byte_end));
    let caret_chars = src[span_byte_start..span_byte_end].chars().count().max(1);
    eprintln!("   |");
    eprintln!("{line_no:>3}| {line_text}");
    eprintln!(
        "   | {pad}{carets}",
        pad = " ".repeat(col.saturating_sub(1)),
        carets = "^".repeat(caret_chars),
    );
}
