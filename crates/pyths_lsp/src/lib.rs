//! Minimal Language Server Protocol implementation for PythScribe.
//!
//! This is a from-scratch LSP server (no `tower-lsp` / `lsp-server` deps) so
//! the workspace stays minimal. It implements the subset needed for diagnostics
//! in editors that talk LSP:
//!
//! - `initialize` — handshake; advertises text-document sync (full).
//! - `initialized` — notification; no-op.
//! - `textDocument/didOpen`, `didChange`, `didSave`, `didClose` — manage doc state.
//! - `textDocument/publishDiagnostics` — outbound notification with parse errors.
//! - `shutdown`, `exit` — graceful lifecycle.
//!
//! Future work (deferred): hover, go-to-definition, completion, symbol info.
//! These need integration with the resolver / type checker which is
//! out of scope for the initial server.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};

use serde_json::{json, Value};

/// Server state — the open documents and shutdown flag.
pub struct Server {
    docs: HashMap<String, String>,
    shutdown_received: bool,
}

impl Server {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
            shutdown_received: false,
        }
    }

    /// Process a single JSON-RPC message. Returns Some(response) for requests,
    /// or a list of outbound notifications to push (e.g. publishDiagnostics).
    pub fn handle(&mut self, msg: &Value) -> ServerResponse {
        let method = msg.get("method").and_then(|m| m.as_str());
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(json!({}));

        let mut out = ServerResponse::default();

        match method {
            Some("initialize") => {
                out.response = Some(self.respond(
                    id,
                    json!({
                        "capabilities": {
                            "textDocumentSync": 1,
                            "diagnosticProvider": {
                                "interFileDependencies": false,
                                "workspaceDiagnostics": false,
                            },
                            "documentSymbolProvider": true,
                            "hoverProvider": true,
                            "completionProvider": {
                                "triggerCharacters": [".", " "],
                            },
                            "definitionProvider": true,
                        },
                        "serverInfo": {
                            "name": "pyths-lsp",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                    }),
                ));
            }
            Some("initialized") => {
                // Notification — no response expected.
            }
            Some("textDocument/didOpen") => {
                if let Some(td) = params.get("textDocument") {
                    let uri = td
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    let text = td
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    self.docs.insert(uri.clone(), text.clone());
                    out.notifications.push(self.diagnostics_for(&uri, &text));
                }
            }
            Some("textDocument/didChange") => {
                if let Some(td) = params.get("textDocument") {
                    let uri = td
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    // Full-text sync (capability we advertised): the entire
                    // document text is in changes[0].text.
                    let text = params
                        .get("contentChanges")
                        .and_then(|c| c.as_array())
                        .and_then(|a| a.first())
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    self.docs.insert(uri.clone(), text.clone());
                    out.notifications.push(self.diagnostics_for(&uri, &text));
                }
            }
            Some("textDocument/didSave") => {
                // Optionally re-run diagnostics on save; we already do on
                // every change so save is a no-op.
            }
            Some("textDocument/didClose") => {
                if let Some(td) = params.get("textDocument") {
                    let uri = td
                        .get("uri")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string();
                    self.docs.remove(&uri);
                    // Clear any diagnostics for this doc.
                    out.notifications.push(json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {
                            "uri": uri,
                            "diagnostics": [],
                        },
                    }));
                }
            }
            Some("textDocument/documentSymbol") => {
                let uri = params
                    .get("textDocument")
                    .and_then(|t| t.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let symbols = self.symbols_for(uri);
                out.response = Some(self.respond(id, Value::Array(symbols)));
            }
            Some("textDocument/hover") => {
                let uri = params
                    .get("textDocument")
                    .and_then(|t| t.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let line = params
                    .get("position")
                    .and_then(|p| p.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32;
                let character = params
                    .get("position")
                    .and_then(|p| p.get("character"))
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0) as u32;
                let result = self.hover_for(uri, line, character);
                out.response = Some(self.respond(id, result));
            }
            Some("textDocument/completion") => {
                out.response = Some(self.respond(
                    id,
                    json!({
                        "isIncomplete": false,
                        "items": completion_items(),
                    }),
                ));
            }
            Some("textDocument/definition") => {
                let uri = params
                    .get("textDocument")
                    .and_then(|t| t.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                let line = params
                    .get("position")
                    .and_then(|p| p.get("line"))
                    .and_then(|l| l.as_u64())
                    .unwrap_or(0) as u32;
                let character = params
                    .get("position")
                    .and_then(|p| p.get("character"))
                    .and_then(|c| c.as_u64())
                    .unwrap_or(0) as u32;
                let result = self.definition_for(uri, line, character);
                out.response = Some(self.respond(id, result));
            }
            Some("shutdown") => {
                self.shutdown_received = true;
                out.response = Some(self.respond(id, Value::Null));
            }
            Some("exit") => {
                out.exit = true;
            }
            _ => {
                // Unknown methods get a "method not found" error if they have an id.
                if id.is_some() {
                    out.response = Some(self.error(id, -32601, "Method not found"));
                }
            }
        }
        out
    }

    fn respond(&self, id: Option<Value>, result: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "result": result,
        })
    }

    fn error(&self, id: Option<Value>, code: i32, message: &str) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "error": { "code": code, "message": message },
        })
    }

    /// Parse a document and emit a `publishDiagnostics` notification.
    /// Returns the JSON-RPC notification message.
    pub fn diagnostics_for(&self, uri: &str, source: &str) -> Value {
        let diagnostics: Vec<Value> = match pyths_parser::parse(source) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .iter()
                .map(|e| {
                    let (start_line, start_col) = byte_offset_to_position(source, e.span.start);
                    let (end_line, end_col) = byte_offset_to_position(source, e.span.end);
                    json!({
                        "range": {
                            "start": { "line": start_line, "character": start_col },
                            "end":   { "line": end_line,   "character": end_col },
                        },
                        "severity": 1, // 1 = Error
                        "source": "pyths",
                        "message": e.message,
                    })
                })
                .collect(),
        };
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "diagnostics": diagnostics,
            },
        })
    }

    pub fn document(&self, uri: &str) -> Option<&String> {
        self.docs.get(uri)
    }

    /// Build a list of LSP `DocumentSymbol` entries for the given doc.
    /// Walks the module body for FuncDef and ClassDef.
    pub fn symbols_for(&self, uri: &str) -> Vec<Value> {
        use pyths_syntax::ast::StmtKind;
        let source = match self.docs.get(uri) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let module = match pyths_parser::parse(source) {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for stmt in &module.body {
            match &stmt.kind {
                StmtKind::FuncDef { name, body, .. } => {
                    out.push(symbol_entry(
                        name,
                        12, // SymbolKind::Function
                        source,
                        stmt.span.start,
                        stmt.span.end,
                        children_for(source, body),
                    ));
                }
                StmtKind::ClassDef { name, body, .. } => {
                    out.push(symbol_entry(
                        name,
                        5, // SymbolKind::Class
                        source,
                        stmt.span.start,
                        stmt.span.end,
                        children_for(source, body),
                    ));
                }
                _ => {}
            }
        }
        out
    }

    /// Hover info: identify the token under the cursor and return a
    /// short markdown description if it's a top-level function/class.
    pub fn hover_for(&self, uri: &str, line: u32, character: u32) -> Value {
        use pyths_syntax::ast::StmtKind;
        let source = match self.docs.get(uri) {
            Some(s) => s,
            None => return Value::Null,
        };
        let offset = position_to_byte_offset(source, line, character);
        let word = word_at_offset(source, offset);
        if word.is_empty() {
            return Value::Null;
        }
        // Look up word in top-level definitions.
        let module = match pyths_parser::parse(source) {
            Ok(m) => m,
            Err(_) => return Value::Null,
        };
        for stmt in &module.body {
            match &stmt.kind {
                StmtKind::FuncDef {
                    name,
                    params,
                    return_type,
                    ..
                } if name == &word => {
                    let params_str: Vec<String> = params
                        .iter()
                        .map(|p| match &p.annotation {
                            Some(_) => format!("{}: <type>", p.name),
                            None => p.name.clone(),
                        })
                        .collect();
                    let ret = if return_type.is_some() {
                        " -> <type>"
                    } else {
                        ""
                    };
                    let sig = format!("def {}({}){}\n```", name, params_str.join(", "), ret);
                    return json!({
                        "contents": {
                            "kind": "markdown",
                            "value": format!("```python\n{}", sig),
                        },
                    });
                }
                StmtKind::ClassDef { name, bases, .. } if name == &word => {
                    let bases_str = if bases.is_empty() {
                        String::new()
                    } else {
                        format!("({})", "...")
                    };
                    return json!({
                        "contents": {
                            "kind": "markdown",
                            "value": format!("```python\nclass {}{}\n```", name, bases_str),
                        },
                    });
                }
                _ => {}
            }
        }
        // Built-in keywords / functions
        if let Some(desc) = builtin_doc(&word) {
            return json!({
                "contents": {
                    "kind": "markdown",
                    "value": desc,
                },
            });
        }
        Value::Null
    }

    /// Goto-definition: find the top-level FuncDef/ClassDef matching the word
    /// under cursor, return its location.
    pub fn definition_for(&self, uri: &str, line: u32, character: u32) -> Value {
        use pyths_syntax::ast::StmtKind;
        let source = match self.docs.get(uri) {
            Some(s) => s,
            None => return Value::Null,
        };
        let offset = position_to_byte_offset(source, line, character);
        let word = word_at_offset(source, offset);
        if word.is_empty() {
            return Value::Null;
        }
        let module = match pyths_parser::parse(source) {
            Ok(m) => m,
            Err(_) => return Value::Null,
        };
        for stmt in &module.body {
            let matched_name = match &stmt.kind {
                StmtKind::FuncDef { name, .. } if name == &word => Some(name),
                StmtKind::ClassDef { name, .. } if name == &word => Some(name),
                _ => None,
            };
            if matched_name.is_some() {
                let (sl, sc) = byte_offset_to_position(source, stmt.span.start);
                let (el, ec) = byte_offset_to_position(source, stmt.span.end);
                return json!({
                    "uri": uri,
                    "range": {
                        "start": { "line": sl, "character": sc },
                        "end":   { "line": el, "character": ec },
                    },
                });
            }
        }
        Value::Null
    }

    pub fn shutdown_received(&self) -> bool {
        self.shutdown_received
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct ServerResponse {
    pub response: Option<Value>,
    pub notifications: Vec<Value>,
    pub exit: bool,
}

/// Build a single LSP DocumentSymbol JSON value.
fn symbol_entry(
    name: &str,
    kind: u32,
    source: &str,
    span_start: usize,
    span_end: usize,
    children: Vec<Value>,
) -> Value {
    let (sl, sc) = byte_offset_to_position(source, span_start);
    let (el, ec) = byte_offset_to_position(source, span_end);
    let range = json!({
        "start": { "line": sl, "character": sc },
        "end":   { "line": el, "character": ec },
    });
    json!({
        "name": name,
        "kind": kind,
        "range": range,
        "selectionRange": range,
        "children": children,
    })
}

/// Recursively collect nested method/class symbols inside a body.
fn children_for(source: &str, body: &[pyths_syntax::ast::Stmt]) -> Vec<Value> {
    use pyths_syntax::ast::StmtKind;
    let mut out = Vec::new();
    for stmt in body {
        match &stmt.kind {
            StmtKind::FuncDef {
                name,
                body: child_body,
                ..
            } => {
                out.push(symbol_entry(
                    name,
                    6, // Method
                    source,
                    stmt.span.start,
                    stmt.span.end,
                    children_for(source, child_body),
                ));
            }
            StmtKind::ClassDef {
                name,
                body: child_body,
                ..
            } => {
                out.push(symbol_entry(
                    name,
                    5, // Class
                    source,
                    stmt.span.start,
                    stmt.span.end,
                    children_for(source, child_body),
                ));
            }
            _ => {}
        }
    }
    out
}

/// Convert (line, character) into a byte offset in `source`. ASCII-only fast
/// path; for non-ASCII, treats each char as one column.
fn position_to_byte_offset(source: &str, line: u32, character: u32) -> usize {
    let mut cur_line = 0u32;
    let mut cur_col = 0u32;
    let mut i = 0usize;
    let bytes = source.as_bytes();
    while i < bytes.len() {
        if cur_line == line && cur_col == character {
            return i;
        }
        if bytes[i] == b'\n' {
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
        i += 1;
    }
    i
}

/// Extract the identifier under (or just before) `offset` in source. Returns
/// "" if no identifier at that position. Identifier chars are
/// `[a-zA-Z0-9_]`.
fn word_at_offset(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return String::new();
    }
    let is_id = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    // Walk left to find start
    let mut start = offset;
    while start > 0 && is_id(bytes[start - 1]) {
        start -= 1;
    }
    // Walk right to find end
    let mut end = offset;
    while end < bytes.len() && is_id(bytes[end]) {
        end += 1;
    }
    if start == end {
        return String::new();
    }
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

/// Static completion list — Python keywords + PythScribe builtins.
fn completion_items() -> Vec<Value> {
    const KEYWORDS: &[&str] = &[
        "def", "class", "if", "elif", "else", "while", "for", "in", "return", "import", "from",
        "as", "True", "False", "None", "and", "or", "not", "is", "lambda", "try", "except",
        "finally", "raise", "assert", "pass", "break", "continue", "async", "await", "with",
        "yield",
    ];
    const BUILTINS: &[&str] = &[
        "print",
        "len",
        "range",
        "abs",
        "min",
        "max",
        "int",
        "float",
        "str",
        "bool",
        "list",
        "dict",
        "tuple",
        "set",
        "enumerate",
        "zip",
        "map",
        "filter",
        "sorted",
        "reversed",
        "iter",
        "next",
        "type",
        "isinstance",
    ];
    const PYTHS: &[&str] = &[
        "@component",
        "@dataclass",
        "@validator",
        "@check",
        "use_state",
        "use_effect",
        "use_memo",
        "use_callback",
    ];
    let mut items = Vec::new();
    for k in KEYWORDS {
        items.push(json!({"label": k, "kind": 14})); // Keyword
    }
    for b in BUILTINS {
        items.push(json!({"label": b, "kind": 3})); // Function
    }
    for p in PYTHS {
        items.push(json!({"label": p, "kind": 15, "detail": "PythScribe"})); // Snippet
    }
    items
}

/// Built-in identifier documentation (used for hover when the word matches a
/// known built-in / keyword).
fn builtin_doc(word: &str) -> Option<String> {
    Some(match word {
        "print" => "```\nprint(*args)\n```\nWrite to stdout via console.log.".to_string(),
        "len" => "```\nlen(obj) -> int\n```\nLength of a string, list, tuple, or dict.".to_string(),
        "range" => "```\nrange(start, stop, step)\n```\nNumeric iterator. Compiles to a counter loop in WASM.".to_string(),
        "abs" => "```\nabs(x) -> number\n```\nAbsolute value.".to_string(),
        "def" => "Define a function.".to_string(),
        "class" => "Define a class.".to_string(),
        "lambda" => "```\nlambda x: <expr>\n```\nAnonymous function. WASM-eligible when no captures.".to_string(),
        "@component" => "Mark a function as a React component. PSX inside compiles to `createElement`.".to_string(),
        "@dataclass" => "Auto-generate constructor / __eq__ / toDict / fromDict / validation.".to_string(),
        _ => return None,
    })
}

/// Convert a byte offset in `source` to (line, column) using 0-based UTF-16
/// code units (LSP convention is UTF-16 by default). For PythScribe source,
/// which is typically ASCII-heavy, the difference is negligible.
fn byte_offset_to_position(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut i = 0usize;
    let bytes = source.as_bytes();
    while i < offset && i < bytes.len() {
        if bytes[i] == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        i += 1;
    }
    (line, col)
}

// =======================================================================
// JSON-RPC framing — read/write LSP-framed messages over stdio.
// =======================================================================

/// Read one framed message: parses `Content-Length: N\r\n\r\n` then N bytes.
/// Returns None on EOF.
pub fn read_message<R: Read>(reader: &mut BufReader<R>) -> std::io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let line = header.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break; // end of headers
        }
        if let Some(rest) = line.strip_prefix("Content-Length:") {
            content_length = rest.trim().parse::<usize>().ok();
        }
        // Other headers (Content-Type) are ignored.
    }
    let len = match content_length {
        Some(l) => l,
        None => return Ok(None),
    };
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let msg: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
    Ok(Some(msg))
}

