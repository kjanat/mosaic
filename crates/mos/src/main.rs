//! `mos` — command-line interface for the Mosaic typesetting engine.
//!
//! Subcommands mirror manifest §15.1. MVP 0 wires `mos check` end-to-end
//! (read source → parse → lower → report diagnostics); the remaining
//! subcommands stay stubbed until layout (MVP 2) and the PDF backend
//! (MVP 0 §6 stage 9) land.

#![allow(clippy::print_stderr, clippy::print_stdout)]

use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use clap::{Parser, Subcommand};
use mos_core::{
    Diagnostic, DiagnosticAnnotation, DiagnosticResult, DiagnosticSink, Severity, SourceSpan,
    linecol,
};

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
        #[arg(value_name = "PATH")]
        entries: Vec<PathBuf>,
        /// Open the generated PDF after a successful build.
        ///
        /// Use `--open` for the platform default, or `--open=PROGRAM`
        /// to invoke a specific viewer.
        #[arg(long, value_name = "PROGRAM", num_args = 0..=1, require_equals = true)]
        open: Option<Option<String>>,
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
        #[arg(value_name = "PATH")]
        entries: Vec<PathBuf>,
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
        Command::Check { entries } => run_checks(&entries),
        Command::Build {
            entries,
            open,
            frozen: _,
            reproducible: _,
        } => run_builds(&entries, PdfOpen::from_cli(&open)),
        Command::Init { .. } => unimplemented_subcommand("init"),
        Command::Watch { .. } => unimplemented_subcommand("watch"),
        Command::Fmt { .. } => unimplemented_subcommand("fmt"),
        Command::Test => unimplemented_subcommand("test"),
        Command::Profile { .. } => unimplemented_subcommand("profile"),
        Command::Clean => unimplemented_subcommand("clean"),
        Command::Package { .. } => unimplemented_subcommand("package"),
    }
}

fn default_entries(entries: &[PathBuf]) -> Vec<PathBuf> {
    if entries.is_empty() {
        vec![PathBuf::from("main.mos")]
    } else {
        entries.to_owned()
    }
}

fn run_checks(entries: &[PathBuf]) -> ExitCode {
    run_many(entries, run_check)
}

fn run_builds(entries: &[PathBuf], open: PdfOpen<'_>) -> ExitCode {
    run_many(entries, |entry| run_build(entry, open))
}

fn run_many(entries: &[PathBuf], mut run_one: impl FnMut(&Path) -> ExitCode) -> ExitCode {
    let entries = default_entries(entries);
    let many = entries.len() > 1;
    let mut ran = false;
    let mut failed = false;

    for entry in &entries {
        if should_skip_glob_file(entry, many) {
            continue;
        }
        ran = true;
        if run_one(entry) != ExitCode::SUCCESS {
            failed = true;
        }
    }

    if failed || !ran {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn should_skip_glob_file(entry: &Path, many: bool) -> bool {
    many && entry.is_file() && !is_mos_source(entry)
}

fn is_mos_source(entry: &Path) -> bool {
    entry.extension().is_some_and(|ext| ext == "mos")
}

fn unimplemented_subcommand(name: &str) -> ExitCode {
    eprintln!("mos {name}: not yet implemented (see manifest §30 MVP roadmap)");
    ExitCode::FAILURE
}

/// `mos check` — parse + lower the entry file and report diagnostics.
/// Exits 0 if no errors (warnings still print); 1 otherwise.
fn run_check(entry: &Path) -> ExitCode {
    let Ok(entry) = resolve_entry("check", entry).map(|entry| entry.source) else {
        return ExitCode::FAILURE;
    };
    let src = match std::fs::read_to_string(&entry) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("mos check: cannot read `{}`: {err}", entry.display());
            return ExitCode::FAILURE;
        }
    };

    let mut sink = RenderingSink::new(&src);

    // Parse phase. A parse error stops the pipeline before lowering, so
    // the evaluator never runs on a structurally broken tree and the
    // user sees every recoverable syntax diagnostic in one pass.
    let Ok(tree) = mos_parse::parse(&src, &entry, &mut sink) else {
        return ExitCode::FAILURE;
    };
    if sink.had_error() {
        eprintln!(
            "mos check: {} error(s), {} warning(s)",
            sink.errors, sink.warnings
        );
        return ExitCode::FAILURE;
    }

    // Lower + resolve phase.
    let result = mos_eval::lower_tree(&tree);
    let node_count = result.document.len();
    sink.render_all(result.diagnostics);

    if sink.had_error() {
        eprintln!(
            "mos check: {} error(s), {} warning(s)",
            sink.errors, sink.warnings
        );
        ExitCode::FAILURE
    } else {
        println!("ok: {node_count} node(s), {} warning(s)", sink.warnings);
        ExitCode::SUCCESS
    }
}

