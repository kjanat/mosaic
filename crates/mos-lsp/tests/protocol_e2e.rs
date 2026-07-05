//! Protocol E2E tests for the `mos-lsp` binary (issue #137).
//!
//! Each test spawns the real server binary (`CARGO_BIN_EXE_mos-lsp`)
//! and speaks `Content-Length`-framed JSON-RPC 2.0 over its stdio,
//! exactly as an editor would. Server output is pumped through a
//! reader thread and consumed via [`std::sync::mpsc::Receiver::recv_timeout`],
//! so a hung or crashed server fails the test with a timeout instead of
//! hanging CI. Tests return [`Result`] and check protocol expectations
//! through fallible [`ensure`] / [`ensure_eq`] helpers instead of
//! panicking assertions.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Upper bound for any single wait: one framed message, or process exit.
const RECV_TIMEOUT: Duration = Duration::from_secs(10);

type BoxError = Box<dyn std::error::Error>;
type TestResult = Result<(), BoxError>;

/// Fallible check: these tests return `Result` and
/// `clippy::panic_in_result_fn` is CI-fatal, so protocol expectations
/// surface as `Err` instead of panicking assertion macros.
fn ensure(condition: bool, what: &str) -> TestResult {
    if condition {
        Ok(())
    } else {
        Err(format!("check failed: {what}").into())
    }
}

/// Fallible equality check carrying both sides in the failure message.
fn ensure_eq<T>(actual: &T, expected: &T, what: &str) -> TestResult
where
    T: PartialEq + std::fmt::Debug + ?Sized,
{
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what}: actual {actual:?}, expected {expected:?}").into())
    }
}

/// A spawned `mos-lsp` process plus the plumbing to talk LSP to it.
///
/// Dropping the harness kills the child, so a test that bails early
/// (failed check, harness error) cannot leak a server process.
#[derive(Debug)]
struct Server {
    child: Child,
    stdin: ChildStdin,
    incoming: mpsc::Receiver<Result<Value, String>>,
}

