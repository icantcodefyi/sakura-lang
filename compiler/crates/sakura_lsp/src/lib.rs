//! `sakura lsp`: the Language Server Protocol on stdio over the analysis facade (design §12.6):
//! diagnostics, hover, go to definition, completion, formatting and quick fixes, plus
//! `sakura/holeContext` (the MCP `hole_context` record at a position). Positions are
//! UTF-8 when the client offers it, else UTF-16; the facade itself only sees byte offsets.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types as lsp;
use sakura_analysis::{Analysis, Analyzed, Snapshot, disk};
use sakura_diagnostics::{Diagnostic, FileId, Severity, Span, hole_records};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub fn serve() -> Result<(), String> {
    let (conn, io) = Connection::stdio();
    let (id, params) = conn.initialize_start().map_err(|e| e.to_string())?;
    let params: lsp::InitializeParams =
        serde_json::from_value(params).map_err(|e| e.to_string())?;
    let utf8 = params
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_ref())
        .is_some_and(|e| e.contains(&lsp::PositionEncodingKind::UTF8));
    let versioned_edits = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.workspace_edit.as_ref())
        .and_then(|e| e.document_changes)
        .unwrap_or(false);
    let root = params
        .workspace_folders
        .as_ref()
        .and_then(|f| f.first())
        .and_then(|f| disk::uri_path(f.uri.as_str()))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let caps = lsp::ServerCapabilities {
        position_encoding: Some(if utf8 {
            lsp::PositionEncodingKind::UTF8
        } else {
            lsp::PositionEncodingKind::UTF16
        }),
        text_document_sync: Some(lsp::TextDocumentSyncKind::FULL.into()),
        hover_provider: Some(true.into()),
        definition_provider: Some(lsp::OneOf::Left(true)),
        completion_provider: Some(lsp::CompletionOptions {
            trigger_characters: Some(vec![".".into()]),
            ..Default::default()
        }),
        document_formatting_provider: Some(lsp::OneOf::Left(true)),
        code_action_provider: Some(true.into()),
        ..Default::default()
    };
    let init = json!({
        "capabilities": caps,
        "serverInfo": {"name": "sakura", "version": env!("CARGO_PKG_VERSION")},
    });
    conn.initialize_finish(id, init)
        .map_err(|e| e.to_string())?;
    let mut server = Server {
        disk: Vec::new(),
        root,
        utf8,
        versioned_edits,
        open: HashMap::new(),
        client_uris: HashMap::new(),
        published: HashSet::new(),
        current: None,
        std_dir: materialize_std(),
    };
    server.reload_disk();
    server.main_loop(&conn)?;
    drop(conn);
    io.join().map_err(|e| e.to_string())
}

struct Server {
    /// Workspace files as last read from disk, re-read before every analysis so edits outside
    /// the editor (an agent, `git checkout`) show up on the next keystroke.
    disk: Vec<(String, String)>,
    root: PathBuf,
    utf8: bool,
    /// The client takes `documentChanges`: quick fixes then name the document version they were
    /// computed for, so an editor refuses one that went stale after an edit.
    versioned_edits: bool,
    /// Open documents by snapshot URI: version and text.
    open: HashMap<String, (i32, String)>,
    /// Snapshot URI → the URI the client used for it.
    client_uris: HashMap<String, String>,
    /// Client URIs that currently show diagnostics.
    published: HashSet<String>,
    current: Option<Arc<Analyzed>>,
    /// The embedded std as files, so `sakura:///std/…` locations open in an editor.
    std_dir: Option<PathBuf>,
}

