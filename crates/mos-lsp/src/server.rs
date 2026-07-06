//! Minimal stdio LSP server. The protocol surface is intentionally
//! narrow: just the messages needed to publish compiler diagnostics
//! for an opened document.
//!
//! Wire format follows the LSP base protocol: `Content-Length` framed
//! JSON-RPC 2.0 messages: implemented directly against [`std::io`]
//! rather than pulling in `tower-lsp` for one notification.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use mos_eval::LowerResult;
use serde_json::{Value, json};

use crate::cache::Store as LoweringCache;
use crate::code_action::code_actions_for_range;
use crate::definition::{path_to_uri, target_in};
use crate::diagnostics::{LspDiagnostic, LspPosition, LspRange, from_result, path_from_uri};
use crate::document_symbol::document_symbols;
use crate::rename::ranges as rename_ranges;

/// Errors surfaced by the LSP server runtime. Compiler diagnostics
/// flow over the wire instead: they are never represented here.
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
        (Some("textDocument/rename"), Some(id)) => {
            write_response(writer, id, &rename_result(state, message))?;
            Ok(false)
        }
        (Some("textDocument/codeAction"), Some(id)) => {
            write_response(writer, id, &code_action_result(state, message))?;
            Ok(false)
        }
        (Some("textDocument/documentSymbol"), Some(id)) => {
            write_response(writer, id, &document_symbol_result(state, message))?;
            Ok(false)
        }
        (Some("textDocument/hover"), Some(id)) => {
            write_response(writer, id, &hover_result(state, message))?;
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
            // Nested headings for editor outlines and breadcrumbs.
            "documentSymbolProvider": true,
            // Rename a label across its declaration and references.
            "renameProvider": true,
            // Quick fixes are projected from compiler `Suggestion`s.
            "codeActionProvider": true,
            // Hover shows a symbol's attached `/** … */` doc comment.
            "hoverProvider": true,
        },
        "serverInfo": {
            "name": "mos-lsp",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn document_symbol_result(state: &mut ServerState, message: &Value) -> Value {
    let Some(uri) = message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
    else {
        return Value::Array(Vec::new());
    };
    let symbols = with_lowering(state, uri, |lowered, _path, src| {
        document_symbols(&lowered.document, src)
    });
    Value::Array(symbols.unwrap_or_default())
}

/// Build the `textDocument/definition` response for `message`: a single
/// LSP `Location` pointing at the referenced label declaration or cited
/// BibTeX key, or `null` when the request names no open document, carries no
/// position, or the cursor is not on a resolvable reference/citation.
///
/// Label targets reuse the request document URI. Citation targets can
/// resolve to an external `.bib` file already read during lowering, so
/// the response URI may differ from the request URI.
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
    let target = with_lowering(state, uri, |lowered, path, src| {
        target_in(&lowered.document, path, src, position).map(|target| {
            let target_uri = if target.path == *path {
                uri.to_owned()
            } else {
                path_to_uri(&target.path)
            };
            json!({ "uri": target_uri, "range": target.range })
        })
    });
    target.flatten().unwrap_or(Value::Null)
}

/// Build the `textDocument/hover` response: the `/** … */` doc comment attached
/// to the symbol under the cursor (a heading, a `<label>` block, or an
/// `@label` / `[@key]` reference to one), rendered as Markdown. `null` when the
/// cursor is not on a documented symbol.
fn hover_result(state: &mut ServerState, message: &Value) -> Value {
    let Some(uri) = message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
    else {
        return Value::Null;
    };
    let Some(position) = read_position(message) else {
        return Value::Null;
    };
    let hover = with_lowering(state, uri, |lowered, path, src| {
        crate::hover::doc_at(&lowered.document, path, src, position)
            .map(|doc| json!({ "contents": { "kind": "markdown", "value": doc } }))
    });
    hover.flatten().unwrap_or(Value::Null)
}

