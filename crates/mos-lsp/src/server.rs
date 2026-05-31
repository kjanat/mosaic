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

use crate::diagnostics::{LspDiagnostic, diagnostics_for_document, path_from_uri};

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
                publish_diagnostics(writer, &uri, &state.documents[&uri])?;
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
                publish_diagnostics(writer, &uri, &state.documents[&uri])?;
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
        },
        "serverInfo": {
            "name": "mos-lsp",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

fn publish_diagnostics<W: Write>(writer: &mut W, uri: &str, src: &str) -> Result<()> {
    let path = path_from_uri(uri);
    let diagnostics = diagnostics_for_document(&path, src);
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
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
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

    fn frame(value: &Value) -> Vec<u8> {
        let body = serde_json::to_vec(value).expect("encode");
        let mut buf = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
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
        input.extend(frame(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "textDocument/definition",
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
}