/// Writes the embedded std (read-only) to `<temp>/sakura-std-<hash of the sources>/std/`, which
/// `disk::std_uri` maps back to `sakura:///std/…` when the editor opens one of its files.
/// `None` if it cannot be written; std locations then stay unopenable.
fn materialize_std() -> Option<PathBuf> {
    let files: Vec<(String, String)> = sakura_analysis::std_sources().collect();
    let mut h = std::hash::DefaultHasher::new();
    files.hash(&mut h);
    let dir = std::env::temp_dir()
        .join(format!("sakura-std-{:016x}", h.finish()))
        .join("std");
    let index = "Read-only copy of the std embedded in `sakura`; edits here are ignored.\n";
    let entries = files
        .iter()
        .filter_map(|(uri, text)| Some((uri.strip_prefix("sakura:///std/")?, text.as_str())))
        .chain([("INDEX.md", index)]);
    for (rel, text) in entries {
        let path = dir.join(rel);
        if path.is_file() {
            continue;
        }
        std::fs::create_dir_all(path.parent()?).ok()?;
        // Write then rename, so a concurrent server never reads a partial file.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, text).ok()?;
        let mut perms = std::fs::metadata(&tmp).ok()?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&tmp, perms).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
    }
    Some(dir)
}

impl Server {
    /// Design §17.75: everything the client has sent is taken in one pass, so edits apply (and
    /// diagnostics publish) before any request is answered, every request sees the newest
    /// snapshot, and a request cancelled by then gets `RequestCancelled` instead of an answer.
    fn main_loop(&mut self, conn: &Connection) -> Result<(), String> {
        // `lsp_server` hands messages over one at a time; this queue holds whatever arrived
        // while an analysis ran, so the pass after it sees the cancellations too.
        let (tx, rx) = std::sync::mpsc::channel();
        let incoming = conn.receiver.clone();
        std::thread::spawn(move || incoming.into_iter().all(|m| tx.send(m).is_ok()));
        let send = |m: Message| conn.sender.send(m).map_err(|e| e.to_string());
        let mut shutdown = false;
        while let Ok(first) = rx.recv() {
            let mut requests = Vec::new();
            let mut cancelled = HashSet::new();
            let mut pending = vec![first];
            while !pending.is_empty() {
                let mut dirty = false;
                for msg in pending {
                    match msg {
                        Message::Request(req) => requests.push(req),
                        Message::Notification(n) if n.method == "exit" => return Ok(()),
                        Message::Notification(n) if n.method == "$/cancelRequest" => {
                            cancelled.extend(params::<lsp::CancelParams>(n.params).ok().map(|p| {
                                match p.id {
                                    lsp::NumberOrString::Number(n) => RequestId::from(n),
                                    lsp::NumberOrString::String(s) => RequestId::from(s),
                                }
                            }));
                        }
                        Message::Notification(n) => dirty |= self.apply(n),
                        Message::Response(_) => {}
                    }
                }
                // A compiler crash keeps the previous analysis; the next edit retries.
                if dirty && let Ok(out) = contained(|| self.reanalyze()) {
                    for n in out {
                        send(n.into())?;
                    }
                }
                pending = rx.try_iter().collect();
            }
            for req in requests {
                let id = req.id.clone();
                let resp = if cancelled.contains(&id) {
                    let code = ErrorCode::RequestCanceled as i32;
                    Response::new_err(id, code, "cancelled".into())
                } else if shutdown {
                    let code = ErrorCode::InvalidRequest as i32;
                    Response::new_err(id, code, "the server is shutting down".into())
                } else if req.method == "shutdown" {
                    shutdown = true;
                    Response::new_ok(id, ())
                } else {
                    match contained(|| self.request(req)) {
                        Ok(Ok(v)) => Response::new_ok(id, v),
                        Ok(Err((code, msg))) => Response::new_err(id, code as i32, msg),
                        Err(msg) => Response::new_err(id, ErrorCode::InternalError as i32, msg),
                    }
                };
                send(resp.into())?;
            }
        }
        Ok(())
    }

    fn reload_disk(&mut self) {
        self.disk = match disk::load(&disk::default_roots(&self.root)) {
            Ok(s) => s
                .files()
                .iter()
                .map(|f| (f.uri.clone(), f.text.clone()))
                .collect(),
            Err(_) => sakura_analysis::std_sources().collect(),
        };
    }

    /// The snapshot URI of a client URI: the same scheme `disk::load` uses, so an open file
    /// replaces its disk copy (and an open std checkout file replaces the embedded one).
    fn canonical(&mut self, uri: &lsp::Uri) -> String {
        let canon = canonical(uri);
        self.client_uris
            .insert(canon.clone(), uri.as_str().to_owned());
        canon
    }