/// Run `f` against the lowering for `uri`: a cached one when present, else a
/// fresh `mos_eval::lower`. Returns `None` only when `uri` names no open
/// document; otherwise `Some(f(...))`.
///
/// A freshly-lowered **pure** result is stored in the cache for reuse across
/// the next diagnostics/definition request on the unchanged source (issue
/// #106). An **impure** lowering: one that read external files (`#image` /
/// `#figure` / `#bibliography`, see `reads_external_resources`) is used
/// once and dropped, never cached, so such a document is re-lowered on every
/// request and always reflects the current filesystem (issue #106 review).
fn with_lowering<T>(
    state: &mut ServerState,
    uri: &str,
    f: impl FnOnce(&LowerResult, &Path, &str) -> T,
) -> Option<T> {
    // Disjoint field borrows: read the source from `documents`, look up /
    // populate `lowerings`: separate fields, so neither aliases the other.
    let ServerState {
        documents,
        lowerings,
    } = state;
    let src = documents.get(uri)?;
    let path = path_from_uri(uri);
    if let Some(cached) = lowerings.get(uri) {
        return Some(f(cached, &path, src));
    }
    let fresh = mos_eval::lower(src, &path);
    let result = f(&fresh, &path, src);
    if !fresh.reads_external_resources {
        lowerings.store(uri, fresh);
    }
    Some(result)
}

/// Build the `textDocument/codeAction` response: one `quickfix` per
/// compiler-provided [`mos_core::Suggestion`] whose diagnostic/suggestion span
/// intersects the requested range.
fn code_action_result(state: &mut ServerState, message: &Value) -> Value {
    if !code_action_context_allows_quickfix(message) {
        return Value::Array(Vec::new());
    }
    let Some(uri) = message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
    else {
        return Value::Array(Vec::new());
    };
    let Some(range) = read_range(message) else {
        return Value::Array(Vec::new());
    };
    let actions = with_lowering(state, uri, |lowered, path, src| {
        code_actions_for_range(path, src, uri, lowered, range)
    });
    Value::Array(actions.unwrap_or_default())
}

fn code_action_context_allows_quickfix(message: &Value) -> bool {
    let Some(only) = message.pointer("/params/context/only") else {
        return true;
    };
    let Some(kinds) = only.as_array() else {
        return true;
    };
    kinds.iter().any(|kind| kind.as_str() == Some("quickfix"))
}

/// Build the `textDocument/rename` response: a `WorkspaceEdit` rewriting the
/// label under the cursor: its first declaration's token and every reference
///: to the request's `newName`. Returns `null` when the cursor is not on a
/// label, the request omits a position or new name, or names no open document.
///
/// Single-document: every edit lands in the request URI, so the
/// `WorkspaceEdit` carries one `changes` entry keyed by that URI.
fn rename_result(state: &mut ServerState, message: &Value) -> Value {
    let Some(uri) = message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
    else {
        return Value::Null;
    };
    let Some(position) = read_position(message) else {
        return Value::Null;
    };
    let Some(new_name) = message.pointer("/params/newName").and_then(Value::as_str) else {
        return Value::Null;
    };
    let ranges = with_lowering(state, uri, |lowered, path, src| {
        rename_ranges(&lowered.document, path, src, position)
    });
    // Outer `None`: unknown URI. Inner `None`: cursor not on a renameable label.
    let Some(Some(ranges)) = ranges else {
        return Value::Null;
    };
    let edits: Vec<Value> = ranges
        .iter()
        .map(|range| json!({ "range": range, "newText": new_name }))
        .collect();
    let mut changes = serde_json::Map::new();
    changes.insert(uri.to_owned(), Value::Array(edits));
    json!({ "changes": Value::Object(changes) })
}

/// Extract the zero-based `position` (`line`, UTF-16 `character`) from a
/// request's params. Out-of-`u32`-range values clamp to `u32::MAX`,
/// which [`definition_range`](crate::definition_range) then resolves to end-of-document: a
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