/// Serialize a message and write it framed.
pub fn write_message<W: Write>(writer: &mut W, msg: &Value) -> std::io::Result<()> {
    let body = serde_json::to_string(msg).unwrap();
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}

// =======================================================================
// Tests — exercise the server state machine without real stdio.
// =======================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: u64, method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
    }

    fn note(method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        })
    }

    #[test]
    fn initialize_advertises_capabilities() {
        let mut s = Server::new();
        let r = s.handle(&req(1, "initialize", json!({})));
        let resp = r.response.expect("response");
        let caps = resp
            .get("result")
            .and_then(|r| r.get("capabilities"))
            .unwrap();
        assert_eq!(caps["textDocumentSync"], 1);
        assert!(caps.get("diagnosticProvider").is_some());
    }

    #[test]
    fn did_open_emits_diagnostics_for_clean_source() {
        let mut s = Server::new();
        let r = s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///main.ps",
                    "languageId": "pyths",
                    "version": 1,
                    "text": "def add(a: int, b: int) -> int:\n    return a + b\n",
                },
            }),
        ));
        assert_eq!(r.notifications.len(), 1);
        let diags = &r.notifications[0]["params"]["diagnostics"];
        assert!(diags.is_array());
        assert_eq!(diags.as_array().unwrap().len(), 0);
    }

    #[test]
    fn did_open_emits_errors_for_bad_source() {
        let mut s = Server::new();
        let r = s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///bad.ps",
                    "text": "def add(a: int, b: int -> int:\n    return a + b",
                },
            }),
        ));
        assert_eq!(r.notifications.len(), 1);
        let diags = &r.notifications[0]["params"]["diagnostics"];
        assert!(!diags.as_array().unwrap().is_empty());
        let first = &diags[0];
        assert_eq!(first["severity"], 1); // Error
        assert_eq!(first["source"], "pyths");
        assert!(first["range"]["start"]["line"].is_u64());
    }

    #[test]
    fn did_change_updates_diagnostics() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///x.ps",
                    "text": "def f() -> int:\n    return 1\n",
                },
            }),
        ));
        // Make a change introducing an error.
        let r = s.handle(&note(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": "file:///x.ps", "version": 2 },
                "contentChanges": [
                    { "text": "def f( -> int:\n    return 1\n" }
                ],
            }),
        ));
        assert_eq!(r.notifications.len(), 1);
        let diags = &r.notifications[0]["params"]["diagnostics"];
        assert!(!diags.as_array().unwrap().is_empty());
    }

    #[test]
    fn did_close_clears_diagnostics() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///x.ps", "text": "def f() -> int:\n    return 1\n"}}),
        ));
        let r = s.handle(&note(
            "textDocument/didClose",
            json!({"textDocument": {"uri": "file:///x.ps"}}),
        ));
        assert_eq!(r.notifications.len(), 1);
        let diags = &r.notifications[0]["params"]["diagnostics"];
        assert_eq!(diags.as_array().unwrap().len(), 0);
        assert!(s.document("file:///x.ps").is_none());
    }

    #[test]
    fn shutdown_then_exit() {
        let mut s = Server::new();
        let r = s.handle(&req(2, "shutdown", json!({})));
        assert!(r.response.is_some());
        assert!(s.shutdown_received());
        let r = s.handle(&note("exit", json!({})));
        assert!(r.exit);
    }

    #[test]
    fn unknown_method_returns_error() {
        let mut s = Server::new();
        let r = s.handle(&req(3, "nope/whatever", json!({})));
        let resp = r.response.expect("response");
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn byte_offset_position_basic() {
        let s = "abc\ndef\nghi";
        assert_eq!(byte_offset_to_position(s, 0), (0, 0));
        assert_eq!(byte_offset_to_position(s, 4), (1, 0));
        assert_eq!(byte_offset_to_position(s, 5), (1, 1));
        assert_eq!(byte_offset_to_position(s, 8), (2, 0));
    }

    #[test]
    fn write_then_read_roundtrip() {
        use std::io::Cursor;
        let mut buf: Vec<u8> = Vec::new();
        let msg = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
        write_message(&mut buf, &msg).unwrap();
        let mut reader = BufReader::new(Cursor::new(buf));
        let parsed = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn document_symbols_lists_top_level_defs() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///app.ps",
                    "text": "def foo(a: int) -> int:\n    return a\n\nclass Bar(Exception):\n    pass\n",
                },
            }),
        ));
        let r = s.handle(&req(
            10,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": "file:///app.ps" } }),
        ));
        let result = r.response.expect("response");
        let arr = result["result"].as_array().expect("array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "foo");
        assert_eq!(arr[0]["kind"], 12); // Function
        assert_eq!(arr[1]["name"], "Bar");
        assert_eq!(arr[1]["kind"], 5); // Class
    }

    #[test]
    fn hover_returns_signature_for_known_function() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///app.ps",
                    "text": "def add(a: int, b: int) -> int:\n    return a + b\n",
                },
            }),
        ));
        // Cursor on "add" at line 0, col 4
        let r = s.handle(&req(
            11,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///app.ps" },
                "position": { "line": 0, "character": 5 },
            }),
        ));
        let result = r.response.expect("response");
        let v = result["result"]["contents"]["value"].as_str().unwrap_or("");
        assert!(v.contains("def add"), "hover text: {}", v);
    }

    #[test]
    fn hover_for_builtin() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": { "uri": "file:///x.ps", "text": "x = print(1)\n" },
            }),
        ));
        let r = s.handle(&req(
            12,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": "file:///x.ps" },
                "position": { "line": 0, "character": 5 },
            }),
        ));
        let result = r.response.expect("response");
        let v = result["result"]["contents"]["value"].as_str().unwrap_or("");
        assert!(v.contains("print"), "hover for print: {}", v);
    }

    #[test]
    fn completion_returns_keywords_and_builtins() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///x.ps", "text": ""}}),
        ));
        let r = s.handle(&req(
            13,
            "textDocument/completion",
            json!({"textDocument": {"uri": "file:///x.ps"}, "position": {"line": 0, "character": 0}}),
        ));
        let items = r.response.expect("response")["result"]["items"]
            .as_array()
            .unwrap()
            .clone();
        let labels: Vec<String> = items
            .iter()
            .map(|i| i["label"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(labels.contains(&"def".to_string()));
        assert!(labels.contains(&"return".to_string()));
        assert!(labels.contains(&"print".to_string()));
        assert!(labels.contains(&"@component".to_string()));
    }

    #[test]
    fn definition_jumps_to_function_def() {
        let mut s = Server::new();
        s.handle(&note(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///x.ps",
                    "text": "def add(a: int, b: int) -> int:\n    return a + b\n\nadd(1, 2)\n",
                },
            }),
        ));
        // Cursor on the call site `add(1, 2)` — line 3, col 1
        let r = s.handle(&req(
            14,
            "textDocument/definition",
            json!({
                "textDocument": { "uri": "file:///x.ps" },
                "position": { "line": 3, "character": 1 },
            }),
        ));
        let result = r.response.expect("response");
        // Should point to the def location at line 0
        assert_eq!(result["result"]["range"]["start"]["line"], 0);
    }

    #[test]
    fn word_at_offset_finds_identifier() {
        assert_eq!(super::word_at_offset("hello world", 2), "hello");
        assert_eq!(super::word_at_offset("hello world", 6), "world");
        assert_eq!(super::word_at_offset("hello world", 5), "hello");
        assert_eq!(super::word_at_offset("foo_bar123", 4), "foo_bar123");
    }

    #[test]
    fn position_to_byte_offset_works() {
        let s = "abc\ndef\nghi";
        assert_eq!(super::position_to_byte_offset(s, 0, 0), 0);
        assert_eq!(super::position_to_byte_offset(s, 1, 0), 4);
        assert_eq!(super::position_to_byte_offset(s, 2, 1), 9);
    }
}