    /// Applies a document notification; whether the workspace needs analyzing again. A
    /// `didChange` whose version is not newer than the document's is stale and dropped.
    fn apply(&mut self, n: Notification) -> bool {
        match n.method.as_str() {
            "textDocument/didOpen" => {
                let Ok(p) = params::<lsp::DidOpenTextDocumentParams>(n.params) else {
                    return false;
                };
                let uri = self.canonical(&p.text_document.uri);
                let doc = (p.text_document.version, p.text_document.text);
                self.open.insert(uri, doc);
            }
            "textDocument/didChange" => {
                let Ok(p) = params::<lsp::DidChangeTextDocumentParams>(n.params) else {
                    return false;
                };
                let uri = self.canonical(&p.text_document.uri);
                let version = p.text_document.version;
                let Some(change) = p.content_changes.into_iter().last() else {
                    return false;
                };
                if self.open.get(&uri).is_some_and(|(v, _)| *v >= version) {
                    return false;
                }
                self.open.insert(uri, (version, change.text));
            }
            "textDocument/didSave" | "workspace/didChangeWatchedFiles" => {}
            "textDocument/didClose" => {
                let Ok(p) = params::<lsp::DidCloseTextDocumentParams>(n.params) else {
                    return false;
                };
                let uri = self.canonical(&p.text_document.uri);
                self.open.remove(&uri);
            }
            _ => return false,
        }
        true
    }

    /// Re-analyzes disk files overlaid with open documents and publishes diagnostics.
    fn reanalyze(&mut self) -> Vec<Notification> {
        self.reload_disk();
        let mut files: Vec<(String, String)> = self
            .disk
            .iter()
            .filter(|(uri, _)| !self.open.contains_key(uri))
            .cloned()
            .collect();
        files.extend(self.open.iter().map(|(u, (_, t))| (u.clone(), t.clone())));
        let a = Analysis::analyze(&Snapshot::new(files));
        self.current = Some(a.clone());
        self.publish(&a)
    }

    fn publish(&mut self, a: &Analyzed) -> Vec<Notification> {
        let s = &a.snapshot;
        let mut by_file: HashMap<FileId, Vec<lsp::Diagnostic>> = HashMap::new();
        // A diagnostic without a span (a package-level one) goes at the top of `sakura.toml`,
        // or of the first open document when there is no manifest.
        let mut open: Vec<&String> = self.open.keys().collect();
        open.sort();
        let fallback = s
            .file_ids()
            .find(|&f| disk::is_manifest(&s.file(f).uri))
            .or_else(|| open.iter().find_map(|u| s.file_id(u)));
        for d in &a.check.diagnostics {
            let span = match (d.spans.first(), fallback) {
                (Some(span), _) => span.clone(),
                (None, Some(f)) => Span::primary(f, 0, 0),
                (None, None) => continue,
            };
            by_file
                .entry(span.file)
                .or_default()
                .push(self.diagnostic(s, d, &span));
        }
        let mut now = HashSet::new();
        let mut out = Vec::new();
        for f in s.file_ids() {
            let Some(client) = self.client_uri(&s.file(f).uri) else {
                continue;
            };
            let diagnostics = by_file.remove(&f).unwrap_or_default();
            if diagnostics.is_empty() && !self.published.contains(&client) {
                continue;
            }
            if !diagnostics.is_empty() {
                now.insert(client.clone());
            }
            out.push(publish(&client, diagnostics));
        }
        for gone in self.published.difference(&now) {
            if !out.iter().any(|n| n.params["uri"] == gone.as_str()) {
                out.push(publish(gone, Vec::new()));
            }
        }
        self.published = now;
        out
    }

    /// The URI to show `uri` under: the client's own spelling, a `file://` URI, or for an
    /// embedded std file (`sakura:///std/…`) its materialized copy.
    fn client_uri(&self, uri: &str) -> Option<String> {
        if let Some(c) = self.client_uris.get(uri) {
            return Some(c.clone());
        }
        if let Some(rel) = uri.strip_prefix("sakura:///std/") {
            let path = self.std_dir.as_deref().map(|d| d.join(rel))?;
            return path.is_file().then(|| disk::file_uri(&path));
        }
        uri.starts_with("file://").then(|| uri.to_owned())
    }