/// `mos build` — read source, parse, lower, lay out, and emit a PDF
/// to `build/<entry-stem>.pdf`. MVP 0 produces a fixed-A4 document
/// using the standard PDF base fonts (no embedding). Layout warnings
/// (e.g. non-ASCII substitutions) print but don't fail the build.
fn run_build(entry: &Path, open: PdfOpen<'_>) -> ExitCode {
    let Ok(resolved) = resolve_entry("build", entry) else {
        return ExitCode::FAILURE;
    };
    let entry = resolved.source;
    let src = match std::fs::read_to_string(&entry) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("mos build: cannot read `{}`: {err}", entry.display());
            return ExitCode::FAILURE;
        }
    };

    let started = std::time::Instant::now();
    let mut sink = RenderingSink::new(&src);

    // Each phase runs to completion, then the barrier below stops the
    // build before the next phase if any error was collected — so a
    // broken document never reaches PDF emission and writes garbage.
    let Ok(tree) = mos_parse::parse(&src, &entry, &mut sink) else {
        return ExitCode::FAILURE;
    };
    if sink.had_error() {
        return ExitCode::FAILURE;
    }

    let result = mos_eval::lower_tree(&tree);
    sink.render_all(result.diagnostics);
    if sink.had_error() {
        return ExitCode::FAILURE;
    }

    // Layout can produce real errors (MOS0200 unknown paper, MOS0201
    // geometrically invalid margin/leading). Don't ship a PDF with
    // broken config under a success exit code.
    let layout = mos_layout::LayoutEngine::new().layout(&result.document);
    sink.render_all(layout.diagnostics);
    if sink.had_error() {
        return ExitCode::FAILURE;
    }

    let stem = entry.file_stem().map_or_else(
        || std::ffi::OsString::from("out"),
        std::ffi::OsStr::to_os_string,
    );
    let out = resolved.output.unwrap_or_else(|| {
        let mut path = resolved.output_base.join("build");
        path.push(format!("{}.pdf", stem.to_string_lossy()));
        path
    });

    let metadata = mos_pdf::PdfMetadata {
        title: result.metadata.title.clone(),
        author: result.metadata.author.clone(),
        language: result.metadata.language,
    };
    match mos_pdf::emit(&layout.graph, &metadata, &out) {
        Ok(pdf_diagnostics) => {
            sink.render_all(pdf_diagnostics);
            if sink.had_error() {
                return ExitCode::FAILURE;
            }
        }
        Err(err) => {
            match err {
                mos_core::CoreError::Diagnostic(d) => {
                    let _ = sink.emit(*d);
                }
                mos_core::CoreError::Unimplemented(msg) => {
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
    if open.should_open() {
        match open_pdf(&out, open) {
            Ok(()) => println!("opened {}", out.display()),
            Err(err) => {
                eprintln!("mos build: {err}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

struct ResolvedEntry {
    source: PathBuf,
    output_base: PathBuf,
    output: Option<PathBuf>,
}

fn resolve_entry(command: &str, entry: &Path) -> Result<ResolvedEntry, ()> {
    if !entry.is_dir() {
        let output_base = entry
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok(ResolvedEntry {
            source: entry.to_path_buf(),
            output_base,
            output: None,
        });
    }

    let manifest_path = entry.join("mosaic.toml");
    if manifest_path.is_file() {
        let manifest = match mos_packages::ProjectManifest::load(&manifest_path) {
            Ok(manifest) => manifest,
            Err(err) => {
                eprintln!("mos {command}: {err}");
                return Err(());
            }
        };
        return Ok(ResolvedEntry {
            source: entry.join(manifest.project.entry),
            output_base: entry.to_path_buf(),
            output: match manifest.output.pdf.as_deref() {
                Some(path) => Some(resolve_manifest_output(command, entry, path)?),
                None => None,
            },
        });
    }

    Ok(ResolvedEntry {
        source: entry.join("main.mos"),
        output_base: entry.to_path_buf(),
        output: None,
    })
}

fn resolve_manifest_output(command: &str, project_dir: &Path, output: &str) -> Result<PathBuf, ()> {
    let output_path = Path::new(output);
    if output_path.as_os_str().is_empty()
        || output_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        eprintln!(
            "mos {command}: invalid PDF output path `{output}`; use a relative path inside the project"
        );
        return Err(());
    }
    Ok(project_dir.join(output_path))
}

#[derive(Debug, Clone, Copy)]
enum PdfOpen<'a> {
    No,
    Default,
    Program(&'a str),
}

impl<'a> PdfOpen<'a> {
    fn from_cli(open: &'a Option<Option<String>>) -> Self {
        match open {
            None => Self::No,
            Some(None) => Self::Default,
            Some(Some(program)) if program.is_empty() => Self::Default,
            Some(Some(program)) => Self::Program(program.as_str()),
        }
    }

    fn should_open(self) -> bool {
        !matches!(self, Self::No)
    }
}

fn open_pdf(path: &Path, request: PdfOpen<'_>) -> Result<(), String> {
    match request {
        PdfOpen::No => Ok(()),
        PdfOpen::Default => opener::open(path.as_os_str())
            .map_err(|err| format!("could not open `{}`: {err}", path.display())),
        PdfOpen::Program(program) => {
            let mut command = ProcessCommand::new(program);
            command.arg(path);
            let status = command.status().map_err(|err| {
                format!(
                    "could not open `{}` with `{program}`: {err}",
                    path.display()
                )
            })?;
            if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "opener `{program}` failed for `{}` with {status}",
                    path.display()
                ))
            }
        }
    }
}

/// A [`DiagnosticSink`] that renders each diagnostic to stderr as it
/// arrives and tracks error/warning counts. The CLI drives one of these
/// across every phase and checks [`Self::had_error`] at each phase
/// barrier — that, not `Severity::Error` itself, is what stops the build.
struct RenderingSink<'a> {
    src: &'a str,
    errors: usize,
    warnings: usize,
}

impl<'a> RenderingSink<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            errors: 0,
            warnings: 0,
        }
    }

    fn had_error(&self) -> bool {
        self.errors > 0
    }

    /// Render every diagnostic in `diags`. Bridges phases that still
    /// return a `Vec<Diagnostic>` (layout, PDF emit) into the sink.
    fn render_all(&mut self, diags: impl IntoIterator<Item = Diagnostic>) {
        for diag in diags {
            let _ = self.emit(diag);
        }
    }
}

