//! Minimal stdio LSP server. The protocol surface is intentionally
//! narrow: just the messages needed to publish compiler diagnostics
//! for an opened document.
//!
//! Wire format follows the LSP base protocol — `Content-Length` framed
//! JSON-RPC 2.0 messages — implemented directly against [`std::io`]
//! rather than pulling in `tower-lsp` for one notification.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};

use serde_json::{Value, json};

use crate::cache::LoweringCache;
use crate::definition::definition_range_in;
use crate::diagnostics::{LspDiagnostic, LspPosition, diagnostics_from_result, path_from_uri};

/// Errors surfaced by the LSP server runtime. Compiler diagnostics
/// flow over the wire instead — they are never represented here.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    /// stdio read/write failure.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// Malformed JSON-RPC envelope (bad header, missing body bytes, …).
    #[error("LSP protocol error: {0}")]
    Protocol(String),
    /// JSON parse or serialisation failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Convenience alias for [`LspError`]-returning fns.
pub type Result<T> = std::result::Result<T, LspError>;

/// Run the language server against the process's stdio. Blocks until
/// the client sends `exit` (or stdin reaches EOF).
///
/// # Errors
///
/// Returns [`LspError`] if reading from stdin, parsing a message, or
/// writing a response fails.
pub fn run() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    serve(&mut reader, &mut writer)
}

/// Drive the server loop against an arbitrary reader/writer pair.
/// Exposed for tests and for callers that want to embed the server in
/// a non-stdio transport (e.g. a TCP harness).
///
/// # Errors
///
/// Propagates [`LspError`] for I/O and protocol failures.
pub fn serve<R: BufRead, W: Write>(reader: &mut R, writer: &mut W) -> Result<()> {
    let mut state = ServerState::default();
    loop {
        match read_message(reader)? {
            Some(message) => {
                if handle_message(&message, &mut state, writer)? {
                    return Ok(());
                }
            }
            None => return Ok(()),
        }
    }
}

#[derive(Default, Debug)]
struct ServerState {
    documents: HashMap<String, String>,
    /// Memoised `mos-eval` lowerings, reused across `textDocument/definition`
    /// requests and dropped whenever a document's source changes or closes.
    lowerings: LoweringCache,
}

/// Process a single decoded LSP message. Returns `Ok(true)` if the
/// loop should exit (the client sent `exit`).
fn handle_message<W: Write>(
    message: &Value,
    state: &mut ServerState,
    writer: &mut W,
) -> Result<bool> {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id");
    match (method, id) {
        (Some("initialize"), Some(id)) => {
            write_response(writer, id, &initialize_result())?;
            Ok(false)
        }
        (Some("initialized"), _) => Ok(false),
        (Some("shutdown"), Some(id)) => {
            write_response(writer, id, &Value::Null)?;
            Ok(false)
        }
        (Some("exit"), _) => Ok(true),
        (Some("textDocument/definition"), Some(id)) => {
            write_response(writer, id, &definition_result(state, message))?;
            Ok(false)
        }
        (Some("textDocument/didOpen"), _) => {
            if let Some(doc) = message.pointer("/params/textDocument")
                && let (Some(uri), Some(text)) = (
                    doc.get("uri").and_then(Value::as_str),
                    doc.get("text").and_then(Value::as_str),
                )
            {
                let uri = uri.to_owned();
                let text = text.to_owned();
                state.documents.insert(uri.clone(), text);
                // A re-open replaces the source; drop any prior lowering so
                // the publish below re-lowers the new text into the cache.
                state.lowerings.invalidate(&uri);
                publish_diagnostics(writer, state, &uri)?;
            }
            Ok(false)
        }
        (Some("textDocument/didChange"), _) => {
            let uri = message
                .pointer("/params/textDocument/uri")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let new_text = message
                .pointer("/params/contentChanges/0/text")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let (Some(uri), Some(text)) = (uri, new_text) {
                state.documents.insert(uri.clone(), text);
                // The edit invalidates the cached lowering; the publish below
                // re-lowers the new text once for diagnostics and definition.
                state.lowerings.invalidate(&uri);
                publish_diagnostics(writer, state, &uri)?;
            }
            Ok(false)
        }
        (Some("textDocument/didClose"), _) => {
            if let Some(uri) = message
                .pointer("/params/textDocument/uri")
                .and_then(Value::as_str)
            {
                let uri = uri.to_owned();
                state.documents.remove(&uri);
                state.lowerings.invalidate(&uri);
                // Clear stale squigglies in the editor.
                clear_diagnostics(writer, &uri)?;
            }
            Ok(false)
        }
        // Unknown request: respond with MethodNotFound so the client
        // doesn't hang on the missing reply. Unknown notifications
        // (no `id`) are silently dropped per LSP convention.
        (Some(_), Some(id)) => {
            write_error(writer, id, -32601, "method not found")?;
            Ok(false)
        }
        _ => Ok(false),
    }
}