    fn diagnostic(&self, s: &Snapshot, d: &Diagnostic, span: &Span) -> lsp::Diagnostic {
        let mut message = d.message.clone();
        if let (Some(e), Some(a)) = (&d.expected, &d.actual) {
            message.push_str(&format!("\nexpected: {}\n  actual: {}", e.render, a.render));
        }
        let related = d
            .spans
            .iter()
            .skip(1)
            .filter_map(|sp| {
                let uri = self.client_uri(&s.file(sp.file).uri)?.parse().ok()?;
                Some(lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        uri,
                        range: self.range(s, sp.file, sp.start, sp.end),
                    },
                    message: sp.label.clone().unwrap_or_default(),
                })
            })
            .collect::<Vec<_>>();
        lsp::Diagnostic {
            range: self.range(s, span.file, span.start, span.end),
            severity: Some(match d.severity {
                Severity::Error => lsp::DiagnosticSeverity::ERROR,
                Severity::Warning => lsp::DiagnosticSeverity::WARNING,
                Severity::Note => lsp::DiagnosticSeverity::INFORMATION,
            }),
            code: Some(lsp::NumberOrString::String(d.code.0.to_owned())),
            code_description: format!("{ERRORS_URL}/{}", d.code.0)
                .parse()
                .ok()
                .map(|href| lsp::CodeDescription { href }),
            source: Some("sakura".into()),
            message,
            related_information: (!related.is_empty()).then_some(related),
            ..Default::default()
        }
    }

    fn range(&self, s: &Snapshot, file: FileId, start: u32, end: u32) -> lsp::Range {
        let lines = Lines::new(&s.file(file).text, self.utf8);
        lsp::Range::new(lines.position(start), lines.position(end))
    }

    fn request(&self, req: Request) -> Result<Value, (ErrorCode, String)> {
        let bad = |e: String| (ErrorCode::InvalidParams, e);
        match req.method.as_str() {
            "textDocument/hover" => {
                let p: lsp::HoverParams = params(req.params).map_err(bad)?;
                let r = self.at(&p.text_document_position_params, |a, f, o| {
                    let h = a.hover(f, o)?;
                    Some(json!(lsp::Hover {
                        contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                            kind: lsp::MarkupKind::Markdown,
                            value: h.markdown,
                        }),
                        range: Some(self.range(&a.snapshot, f, h.range[0], h.range[1])),
                    }))
                });
                Ok(r.unwrap_or(Value::Null))
            }
            "textDocument/definition" => {
                let p: lsp::GotoDefinitionParams = params(req.params).map_err(bad)?;
                let r = self.at(&p.text_document_position_params, |a, f, o| {
                    let l = a.definition(f, o)?;
                    let uri: lsp::Uri = self.client_uri(&l.uri)?.parse().ok()?;
                    let range = self.range(&a.snapshot, l.file, l.range[0], l.range[1]);
                    Some(json!(lsp::Location { uri, range }))
                });
                Ok(r.unwrap_or(Value::Null))
            }
            "textDocument/completion" => {
                let p: lsp::CompletionParams = params(req.params).map_err(bad)?;
                let r = self.at(&p.text_document_position, |a, f, o| {
                    let items: Vec<lsp::CompletionItem> = a
                        .completions(f, o)
                        .into_iter()
                        .map(|c| lsp::CompletionItem {
                            label: c.label,
                            kind: Some(completion_kind(c.kind)),
                            detail: c.detail,
                            ..Default::default()
                        })
                        .collect();
                    Some(json!(items))
                });
                Ok(r.unwrap_or(json!([])))
            }
            "textDocument/formatting" => {
                let p: lsp::DocumentFormattingParams = params(req.params).map_err(bad)?;
                let r = self.document(&p.text_document.uri, |a, f| {
                    let old = &a.snapshot.file(f).text;
                    let new = Analysis::format(&a.snapshot, f).ok()?;
                    let edits = if new == *old {
                        vec![]
                    } else {
                        let end = old.len() as u32;
                        vec![lsp::TextEdit::new(self.range(&a.snapshot, f, 0, end), new)]
                    };
                    Some(json!(edits))
                });
                Ok(r.unwrap_or(Value::Null))
            }
            "textDocument/codeAction" => {
                let p: lsp::CodeActionParams = params(req.params).map_err(bad)?;
                let r = self.document(&p.text_document.uri, |a, f| {
                    Some(json!(self.code_actions(a, f, p.range)))
                });
                Ok(r.unwrap_or(json!([])))
            }
            // Extension: the MCP `hole_context` record of the hole at a position, same JSON.
            "sakura/holeContext" => {
                let p: lsp::TextDocumentPositionParams = params(req.params).map_err(bad)?;
                let r = self.at(&p, |a, f, o| {
                    let s = &a.snapshot;
                    let holes = hole_records(&a.check.holes, s.files(), s.hash());
                    let hole = holes.iter().find(|h| {
                        let at = |k: &str| h["span"][k].as_u64().unwrap_or(0) as u32;
                        h["span"]["uri"] == s.file(f).uri.as_str()
                            && at("start_byte") <= o
                            && o <= at("end_byte")
                    })?;
                    a.hole_context(hole["id"].as_str()?)
                });
                Ok(r.unwrap_or(Value::Null))
            }
            m => Err((ErrorCode::MethodNotFound, format!("unsupported method {m}"))),
        }
    }

    /// Runs `f` on the current analysis and the snapshot file of `uri`.
    fn document<T>(
        &self,
        uri: &lsp::Uri,
        f: impl FnOnce(&Analyzed, FileId) -> Option<T>,
    ) -> Option<T> {
        let a = self.current.clone()?;
        let file = a.snapshot.file_id(&canonical(uri))?;
        f(&a, file)
    }

    /// Runs `f` on the current analysis at the byte offset of `pos`.
    fn at<T>(
        &self,
        pos: &lsp::TextDocumentPositionParams,
        f: impl FnOnce(&Analyzed, FileId, u32) -> Option<T>,
    ) -> Option<T> {
        let utf8 = self.utf8;
        self.document(&pos.text_document.uri, |a, file| {
            let offset = Lines::new(&a.snapshot.file(file).text, utf8).offset(pos.position);
            f(a, file, offset)
        })
    }

    /// One quick fix per fix of each diagnostic overlapping `range` in `file`.
    // `lsp::Uri` caches parse data in a `Cell`; the map's keys are never mutated.
    #[allow(clippy::mutable_key_type)]
    fn code_actions(&self, a: &Analyzed, file: FileId, range: lsp::Range) -> Vec<lsp::CodeAction> {
        let s = &a.snapshot;
        let mut actions = Vec::new();
        for d in &a.check.diagnostics {
            let Some(span) = d.spans.first().filter(|sp| sp.file == file) else {
                continue;
            };
            let at = self.range(s, file, span.start, span.end);
            if at.end < range.start || range.end < at.start {
                continue;
            }
            let lsp_diag = self.diagnostic(s, d, span);
            for (i, fix) in d.fixes.iter().enumerate() {
                let mut changes: HashMap<lsp::Uri, Vec<lsp::TextEdit>> = HashMap::new();
                for e in &fix.edits {
                    let Some(uri) = self
                        .client_uri(&s.file(e.span.file).uri)
                        .and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    let r = self.range(s, e.span.file, e.span.start, e.span.end);
                    changes
                        .entry(uri)
                        .or_default()
                        .push(lsp::TextEdit::new(r, e.replacement.clone()));
                }
                let edit = if self.versioned_edits {
                    let docs = changes.into_iter().map(|(uri, edits)| {
                        let version = self.open.get(&canonical(&uri)).map(|(v, _)| *v);
                        lsp::TextDocumentEdit {
                            text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                                uri,
                                version,
                            },
                            edits: edits.into_iter().map(lsp::OneOf::Left).collect(),
                        }
                    });
                    lsp::WorkspaceEdit {
                        document_changes: Some(lsp::DocumentChanges::Edits(docs.collect())),
                        ..Default::default()
                    }
                } else {
                    lsp::WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }
                };
                actions.push(lsp::CodeAction {
                    title: fix.title.clone(),
                    kind: Some(lsp::CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![lsp_diag.clone()]),
                    edit: Some(edit),
                    is_preferred: Some(i == 0),
                    // The fix's impact class (design §12.1), which an editor shows beside the title.
                    data: Some(json!({"impact": fix.impact, "applicability": fix.applicability})),
                    ..Default::default()
                });
            }
        }
        actions
    }
}