impl DiagnosticSink for RenderingSink<'_> {
    fn emit(&mut self, diagnostic: Diagnostic) -> DiagnosticResult<()> {
        match diagnostic.severity() {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            Severity::Notice => {}
        }
        render_diagnostic(&diagnostic, self.src);
        Ok(())
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Notice => "notice",
    }
}

fn render_diagnostic(diag: &Diagnostic, src: &str) {
    let label = severity_label(diag.severity());
    let code = diag.def().code();
    if let Some(span) = diag.span() {
        let (line, col) = linecol(src, span.start);
        eprintln!(
            "{label}[{code}]: {msg}\n  --> {file}:{line}:{col}",
            msg = diag.message(),
            file = span.file.display(),
        );
        render_span_caret(src, span);
    } else {
        eprintln!("{label}[{code}]: {msg}", msg = diag.message());
    }
    for annotation in diag.annotations() {
        match annotation {
            DiagnosticAnnotation::Related { span, message } => {
                let (line, col) = linecol(src, span.start);
                eprintln!(
                    "  note: {message} ({file}:{line}:{col})",
                    file = span.file.display(),
                );
            }
            DiagnosticAnnotation::Note(message) => eprintln!("  note: {message}"),
            DiagnosticAnnotation::Help(message) => eprintln!("  help: {message}"),
            DiagnosticAnnotation::Hint(message) => eprintln!("  hint: {message}"),
        }
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

#[cfg(test)]
mod tests {
    use super::PdfOpen;

    #[test]
    fn pdf_open_from_cli_distinguishes_absent_default_and_program() {
        assert!(matches!(PdfOpen::from_cli(&None), PdfOpen::No));

        let default = Some(None);
        assert!(matches!(PdfOpen::from_cli(&default), PdfOpen::Default));

        let empty = Some(Some(String::new()));
        assert!(matches!(PdfOpen::from_cli(&empty), PdfOpen::Default));

        let program = Some(Some("zathura".to_owned()));
        assert!(matches!(
            PdfOpen::from_cli(&program),
            PdfOpen::Program("zathura")
        ));
    }
}