impl Server {
    /// Spawn the real `mos-lsp` binary and start a thread that decodes
    /// framed messages from its stdout into a channel.
    fn spawn() -> Result<Self, BoxError> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mos-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or("child stdin was not piped")?;
        let stdout = child.stdout.take().ok_or("child stdout was not piped")?;
        let (tx, incoming) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_frame(&mut reader) {
                    Ok(Some(message)) => {
                        if tx.send(Ok(message)).is_err() {
                            return;
                        }
                    }
                    // Clean EOF: the server exited; the channel
                    // disconnect tells the test side.
                    Ok(None) => return,
                    Err(err) => {
                        let _ = tx.send(Err(err));
                        return;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            incoming,
        })
    }

    /// Send a JSON-RPC notification.
    fn notify(&mut self, method: &str, params: &Value) -> Result<(), BoxError> {
        write_frame(
            &mut self.stdin,
            &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        )
    }

    /// Send a JSON-RPC request and wait for the response carrying `id`.
    /// Notifications arriving in between (e.g. `publishDiagnostics`)
    /// are skipped.
    fn request(&mut self, id: u64, method: &str, params: &Value) -> Result<Value, BoxError> {
        write_frame(
            &mut self.stdin,
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )?;
        self.message_matching(&format!("response to `{method}` (id {id})"), |message| {
            message.get("id") == Some(&json!(id))
        })
    }

    /// `textDocument/didOpen` for a full document.
    fn open_document(&mut self, uri: &str, text: &str) -> Result<(), BoxError> {
        self.notify(
            "textDocument/didOpen",
            &json!({ "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": text,
            } }),
        )
    }

    /// Wait for the next `publishDiagnostics` notification for `uri`
    /// and return its diagnostics array.
    fn diagnostics_for(&mut self, uri: &str) -> Result<Vec<Value>, BoxError> {
        let message =
            self.message_matching(&format!("publishDiagnostics for {uri}"), |message| {
                message.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
                    && message.pointer("/params/uri").and_then(Value::as_str) == Some(uri)
            })?;
        message
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| format!("publish without a diagnostics array: {message}").into())
    }

    /// Read framed messages until one satisfies `matches`, bounded by
    /// [`RECV_TIMEOUT`] overall so a silent server fails deterministically.
    fn message_matching(
        &mut self,
        what: &str,
        matches: impl Fn(&Value) -> bool,
    ) -> Result<Value, BoxError> {
        let deadline = Instant::now() + RECV_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| format!("timed out waiting for {what}"))?;
            let framed = self
                .incoming
                .recv_timeout(remaining)
                .map_err(|err| format!("waiting for {what}: {err}"))?;
            let message =
                framed.map_err(|err| format!("reader failed while waiting for {what}: {err}"))?;
            if matches(&message) {
                return Ok(message);
            }
        }
    }

    /// Clean LSP teardown: `shutdown` request (must answer `null`),
    /// `exit` notification, then wait for the process to terminate
    /// successfully within [`RECV_TIMEOUT`].
    fn shutdown(mut self, id: u64) -> TestResult {
        let reply = self.request(id, "shutdown", &Value::Null)?;
        if reply.get("result") != Some(&Value::Null) {
            return Err(format!("shutdown reply was not a null result: {reply}").into());
        }
        self.notify("exit", &Value::Null)?;
        let deadline = Instant::now() + RECV_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("server exited with {status}").into())
                };
            }
            if Instant::now() >= deadline {
                return Err("server did not exit after the `exit` notification".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Failure path: don't leak a hung server. On the clean path the
        // process already exited and both calls are ignorable no-ops.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Write one `Content-Length`-framed JSON-RPC message.
fn write_frame(writer: &mut impl Write, message: &Value) -> Result<(), BoxError> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Read one framed message. `Ok(None)` on clean EOF before any header.
/// Errors are `String`s so the reader thread's channel stays `Send`.
fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length: Option<usize> = None;
    let mut saw_any_header = false;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| format!("read header: {err}"))?;
        if bytes == 0 {
            return if saw_any_header {
                Err("EOF inside a header block".to_owned())
            } else {
                Ok(None)
            };
        }
        saw_any_header = true;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|err| format!("bad Content-Length: {err}"))?,
            );
        }
    }
    let length = content_length.ok_or_else(|| "missing Content-Length header".to_owned())?;
    let mut body = vec![0u8; length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("read body: {err}"))?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| format!("parse body: {err}"))
}

/// `initialize` (id 1) + `initialized` handshake; returns the
/// `InitializeResult`.
fn initialize(server: &mut Server, params: &Value) -> Result<Value, BoxError> {
    let reply = server.request(1, "initialize", params)?;
    server.notify("initialized", &json!({}))?;
    match reply.get("result") {
        Some(result) => Ok(result.clone()),
        None => Err(format!("initialize reply without a result: {reply}").into()),
    }
}

/// Client-capability profile mirroring what Zed sends: UTF-16 position
/// encoding, hierarchical document symbols, quickfix code-action
/// literals, publish-diagnostics support.
fn zed_like_initialize_params() -> Value {
    json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "Zed", "version": "0.190.0" },
        "capabilities": {
            "general": { "positionEncodings": ["utf-16"] },
            "textDocument": {
                "synchronization": { "didSave": true },
                "publishDiagnostics": { "versionSupport": false },
                "definition": { "linkSupport": false },
                "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                "rename": { "prepareSupport": false },
                "codeAction": {
                    "codeActionLiteralSupport": {
                        "codeActionKind": { "valueSet": ["quickfix"] },
                    },
                },
            },
        },
        "workspaceFolders": null,
    })
}

/// Minimal client profile: a client that declares nothing.
fn default_initialize_params() -> Value {
    json!({ "capabilities": {} })
}