/// Every registry code has a generated page here (design §17.144).
const ERRORS_URL: &str = "https://www.sakuralang.com/docs/errors";

fn canonical(uri: &lsp::Uri) -> String {
    let uri = uri.as_str();
    disk::uri_path(uri)
        .map(|p| disk::std_uri(&p).unwrap_or_else(|| disk::file_uri(&p)))
        .unwrap_or_else(|| uri.to_owned())
}

fn publish(uri: &str, diagnostics: Vec<lsp::Diagnostic>) -> Notification {
    Notification::new(
        "textDocument/publishDiagnostics".into(),
        json!({"uri": uri, "diagnostics": diagnostics}),
    )
}

fn params<T: DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Runs `f`, turning a compiler panic into an error message instead of ending the session.
fn contained<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
        let what = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        format!("internal compiler error (V9001): {what}")
    })
}

fn completion_kind(kind: &str) -> lsp::CompletionItemKind {
    use lsp::CompletionItemKind as K;
    match kind {
        "fn" | "op" => K::FUNCTION,
        "const" => K::CONSTANT,
        "type" | "error" => K::STRUCT,
        "variant" => K::ENUM_MEMBER,
        "effect" => K::INTERFACE,
        "module" => K::MODULE,
        "local" => K::VARIABLE,
        "keyword" => K::KEYWORD,
        _ => K::TEXT,
    }
}