fn initialize_result() -> Value {
    // Pull diagnostics (`textDocument/diagnostic`) are a separate
    // slice; advertising `diagnosticProvider` without handling the
    // request would make pull-capable clients hang waiting for a
    // reply. We only push via `publishDiagnostics` for now.
    json!({
        "capabilities": {
            // Full sync keeps the implementation small; incremental
            // sync lands when more than diagnostics is on the table.
            "textDocumentSync": 1,
            "positionEncoding": "utf-16",
            // Go-to-definition for `@label` references (issue #71).
            "definitionProvider": true,
        },
        "serverInfo": {
            "name": "mos-lsp",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

/// Build the `textDocument/definition` response for `message`: a single
/// LSP `Location` pointing at the referenced label's declaration, or
/// `null` when the request names no open document, carries no position,
/// the cursor is not on a reference, or the label is undeclared.
///
/// The target always lives in the requested document — the server holds
/// one source per URI and lowers it in isolation — so the response
/// reuses the request URI verbatim.
fn definition_result(state: &mut ServerState, message: &Value) -> Value {
    let Some(uri) = message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
    else {
        return Value::Null;
    };
    let Some(position) = read_position(message) else {
        return Value::Null;
    };
    // Disjoint field borrows: `documents` (read) and `lowerings` (lower on a
    // cache miss) are separate fields, so the source can be handed to the
    // cache while it populates without aliasing.
    let ServerState {
        documents,
        lowerings,
    } = state;
    let Some(src) = documents.get(uri) else {
        return Value::Null;
    };
    let path = path_from_uri(uri);
    let lowered = lowerings.get_or_lower(uri, src, &path);
    match definition_range_in(&lowered.document, &path, src, position) {
        Some(range) => json!({ "uri": uri, "range": range }),
        None => Value::Null,
    }
}

/// Extract the zero-based `position` (`line`, UTF-16 `character`) from a
/// request's params. Out-of-`u32`-range values clamp to `u32::MAX`,
/// which [`definition_range`] then resolves to end-of-document — a
/// harmless "no definition here" rather than a panic.
fn read_position(message: &Value) -> Option<LspPosition> {
    let line = message
        .pointer("/params/position/line")
        .and_then(Value::as_u64)?;
    let character = message
        .pointer("/params/position/character")
        .and_then(Value::as_u64)?;
    Some(LspPosition {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        character: u32::try_from(character).unwrap_or(u32::MAX),
    })
}

/// Publish diagnostics for `uri` from the **shared** per-document lowering
/// (issue #106). The source is lowered once into [`LoweringCache`] and that
/// same [`mos_eval::LowerResult`] is projected into LSP diagnostics; the
/// cached lowering then stays available for a later
/// `textDocument/definition` request on the unchanged document, so an edit
/// lowers its source only once for both diagnostics and go-to-definition.
///
/// Callers invalidate the cache *before* publishing on a source mutation,
/// so the lowering populated here always reflects the current text.
fn publish_diagnostics<W: Write>(writer: &mut W, state: &mut ServerState, uri: &str) -> Result<()> {
    let path = path_from_uri(uri);
    // Disjoint field borrows: read the source from `documents`, lower into
    // `lowerings`. The projected `Vec` is owned, so the cache borrow ends
    // before `send_publish` writes.
    let ServerState {
        documents,
        lowerings,
    } = state;
    let Some(src) = documents.get(uri) else {
        return Ok(());
    };
    let lowered = lowerings.get_or_lower(uri, src, &path);
    let diagnostics = diagnostics_from_result(&path, src, lowered);
    send_publish(writer, uri, &diagnostics)
}

fn clear_diagnostics<W: Write>(writer: &mut W, uri: &str) -> Result<()> {
    send_publish(writer, uri, &[])
}

fn send_publish<W: Write>(writer: &mut W, uri: &str, diagnostics: &[LspDiagnostic]) -> Result<()> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics,
        },
    });
    write_message(writer, &notification)
}