/// Zero-based LSP position (`line`, UTF-16 `character`) of the first
/// occurrence of `needle` in `src`, shifted `offset` bytes into it.
fn position_of(src: &str, needle: &str, offset: usize) -> Result<Value, BoxError> {
    let at = src
        .find(needle)
        .ok_or_else(|| format!("{needle:?} not found in fixture"))?
        + offset;
    let prefix = src
        .get(..at)
        .ok_or("cursor offset does not land on a char boundary")?;
    let line = prefix.matches('\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |newline| newline + 1);
    let character = prefix
        .get(line_start..)
        .map_or(0, |line_text| line_text.encode_utf16().count());
    Ok(json!({ "line": line, "character": character }))
}

/// LSP range covering the first occurrence of `needle` in `src`.
fn range_of(src: &str, needle: &str) -> Result<Value, BoxError> {
    Ok(json!({
        "start": position_of(src, needle, 0)?,
        "end": position_of(src, needle, needle.len())?,
    }))
}

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

/// Tiny scoped tempdir for real on-disk `.mos` fixtures. Zero-dep,
/// unique per test (timestamp + counter + PID), removed on drop.
#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let seq = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        path.push(format!("{label}-{nanos}-{seq}-{}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn initialize_handshake_advertises_capabilities_and_exits_cleanly() -> TestResult {
    let mut server = Server::spawn()?;
    let result = initialize(&mut server, &zed_like_initialize_params())?;

    let capabilities = result
        .get("capabilities")
        .ok_or("initialize result without capabilities")?;
    ensure_eq(
        &capabilities.get("positionEncoding"),
        &Some(&json!("utf-16")),
        "positionEncoding",
    )?;
    ensure_eq(
        &capabilities.get("textDocumentSync"),
        &Some(&json!(1)),
        "textDocumentSync",
    )?;
    ensure_eq(
        &capabilities.get("definitionProvider"),
        &Some(&json!(true)),
        "definitionProvider",
    )?;
    ensure_eq(
        &capabilities.get("documentSymbolProvider"),
        &Some(&json!(true)),
        "documentSymbolProvider",
    )?;
    ensure_eq(
        &capabilities.get("renameProvider"),
        &Some(&json!(true)),
        "renameProvider",
    )?;
    ensure_eq(
        &capabilities.get("codeActionProvider"),
        &Some(&json!(true)),
        "codeActionProvider",
    )?;
    // Pull diagnostics are not implemented; advertising them would
    // deadlock pull-capable clients.
    ensure(
        capabilities.get("diagnosticProvider").is_none(),
        &format!("must not advertise pull diagnostics, got {capabilities}"),
    )?;
    ensure_eq(
        &result.pointer("/serverInfo/name"),
        &Some(&json!("mos-lsp")),
        "serverInfo.name",
    )?;

    server.shutdown(2)
}

#[test]
fn did_open_change_close_drive_the_diagnostics_lifecycle() -> TestResult {
    let uri = "file:///virtual/main.mos";
    let mut server = Server::spawn()?;
    initialize(&mut server, &default_initialize_params())?;

    // Open a document with an undefined `@no:such` reference.
    server.open_document(uri, "see @no:such\n")?;
    let dirty = server.diagnostics_for(uri)?;
    let undefined_reference = dirty
        .iter()
        .find(|diagnostic| diagnostic.get("code") == Some(&json!("MOS0033")))
        .ok_or_else(|| format!("expected a MOS0033 diagnostic, got {dirty:?}"))?;
    ensure_eq(
        &undefined_reference.get("severity"),
        &Some(&json!(1)),
        "MOS0033 severity (1 = Error)",
    )?;
    ensure_eq(
        &undefined_reference.get("source"),
        &Some(&json!("mosaic")),
        "diagnostic source",
    )?;
    ensure_eq(
        &undefined_reference.pointer("/range/start/line"),
        &Some(&json!(0)),
        "diagnostic start line",
    )?;

    // Full-sync change to a clean document clears the squiggle.
    server.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "= clean\n\nbody\n" }],
        }),
    )?;
    let clean = server.diagnostics_for(uri)?;
    ensure(
        clean.is_empty(),
        &format!("clean doc should publish [], got {clean:?}"),
    )?;

    // Close publishes an empty set to clear editor state.
    server.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri } }),
    )?;
    let closed = server.diagnostics_for(uri)?;
    ensure(
        closed.is_empty(),
        &format!("didClose should clear diagnostics, got {closed:?}"),
    )?;

    server.shutdown(2)
}