/// Line starts of one text, converting byte offsets to protocol positions and back.
struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
    utf8: bool,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str, utf8: bool) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Lines { text, starts, utf8 }
    }

    fn position(&self, offset: u32) -> lsp::Position {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let before = &self.text[self.starts[line]..offset];
        let character = if self.utf8 {
            before.len()
        } else {
            before.encode_utf16().count()
        };
        lsp::Position::new(line as u32, character as u32)
    }

    /// Clamped to the line's end (before its newline) and to the text.
    fn offset(&self, pos: lsp::Position) -> u32 {
        let Some(&start) = self.starts.get(pos.line as usize) else {
            return self.text.len() as u32;
        };
        let line = &self.text[start..];
        let line = &line[..line.find('\n').unwrap_or(line.len())];
        let mut units = 0;
        for (i, c) in line.char_indices() {
            if units >= pos.character as usize {
                return (start + i) as u32;
            }
            units += if self.utf8 {
                c.len_utf8()
            } else {
                c.len_utf16()
            };
        }
        (start + line.len()) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip_in_both_encodings() {
        let text = "let a = \"é𝄞\"\nb\n";
        for utf8 in [true, false] {
            let lines = Lines::new(text, utf8);
            for (i, _) in text.char_indices() {
                let p = lines.position(i as u32);
                assert_eq!(lines.offset(p), i as u32, "utf8={utf8} offset {i} at {p:?}");
            }
        }
        let utf16 = Lines::new(text, false);
        // `𝄞` is 4 UTF-8 bytes and 2 UTF-16 units.
        assert_eq!(
            utf16.position(text.find('"').unwrap() as u32 + 7),
            lsp::Position::new(0, 12)
        );
        assert_eq!(utf16.position(text.len() as u32), lsp::Position::new(2, 0));
        assert_eq!(
            utf16.offset(lsp::Position::new(1, 99)),
            text.find("b\n").unwrap() as u32 + 1
        );
        assert_eq!(utf16.offset(lsp::Position::new(9, 0)), text.len() as u32);
    }
}