fn read_range(message: &Value) -> Option<LspRange> {
    Some(LspRange {
        start: LspPosition {
            line: u32::try_from(
                message
                    .pointer("/params/range/start/line")
                    .and_then(Value::as_u64)?,
            )
            .unwrap_or(u32::MAX),
            character: u32::try_from(
                message
                    .pointer("/params/range/start/character")
                    .and_then(Value::as_u64)?,
            )
            .unwrap_or(u32::MAX),
        },
        end: LspPosition {
            line: u32::try_from(
                message
                    .pointer("/params/range/end/line")
                    .and_then(Value::as_u64)?,
            )
            .unwrap_or(u32::MAX),
            character: u32::try_from(
                message
                    .pointer("/params/range/end/character")
                    .and_then(Value::as_u64)?,
            )
            .unwrap_or(u32::MAX),
        },
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
    // Project diagnostics from the shared lowering (cached pure result, or a
    // fresh lower that gets stored when pure). The owned `Vec` outlives the
    // lowering borrow, so the write below is unconstrained. Unknown URIs
    // yield `None` and publish nothing.
    let diagnostics = with_lowering(state, uri, |lowered, path, src| {
        from_result(path, src, lowered)
    });
    diagnostics.map_or(Ok(()), |diagnostics| {
        send_publish(writer, uri, &diagnostics)
    })
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

    fn range_json(src: &str, start: usize, end: usize) -> Value {
        let start = byte_to_position(src, start);
        let end = byte_to_position(src, end);
        json!({
            "start": { "line": start.line, "character": start.character },
            "end": { "line": end.line, "character": end.character },
        })
    }

    fn code_actions_for_source_range(src: &str, start: usize, end: usize) -> Vec<Value> {
        code_actions_for_source_range_with_context(src, start, end, &json!({ "diagnostics": [] }))
    }

    fn code_actions_for_source_range_with_context(
        src: &str,
        start: usize,
        end: usize,
        context: &Value,
    ) -> Vec<Value> {
        let uri = "file:///virtual/main.mos";
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": src,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 90,
            "method": "textDocument/codeAction",
            "params": {
                "textDocument": { "uri": uri },
                "range": range_json(src, start, end),
                "context": context,
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        decode_messages(&writer)
            .into_iter()
            .find(|m| m.get("id") == Some(&json!(90)))
            .and_then(|reply| reply.get("result").and_then(Value::as_array).cloned())
            .expect("code action response")
    }

    fn code_action_new_texts(actions: &[Value]) -> Vec<&str> {
        let uri = "file:///virtual/main.mos";
        actions
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
            .collect()
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
        // (`textDocument/diagnostic`), which we don't implement.
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
        // `textDocument/foldingRange` is not implemented: an unhandled request
        // must still get a MethodNotFound reply so the client doesn't hang.
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/foldingRange",
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
        assert_eq!(
            capabilities.get("documentSymbolProvider"),
            Some(&json!(true))
        );
        assert_eq!(capabilities.get("renameProvider"), Some(&json!(true)));
        assert_eq!(capabilities.get("codeActionProvider"), Some(&json!(true)));
        assert_eq!(capabilities.get("hoverProvider"), Some(&json!(true)));
    }

    #[test]
    fn document_symbol_request_returns_nested_headings() {
        let uri = "file:///virtual/main.mos";
        let src = "= Intro\n\n== Setup\n\n=== Deep\n\n== Next\n\n= Appendix\n";
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": src,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": uri } },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let result = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(18)))
            .and_then(|reply| reply.get("result"))
            .and_then(Value::as_array)
            .expect("document symbol response");
        assert_eq!(result.len(), 2, "top-level Intro + Appendix: {result:?}");
        assert_eq!(result[0].get("name"), Some(&json!("Intro")));
        assert_eq!(result[1].get("name"), Some(&json!("Appendix")));
        let intro_children = result[0]
            .get("children")
            .and_then(Value::as_array)
            .expect("Intro children");
        assert_eq!(intro_children.len(), 2, "Setup + Next: {intro_children:?}");
        assert_eq!(intro_children[0].get("name"), Some(&json!("Setup")));
        assert_eq!(intro_children[1].get("name"), Some(&json!("Next")));
        let setup_children = intro_children[0]
            .get("children")
            .and_then(Value::as_array)
            .expect("Setup children");
        assert_eq!(setup_children.len(), 1, "Deep child: {setup_children:?}");
        assert_eq!(setup_children[0].get("name"), Some(&json!("Deep")));
        assert_eq!(result[0].get("kind"), Some(&json!(3)));
    }

    #[test]
    fn code_action_request_returns_unknown_reference_fix() {
        let src = "= Intro <intro>\n\nSee @intrp here.\n";
        let start = src.find("@intrp").expect("reference");
        let actions = code_actions_for_source_range(src, start, start + "@intrp".len());

        assert_eq!(code_action_new_texts(&actions), vec!["@intro"]);
    }

    #[test]
    fn code_action_request_returns_inline_closer_at_diagnostic() {
        let src = "*unclosed\n";
        let start = src.find('*').expect("emphasis opener");
        let actions = code_actions_for_source_range(src, start, start + 1);

        assert_eq!(code_action_new_texts(&actions), vec!["*"]);
    }

    #[test]
    fn code_action_request_omits_ambiguous_inline_closer() {
        let src = "hi `a *b*\n";
        let start = src.find('`').expect("code opener");
        let actions = code_actions_for_source_range(src, start, start + 1);

        assert!(actions.is_empty(), "ambiguous closer actions: {actions:?}");
    }

    #[test]
    fn code_action_request_omits_multiline_ambiguous_inline_closer() {
        let src = "hi `a\n*b*\n";
        let start = src.find('`').expect("code opener");
        let actions = code_actions_for_source_range(src, start, start + 1);

        assert!(actions.is_empty(), "ambiguous closer actions: {actions:?}");
    }

    #[test]
    fn code_action_request_honors_context_only() {
        let src = "= Intro <intro>\n\nSee @intrp here.\n";
        let start = src.find("@intrp").expect("reference");
        let source_actions = code_actions_for_source_range_with_context(
            src,
            start,
            start + "@intrp".len(),
            &json!({ "diagnostics": [], "only": ["source"] }),
        );
        let quickfix_actions = code_actions_for_source_range_with_context(
            src,
            start,
            start + "@intrp".len(),
            &json!({ "diagnostics": [], "only": ["quickfix"] }),
        );

        assert!(
            source_actions.is_empty(),
            "source-only request got {source_actions:?}"
        );
        assert_eq!(code_action_new_texts(&quickfix_actions), vec!["@intro"]);
    }

    #[test]
    fn code_action_request_returns_duplicate_label_fix() {
        let src = "= One <dup>\n= Two <dup>\n";
        let start = src.rfind("dup").expect("duplicate label token");
        let actions = code_actions_for_source_range(src, start, start + "dup".len());

        assert_eq!(code_action_new_texts(&actions), vec!["dup-2"]);
    }

    #[test]
    fn code_action_request_returns_each_heading_label_fix() {
        let src = "= Title <intro> [@k]\n";
        let start = src.find("<intro>").expect("misplaced label");
        let actions = code_actions_for_source_range(src, start, start + "<intro>".len());

        assert_eq!(
            code_action_new_texts(&actions),
            vec!["Title [@k] <intro>", "\\<"]
        );
    }

    #[test]
    fn rename_request_returns_workspace_edit_for_all_occurrences() {
        // `@intro` (and the declaration) rename to `outro`. The response is a
        // WorkspaceEdit whose single `changes` entry lists one edit per
        // occurrence: declaration token + reference; each replacing the
        // identifier text with the new name.
        let uri = "file:///virtual/main.mos";
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        let cursor = src.find("@intro").map_or(0, |at| at + 2);
        let position = byte_to_position(src, cursor);

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": src,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 40,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
                "newName": "outro",
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let reply = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(40)))
            .expect("rename response");
        let edits = reply
            .pointer("/result/changes")
            .and_then(|changes| changes.get(uri))
            .and_then(Value::as_array)
            .expect("changes for the request URI");
        assert_eq!(
            edits.len(),
            2,
            "declaration token + one reference: {edits:?}"
        );
        assert!(
            edits
                .iter()
                .all(|e| e.get("newText").and_then(Value::as_str) == Some("outro")),
            "every edit rewrites to the new name"
        );
        // The declaration token edit targets line 0, columns 9..14 (`intro`).
        assert!(
            edits
                .iter()
                .any(|e| e.pointer("/range/start/line") == Some(&json!(0))),
            "an edit lands on the declaration line"
        );
    }

    #[test]
    fn rename_off_any_label_returns_null() {
        let uri = "file:///virtual/main.mos";
        let src = "= Intro <intro>\n\nplain text\n";
        // Line 2 column 0 is plain paragraph text, not a label.
        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": src,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 41,
            "method": "textDocument/rename",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": 2, "character": 0 },
                "newName": "x",
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");

        let messages = decode_messages(&writer);
        let reply = messages
            .iter()
            .find(|m| m.get("id") == Some(&json!(41)))
            .expect("rename response");
        assert_eq!(reply.get("result"), Some(&Value::Null));
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

    /// Drive a `didOpen` + `textDocument/hover` round-trip and return the hover
    /// reply (id 11).
    fn hover_reply(uri: &str, src: &str, position: LspPosition) -> Value {
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
            "id": 11,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            },
        })));
        input.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

        let mut reader = BufReader::new(Cursor::new(input));
        let mut writer: Vec<u8> = Vec::new();
        serve(&mut reader, &mut writer).expect("server loop");
        decode_messages(&writer)
            .into_iter()
            .find(|m| m.get("id") == Some(&json!(11)))
            .expect("hover response")
    }

    #[test]
    fn hover_on_heading_returns_its_doc_comment() {
        let uri = "file:///virtual/main.mos";
        let src = "/** Intro doc. */\n= Intro <intro>\n\nSee @intro here.\n";
        // Cursor inside the heading title "Intro" on line 1.
        let cursor = src.find("Intro <").map_or(0, |at| at + 1);
        let reply = hover_reply(uri, src, byte_to_position(src, cursor));
        assert_eq!(
            reply.pointer("/result/contents/value"),
            Some(&json!("Intro doc.")),
        );
        assert_eq!(
            reply.pointer("/result/contents/kind"),
            Some(&json!("markdown")),
        );
    }

    #[test]
    fn hover_on_reference_returns_target_doc_comment() {
        // Documentation follows the symbol: hovering `@intro` shows the doc
        // comment attached to the `<intro>` heading it points at.
        let uri = "file:///virtual/main.mos";
        let src = "/** Intro doc. */\n= Intro <intro>\n\nSee @intro here.\n";
        let cursor = src.find("@intro").map_or(0, |at| at + 2);
        let reply = hover_reply(uri, src, byte_to_position(src, cursor));
        assert_eq!(
            reply.pointer("/result/contents/value"),
            Some(&json!("Intro doc.")),
        );
    }

    #[test]
    fn hover_off_a_documented_symbol_returns_null() {
        let uri = "file:///virtual/main.mos";
        let src = "/** Intro doc. */\n= Intro <intro>\n\nplain text here.\n";
        // Cursor in the undocumented paragraph.
        let cursor = src.find("plain").map_or(0, |at| at + 1);
        let reply = hover_reply(uri, src, byte_to_position(src, cursor));
        assert_eq!(reply.pointer("/result"), Some(&Value::Null));
    }

    #[test]
    fn definition_request_returns_citation_bib_entry_location() {
        let dir = std::env::temp_dir().join(format!(
            "mos-lsp-citation-def-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let bib = dir.join("refs.bib");
        let main = dir.join("main.mos");
        std::fs::write(
            &bib,
            "@book{other, title={Other}}\n@article{patashnik1988, title={BibTeXing}}\n",
        )
        .expect("write bib");
        let src = "#bibliography(\"refs.bib\")\n\nCite [@patashnik1988].\n";
        std::fs::write(&main, src).expect("write source");
        let uri = path_to_uri(&main);
        let bib_uri = path_to_uri(&bib);
        let cursor = src.find("patashnik1988").map_or(0, |at| at + 1);
        let position = byte_to_position(src, cursor);

        let mut input: Vec<u8> = Vec::new();
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "mosaic", "version": 1, "text": src,
            } },
        })));
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 19,
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
            .find(|m| m.get("id") == Some(&json!(19)))
            .expect("definition response");
        assert_eq!(reply.pointer("/result/uri"), Some(&json!(bib_uri)));
        assert_eq!(reply.pointer("/result/range/start/line"), Some(&json!(1)));
        assert_eq!(
            reply.pointer("/result/range/start/character"),
            Some(&json!(9))
        );
        assert_eq!(
            reply.pointer("/result/range/end/character"),
            Some(&json!(22))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn definition_reflects_source_after_did_change() {
        // The lowering cache must not outlive an edit. We resolve `@intro`
        // once (priming the cache), then `didChange` pushes the `<intro>`
        // declaration down a line. A second definition request must land on
        // the *new* declaration line: a stale cache would still report the
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
    fn impure_document_is_not_cached_so_requests_relower() {
        // #106 review: lowering `#figure(image: …)` reads an external file, so
        // the result is impure and must NOT be cached. Otherwise publishing
        // diagnostics at `didOpen` (image missing) would seed the cache with a
        // figure-less lowering, and a later definition request would reuse it
        // even after the image appears. An empty cache after publish proves the
        // document is re-lowered fresh on each request, reflecting the
        // filesystem.
        let uri = "file:///virtual/main.mos";
        let mut state = ServerState::default();
        state.documents.insert(
            uri.to_owned(),
            "#figure(image: \"x.png\", label: \"fig\", caption: \"c\")\n\nSee @fig\n".to_owned(),
        );

        let mut writer: Vec<u8> = Vec::new();
        publish_diagnostics(&mut writer, &mut state, uri).expect("publish");

        assert!(
            !state.lowerings.is_cached(uri),
            "a lowering that read external files must not be cached"
        );
    }

    #[test]
    fn shared_lowering_stays_consistent_across_an_edit() {
        // Sharing one lowering between diagnostics and definition must not
        // leak stale state between them. Open a clean doc and resolve a
        // reference, then change to a doc whose reference is undefined: the
        // post-edit diagnostics must report MOS0033 and the definition request
        // must return null: both reflecting the new text from one re-lowering.
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