#[test]
fn definition_resolves_label_reference_with_utf16_columns() -> TestResult {
    // The emoji (2 UTF-16 units, 4 UTF-8 bytes) in the declaration line
    // shifts UTF-8 and UTF-16 columns apart, so this round-trip fails
    // against a server that counts bytes instead of UTF-16 code units.
    let uri = "file:///virtual/main.mos";
    let src = "= \u{1F600} Intro <intro>\n\nSee @intro here.\n";
    let mut server = Server::spawn()?;
    initialize(&mut server, &zed_like_initialize_params())?;
    server.open_document(uri, src)?;

    let reply = server.request(
        2,
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri },
            // Cursor inside the `@intro` reference token.
            "position": position_of(src, "@intro", 2)?,
        }),
    )?;

    ensure_eq(
        &reply.pointer("/result/uri"),
        &Some(&json!(uri)),
        "definition target uri",
    )?;
    // The target is the declaration's label *token* (`intro` inside
    // `<intro>`): "= 😀 Intro <" is 12 UTF-16 units (14 UTF-8 bytes).
    ensure_eq(
        &reply.pointer("/result/range"),
        &Some(&json!({
            "start": { "line": 0, "character": 12 },
            "end": { "line": 0, "character": 17 },
        })),
        "declaration token range must count UTF-16 columns",
    )?;

    // A cursor off any reference resolves to null, not an error.
    let miss = server.request(
        3,
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 2, "character": 0 },
        }),
    )?;
    ensure_eq(
        &miss.get("result"),
        &Some(&Value::Null),
        "definition miss result",
    )?;
    ensure(
        miss.get("error").is_none(),
        &format!("unexpected error: {miss}"),
    )?;

    server.shutdown(4)
}

#[test]
fn definition_resolves_citation_into_real_bib_fixture() -> TestResult {
    // A real on-disk project: main.mos + refs.bib in a temp dir. The
    // citation resolves across files into the BibTeX key span.
    let dir = TempDir::new("mos-lsp-e2e-citation")?;
    let bib_path = dir.path().join("refs.bib");
    let main_path = dir.path().join("main.mos");
    std::fs::write(
        &bib_path,
        "@book{other, title={Other}}\n@article{patashnik1988, title={BibTeXing}}\n",
    )?;
    let src = "#bibliography(\"refs.bib\")\n\nCite [@patashnik1988].\n";
    std::fs::write(&main_path, src)?;
    let uri = mos_lsp::definition::path_to_uri(&main_path);
    let bib_uri = mos_lsp::definition::path_to_uri(&bib_path);

    let mut server = Server::spawn()?;
    initialize(&mut server, &zed_like_initialize_params())?;
    server.open_document(&uri, src)?;
    let diagnostics = server.diagnostics_for(&uri)?;
    ensure(
        diagnostics.is_empty(),
        &format!("known citation key should lower cleanly, got {diagnostics:?}"),
    )?;

    let reply = server.request(
        2,
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri },
            "position": position_of(src, "patashnik1988", 1)?,
        }),
    )?;
    ensure_eq(
        &reply.pointer("/result/uri"),
        &Some(&json!(bib_uri)),
        "citation definition should target the bib file",
    )?;
    // `patashnik1988` sits at columns 9..22 of the second bib line.
    ensure_eq(
        &reply.pointer("/result/range"),
        &Some(&json!({
            "start": { "line": 1, "character": 9 },
            "end": { "line": 1, "character": 22 },
        })),
        "citation should land on the bib key span",
    )?;

    server.shutdown(3)
}