fn write_response<W: Write>(writer: &mut W, id: &Value, result: &Value) -> Result<()> {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    write_message(writer, &response)
}

fn write_error<W: Write>(writer: &mut W, id: &Value, code: i32, message: &str) -> Result<()> {
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    write_message(writer, &response)
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

/// Read one JSON-RPC message. Returns `Ok(None)` when the peer closes
/// the stream cleanly (EOF before any header bytes), which the loop
/// treats as a graceful shutdown.
fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut saw_any_header = false;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return if saw_any_header {
                Err(LspError::Protocol(
                    "unexpected EOF in header block".to_owned(),
                ))
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
                    .map_err(|err| LspError::Protocol(format!("bad Content-Length: {err}")))?,
            );
        }
        // Other headers (e.g. Content-Type) are ignored.
    }
    let length = content_length
        .ok_or_else(|| LspError::Protocol("missing Content-Length header".to_owned()))?;
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer)?;
    let value = serde_json::from_slice(&buffer)?;
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::io::Cursor;

    use super::*;
    use crate::diagnostics::byte_to_position;

    fn frame(value: &Value) -> Vec<u8> {
        frame_with_header(value, "Content-Length")
    }

    fn frame_with_header(value: &Value, header_name: &str) -> Vec<u8> {
        let body = serde_json::to_vec(value).expect("encode");
        let mut buf = format!("{header_name}: {}\r\n\r\n", body.len()).into_bytes();
        buf.extend_from_slice(&body);
        buf
    }

    fn decode_messages(bytes: &[u8]) -> Vec<Value> {
        let mut cursor = BufReader::new(Cursor::new(bytes.to_vec()));
        let mut out = Vec::new();
        while let Some(msg) = read_message(&mut cursor).expect("decode") {
            out.push(msg);
        }
        out
    }

    #[test]
    fn empty_input_returns_ok() {
        let mut reader = BufReader::new(Cursor::new(Vec::<u8>::new()));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("clean EOF should be Ok");
        assert!(writer.is_empty());
    }

    #[test]
    fn content_length_header_is_case_insensitive() {
        let input = frame_with_header(
            &json!({ "jsonrpc": "2.0", "method": "exit" }),
            "content-length",
        );
        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("lowercase Content-Length should parse");
        assert!(writer.is_empty());
    }

    #[test]
    fn initialize_did_open_publishes_diagnostics_and_exits() {
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {},
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///virtual/main.mos",
                    "languageId": "mosaic",
                    "version": 1,
                    "text": "see @no:such\n",
                },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "exit",
        })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        assert_eq!(
            messages.len(),
            2,
            "expected initialize response + publish, got {messages:?}"
        );

        let init = &messages[0];
        assert_eq!(init.get("id"), Some(&json!(1)));
        assert!(init.pointer("/result/capabilities").is_some());

        let publish = &messages[1];
        assert_eq!(
            publish.get("method").and_then(Value::as_str),
            Some("textDocument/publishDiagnostics")
        );
        let diagnostics = publish
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .expect("diagnostics array");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.get("code").and_then(Value::as_str) == Some("MOS0033")),
            "expected a MOS0033 diagnostic, got {diagnostics:?}"
        );
    }

    #[test]
    fn did_change_republishes_diagnostics() {
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///virtual/main.mos",
                    "languageId": "mosaic",
                    "version": 1,
                    "text": "= clean\n",
                },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": "file:///virtual/main.mos", "version": 2 },
                "contentChanges": [{ "text": "see @no:such\n" }],
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        assert_eq!(messages.len(), 2);
        // First publish: clean document → no diagnostics.
        let clean = messages[0]
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .expect("clean diagnostics");
        assert!(
            clean.is_empty(),
            "clean document should publish empty list, got {clean:?}"
        );
        // Second publish: the changed document re-triggers MOS0033.
        let dirty = messages[1]
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .expect("dirty diagnostics");
        assert!(
            dirty
                .iter()
                .any(|d| d.get("code").and_then(Value::as_str) == Some("MOS0033")),
            "expected MOS0033 after didChange, got {dirty:?}"
        );
    }

    #[test]
    fn initialize_capabilities_omit_pull_diagnostics() {
        // We only push diagnostics over `publishDiagnostics`. The
        // `diagnosticProvider` capability advertises pull support
        // (`textDocument/diagnostic`), which we don't implement —
        // declaring it would deadlock pull-capable clients.
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let capabilities = messages
            .first()
            .and_then(|m| m.pointer("/result/capabilities"))
            .expect("initialize response with capabilities");
        assert!(
            capabilities.get("diagnosticProvider").is_none(),
            "must not advertise pull diagnostics, got {capabilities:?}"
        );
        assert_eq!(capabilities.get("textDocumentSync"), Some(&json!(1)));
    }

    #[test]
    fn unknown_request_returns_method_not_found() {
        let mut input: Vec<u8> = Vec::new();
        // `textDocument/hover` is not implemented — an unhandled request
        // must still get a MethodNotFound reply so the client doesn't hang.
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/hover",
            "params": {},
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get("id"), Some(&json!(7)));
        assert_eq!(messages[0].pointer("/error/code"), Some(&json!(-32601)));
    }

    #[test]
    fn initialize_advertises_definition_provider() {
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let capabilities = messages
            .first()
            .and_then(|m| m.pointer("/result/capabilities"))
            .expect("initialize response with capabilities");
        assert_eq!(capabilities.get("definitionProvider"), Some(&json!(true)));
    }

    #[test]
    fn definition_request_returns_label_declaration_location() {
        // An `@intro` reference on line 2 resolves to the `= Intro <intro>`
        // heading on line 0. The cursor sits inside the reference token.
        let uri = "file:///virtual/main.mos";
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        let cursor = src.find("@intro").map_or(0, |at| at + 2);
        let position = byte_to_position(src, cursor);

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "mosaic",
                    "version": 1,
                    "text": src,
                },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        // didOpen publishes diagnostics; the definition reply carries id 9.
        let reply = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(9)))
            .expect("definition response");
        assert_eq!(reply.pointer("/result/uri"), Some(&json!(uri)));
        assert_eq!(reply.pointer("/result/range/start/line"), Some(&json!(0)));
    }

    #[test]
    fn definition_reflects_source_after_did_change() {
        // The lowering cache must not outlive an edit. We resolve `@intro`
        // once (priming the cache), then `didChange` pushes the `<intro>`
        // declaration down a line. A second definition request must land on
        // the *new* declaration line — a stale cache would still report the
        // original line 0.
        let uri = "file:///virtual/main.mos";
        let before = "= Intro <intro>\n\nSee @intro here.\n";
        let after = "\n= Intro <intro>\n\nSee @intro here.\n";
        let before_cursor = before.find("@intro").map_or(0, |at| at + 2);
        let after_cursor = after.find("@intro").map_or(0, |at| at + 2);
        let before_pos = byte_to_position(before, before_cursor);
        let after_pos = byte_to_position(after, after_cursor);

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "mosaic",
                    "version": 1,
                    "text": before,
                },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": before_pos.line, "character": before_pos.character },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": after }],
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": after_pos.line, "character": after_pos.character },
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let first = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(20)))
            .expect("first definition response");
        assert_eq!(
            first.pointer("/result/range/start/line"),
            Some(&json!(0)),
            "before the edit, the declaration is on line 0"
        );
        let second = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(21)))
            .expect("second definition response");
        assert_eq!(
            second.pointer("/result/range/start/line"),
            Some(&json!(1)),
            "after the edit, the declaration moved to line 1; a stale cache \
             would still report line 0"
        );
    }

    #[test]
    fn definition_on_unknown_label_returns_null() {
        let uri = "file:///virtual/main.mos";
        let src = "See @nope here.\n";
        let cursor = src.find("@nope").map_or(0, |at| at + 1);
        let position = byte_to_position(src, cursor);

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "mosaic",
                    "version": 1,
                    "text": src,
                },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let reply = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(10)))
            .expect("definition response");
        // Unknown label → JSON-RPC result is null (not an error).
        assert_eq!(reply.get("result"), Some(&Value::Null));
        assert!(reply.get("error").is_none());
    }

    #[test]
    fn publishing_diagnostics_populates_the_shared_lowering_cache() {
        // #106: publishing diagnostics lowers the document once and leaves the
        // result cached, so a later `textDocument/definition` request on the
        // unchanged source reuses it instead of lowering the same text again.
        let uri = "file:///virtual/main.mos";
        let mut state = ServerState::default();
        state.documents.insert(
            uri.to_owned(),
            "= Intro <intro>\n\nSee @intro here.\n".to_owned(),
        );
        assert!(
            !state.lowerings.is_cached(uri),
            "nothing is cached before the first publish"
        );

        let mut writer: Vec<u8> = Vec::new();
        publish_diagnostics(&mut writer, &mut state, uri).expect("publish");

        assert!(
            state.lowerings.is_cached(uri),
            "publishing diagnostics must leave the lowering cached for a later \
             definition request to reuse"
        );
    }

    #[test]
    fn shared_lowering_stays_consistent_across_an_edit() {
        // Sharing one lowering between diagnostics and definition must not
        // leak stale state between them. Open a clean doc and resolve a
        // reference, then change to a doc whose reference is undefined: the
        // post-edit diagnostics must report MOS0033 and the definition request
        // must return null — both reflecting the new text from one re-lowering.
        let uri = "file:///virtual/main.mos";
        let before = "= Intro <intro>\n\nSee @intro here.\n";
        let after = "See @gone here.\n";
        let before_pos = byte_to_position(before, before.find("@intro").map_or(0, |at| at + 2));
        let after_pos = byte_to_position(after, after.find("@gone").map_or(0, |at| at + 2));

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": before,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 30,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": before_pos.line, "character": before_pos.character },
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": { "uri": uri, "version": 2 },
                "contentChanges": [{ "text": after }],
            },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": after_pos.line, "character": after_pos.character },
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        // Before the edit: the reference resolves to the line-0 declaration.
        let first = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(30)))
            .expect("first definition response");
        assert_eq!(first.pointer("/result/range/start/line"), Some(&json!(0)));
        // After the edit: diagnostics report the now-undefined reference …
        let publishes: Vec<&Value> = messages
            .iter()
            .filter(|m| {
                m.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics")
            })
            .collect();
        let last_publish = publishes.last().expect("a post-change publish");
        let diagnostics = last_publish
            .pointer("/params/diagnostics")
            .and_then(Value::as_array)
            .expect("diagnostics array");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.get("code").and_then(Value::as_str) == Some("MOS0033")),
            "the edited document's undefined reference must surface MOS0033, got {diagnostics:?}"
        );
        // … and definition for the now-undefined reference is null.
        let second = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(31)))
            .expect("second definition response");
        assert_eq!(second.get("result"), Some(&Value::Null));
    }
}