#[test]
fn rename_returns_workspace_edit_for_declaration_and_references() -> TestResult {
    let uri = "file:///virtual/main.mos";
    let src = "= Intro <intro>\n\nSee @intro here.\n";
    let mut server = Server::spawn()?;
    initialize(&mut server, &zed_like_initialize_params())?;
    server.open_document(uri, src)?;

    let reply = server.request(
        2,
        "textDocument/rename",
        &json!({
            "textDocument": { "uri": uri },
            "position": position_of(src, "@intro", 2)?,
            "newName": "outro",
        }),
    )?;

    let edits = reply
        .pointer("/result/changes")
        .and_then(|changes| changes.get(uri))
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("rename reply without edits for {uri}: {reply}"))?;
    ensure_eq(&edits.len(), &2, "declaration + one reference edit")?;
    ensure(
        edits
            .iter()
            .all(|edit| edit.get("newText") == Some(&json!("outro"))),
        &format!("every edit rewrites to the new name: {edits:?}"),
    )?;
    ensure(
        edits
            .iter()
            .any(|edit| edit.pointer("/range/start/line") == Some(&json!(0))),
        &format!("one edit lands on the declaration line: {edits:?}"),
    )?;

    server.shutdown(3)
}

#[test]
fn code_action_projects_compiler_suggestion_as_quickfix() -> TestResult {
    let uri = "file:///virtual/main.mos";
    let src = "= Intro <intro>\n\nSee @intrp here.\n";
    let mut server = Server::spawn()?;
    initialize(&mut server, &zed_like_initialize_params())?;
    server.open_document(uri, src)?;

    let reply = server.request(
        2,
        "textDocument/codeAction",
        &json!({
            "textDocument": { "uri": uri },
            "range": range_of(src, "@intrp")?,
            "context": { "diagnostics": [], "only": ["quickfix"] },
        }),
    )?;

    let actions = reply
        .pointer("/result")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("code action reply without an array result: {reply}"))?;
    let new_texts: Vec<&str> = actions
        .iter()
        .filter_map(|action| {
            action
                .pointer("/edit/changes")
                .and_then(|changes| changes.get(uri))
                .and_then(Value::as_array)
                .and_then(|edits| edits.first())
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str)
        })
        .collect();
    ensure_eq(
        new_texts.as_slice(),
        ["@intro"].as_slice(),
        "the MOS0033 typo suggestion becomes a quickfix",
    )?;

    server.shutdown(3)
}

#[test]
fn document_symbol_returns_nested_heading_outline() -> TestResult {
    let uri = "file:///virtual/main.mos";
    let src = "= Intro\n\n== Setup\n\n=== Deep\n\n== Next\n\n= Appendix\n";
    let mut server = Server::spawn()?;
    initialize(&mut server, &zed_like_initialize_params())?;
    server.open_document(uri, src)?;

    let reply = server.request(
        2,
        "textDocument/documentSymbol",
        &json!({ "textDocument": { "uri": uri } }),
    )?;

    let symbols = reply
        .pointer("/result")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| format!("document symbol reply without an array result: {reply}"))?;
    let top_names: Vec<&str> = symbols
        .iter()
        .filter_map(|symbol| symbol.get("name").and_then(Value::as_str))
        .collect();
    ensure_eq(
        top_names.as_slice(),
        ["Intro", "Appendix"].as_slice(),
        "top-level headings",
    )?;
    let intro_children: Vec<&str> = symbols
        .first()
        .and_then(|symbol| symbol.get("children"))
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .filter_map(|child| child.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    ensure_eq(
        intro_children.as_slice(),
        ["Setup", "Next"].as_slice(),
        "hierarchical children under Intro",
    )?;

    server.shutdown(3)
}

#[test]
fn unknown_request_gets_method_not_found_instead_of_hanging() -> TestResult {
    let mut server = Server::spawn()?;
    initialize(&mut server, &default_initialize_params())?;

    let reply = server.request(2, "textDocument/hover", &json!({}))?;
    ensure_eq(
        &reply.pointer("/error/code"),
        &Some(&json!(-32601)),
        "unknown request must answer MethodNotFound",
    )?;
    ensure(
        reply.get("result").is_none(),
        &format!("unexpected result: {reply}"),
    )?;

    server.shutdown(3)
}
