//! LSP client crate for Stic.
//!
//! Wraps `async-lsp-client` to provide a simple fire-and-forget interface
//! between the synchronous `App` event loop and one-or-more language server
//! processes.  Zero JSON-RPC framing code is required.
//!
//! ```text
//!  App ──LspAction──► LspManager ──► Session (one per extension) ──► server binary
//!                                        │
//!                                        └── LspEvent ──► App (drained each tick)
//! ```
//!
//! # Usage
//!
//! 1. Create a [`LspManager`] with the server configs from `Config` and the
//!    workspace root.
//! 2. Call [`LspManager::send`] with an [`LspAction`] whenever the editor
//!    needs to communicate with a language server.  Sessions are spawned
//!    lazily on the first call for each file extension.
//! 3. Call [`LspManager::drain_events`] once per render tick to collect
//!    [`LspEvent`] items and apply them to the editor state.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_lsp_client::{LspServer, ServerMessage};
use lsp_types::{
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    },
    request::{Completion, GotoDefinition, HoverRequest},
    CompletionContext, CompletionParams, CompletionResponse, CompletionTriggerKind,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverContents,
    HoverParams, InitializeParams, LanguageString, MarkedString, PartialResultParams, Position,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, Url, VersionedTextDocumentIdentifier, WorkDoneProgressParams,
};
use tokio::sync::mpsc;

use config::LspServerConfig;


/// Events emitted by a language server and consumed by the `App` each tick.
#[derive(Debug)]
pub enum LspEvent {
    /// The server completed its initialise handshake and is ready for requests.
    Ready { ext: String },
    /// A fresh batch of diagnostics for a document URI.
    Diagnostics { uri: Url, items: Vec<LspDiagnostic> },
    /// Hover documentation for the last requested cursor position.
    Hover { markdown: String },
    /// Go-to-definition result pointing to a file location.
    Definition { path: PathBuf, line: u32, col: u32 },
    /// Completion items for the last `Complete` request.
    Completions { items: Vec<CompletionResult> },
    /// A non-fatal error string from a failed server interaction.
    Error(String),
}


/// A single completion item returned by a language server.
#[derive(Debug, Clone)]
pub struct CompletionResult {
    /// Primary label shown in the completion popup.
    pub label:      String,
    /// Short kind badge (e.g. `"fn"`, `"var"`, `"kw"`, `"cls"`).
    pub kind_label: Option<String>,
    /// Additional detail string (e.g. the type signature).
    pub detail:     Option<String>,
}


/// A single diagnostic item received from a language server.
#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    /// Zero-based line of the diagnostic range start.
    pub line:     u32,
    /// Zero-based character column of the diagnostic range start.
    pub col:      u32,
    /// LSP severity code: `1` = Error, `2` = Warning, `3` = Info, `4` = Hint.
    pub severity: u8,
    /// Human-readable diagnostic message.
    pub message:  String,
    /// Optional tool or rule name that produced the diagnostic.
    pub source:   Option<String>,
}


/// Actions the `App` can request from a language server.
pub enum LspAction {
    /// Notify the server that a document has been opened.
    DidOpen   { path: PathBuf, text: String, version: i32 },
    /// Notify the server of a full-text document change.
    DidChange { path: PathBuf, text: String, version: i32 },
    /// Notify the server that a document has been saved.
    DidSave   { path: PathBuf },
    /// Notify the server that a document has been closed.
    DidClose  { path: PathBuf },
    /// Request hover documentation for a cursor position.
    Hover     { path: PathBuf, line: u32, col: u32 },
    /// Request the definition location of the symbol at a cursor position.
    GotoDef   { path: PathBuf, line: u32, col: u32 },
    /// Request completion items at the given cursor position.
    Complete  { path: PathBuf, line: u32, col: u32 },
}


/// A handle to one running language server process.
///
/// One `Session` is maintained per file extension inside [`LspManager`].
/// It is created lazily on the first [`LspAction`] for a given extension.
#[derive(Clone)]
struct Session {
    server: LspServer,
    /// Handle of the background task forwarding server messages to the event
    /// channel.  Shared so cloned sessions can still abort it on shutdown.
    forward_task: Arc<tokio::task::JoinHandle<()>>,
}

impl Session {
    /// Spawns the language server binary, performs the LSP initialise
    /// handshake, and starts a background task that forwards server
    /// notifications onto the shared event channel.
    ///
    /// # Arguments
    ///
    /// * `cfg`  - Executable path, arguments, and optional root override.
    /// * `root` - Workspace root URI sent in `InitializeParams`.
    /// * `ext`  - File extension this session handles (e.g. `"rs"`).
    /// * `tx`   - Sender half of the shared `LspEvent` channel.
    ///
    /// # Returns
    ///
    /// A `Result` containing the ready `Session`, or an error if the process
    /// failed to start or the handshake was rejected.
    async fn start(
        cfg:  &LspServerConfig,
        root: &PathBuf,
        ext:  String,
        tx:   mpsc::UnboundedSender<LspEvent>,
    ) -> anyhow::Result<Self> {
        // `async_lsp_client::LspServer::new` panics (rather than returning an
        // `Err`) if the executable cannot be spawned, which would silently
        // kill this detached task with no message reaching the user. Check
        // the binary is resolvable on `PATH` (or an absolute/relative path
        // that exists) up front so a missing server produces a clear,
        // recoverable [`LspEvent::Error`] instead.
        if which::which(&cfg.command).is_err() {
            anyhow::bail!(
                "'{}' not found on PATH — install it or fix the command in {}",
                cfg.command,
                config::Config::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "the stic config".to_string()),
            );
        }

        let (server, mut rx) = LspServer::new(&cfg.command, &cfg.args);

        let root_uri = Url::from_directory_path(root)
            .unwrap_or_else(|_| Url::parse("file:///").unwrap());

        // Bound the handshake: a server that spawns but dies (or never
        // answers `initialize`) would otherwise suspend this future forever
        // while it holds the session-map lock, wedging every later LSP
        // operation — including shutdown at app exit.
        let init = server.initialize(InitializeParams {
            process_id: Some(std::process::id()),
            root_uri:   Some(root_uri),
            ..Default::default()
        });
        tokio::time::timeout(std::time::Duration::from_secs(15), init)
            .await
            .map_err(|_| anyhow::anyhow!(
                "'{}' did not answer the initialize request (is it installed correctly?)",
                cfg.command,
            ))??;
        let _ = server.initialized().await;

        let _ = tx.send(LspEvent::Ready { ext });

        // Background task: forward server → client messages to the event channel.
        let forward_task = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    None      => break,
                    Some(msg) => handle_server_msg(msg, &tx),
                }
            }
        });

        Ok(Self { server, forward_task: Arc::new(forward_task) })
    }

    /// Performs the LSP shutdown sequence: sends the `shutdown` request,
    /// awaits its response, emits the `exit` notification, then aborts the
    /// message-forwarding task.
    ///
    /// Every step is fail-safe — a hung or already-dead server must never
    /// block the editor from quitting, so errors are logged and ignored.
    async fn shutdown(self) {
        if let Err(e) = self.server.shutdown().await {
            // log::warn!("LSP shutdown request failed: {e}");
        }
        self.server.exit().await;
        self.forward_task.abort();
    }

    /// Sends the given action to the language server, awaiting any response
    /// and routing results back through `tx` as [`LspEvent`] items.
    ///
    /// # Arguments
    ///
    /// * `action` - The editor operation to forward to the server.
    /// * `tx`     - Channel for emitting response events back to the `App`.
    async fn dispatch(&self, action: LspAction, tx: &mpsc::UnboundedSender<LspEvent>) {
        match action {
            LspAction::DidOpen { path, text, version } => {
                let _ = self.server.send_notification::<DidOpenTextDocument>(
                    DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri(&path), language_id: file_ext(&path), version, text,
                        },
                    }).await;
            }

            LspAction::DidChange { path, text, version } => {
                let _ = self.server.send_notification::<DidChangeTextDocument>(
                    DidChangeTextDocumentParams {
                        text_document: VersionedTextDocumentIdentifier { uri: uri(&path), version },
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None, range_length: None, text,
                        }],
                    }).await;
            }

            LspAction::DidSave { path } => {
                let _ = self.server.send_notification::<DidSaveTextDocument>(
                    DidSaveTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri: uri(&path) },
                        text: None,
                    }).await;
            }

            LspAction::DidClose { path } => {
                let _ = self.server.send_notification::<DidCloseTextDocument>(
                    DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri: uri(&path) },
                    }).await;
            }

            LspAction::Hover { path, line, col } => {
                let res = self.server.send_request::<HoverRequest>(HoverParams {
                    text_document_position_params: tdp(&path, line, col),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                }).await;
                match res {
                    Ok(Some(h)) => {
                        let md: String = hover_to_markdown(h.contents);
                        let _ = tx.send(LspEvent::Hover { markdown: md });
                    }
                    Ok(None)  => {}
                    Err(e)    => { let _ = tx.send(LspEvent::Error(e.to_string())); }
                }
            }

            LspAction::Complete { path, line, col } => {
                let res = self.server.send_request::<Completion>(CompletionParams {
                    text_document_position: tdp(&path, line, col),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                    context: Some(CompletionContext {
                        trigger_kind: CompletionTriggerKind::INVOKED,
                        trigger_character: None,
                    }),
                }).await;
                match res {
                    Ok(Some(resp)) => {
                        let raw_items = match resp {
                            CompletionResponse::Array(v) => v,
                            CompletionResponse::List(l)  => l.items,
                        };
                        let items: Vec<CompletionResult> = raw_items.into_iter()
                            .map(|i| CompletionResult {
                                label:      i.label,
                                kind_label: i.kind.map(completion_kind_label),
                                detail:     i.detail,
                            })
                            .collect();
                        let _ = tx.send(LspEvent::Completions { items });
                    }
                    Ok(None)  => {}
                    Err(e)    => { let _ = tx.send(LspEvent::Error(e.to_string())); }
                }
            }

            LspAction::GotoDef { path, line, col } => {
                let res = self.server.send_request::<GotoDefinition>(GotoDefinitionParams {
                    text_document_position_params: tdp(&path, line, col),
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                }).await;
                let loc = match res {
                    Ok(Some(GotoDefinitionResponse::Scalar(l)))   => Some(l),
                    Ok(Some(GotoDefinitionResponse::Array(ls)))   => ls.into_iter().next(),
                    Ok(Some(GotoDefinitionResponse::Link(links))) => {
                        links.into_iter().next().map(|l| lsp_types::Location {
                            uri:   l.target_uri,
                            range: l.target_range,
                        })
                    }
                    Ok(None) => None,
                    Err(e)   => { let _ = tx.send(LspEvent::Error(e.to_string())); return; }
                };
                if let Some(l) = loc {
                    if let Ok(p) = l.uri.to_file_path() {
                        let _ = tx.send(LspEvent::Definition {
                            path: p,
                            line: l.range.start.line,
                            col:  l.range.start.character,
                        });
                    }
                }
            }
        }
    }
}


/// Translates a raw [`ServerMessage`] into zero-or-one [`LspEvent`] items.
///
/// Currently handles `textDocument/publishDiagnostics` notifications and
/// silently drops server-initiated requests (e.g. `window/workDoneProgress/create`).
///
/// # Arguments
///
/// * `msg` - Message received from the language server.
/// * `tx`  - Channel used to forward the translated event to the `App`.
fn handle_server_msg(msg: ServerMessage, tx: &mpsc::UnboundedSender<LspEvent>) {
    match msg {
        // `method` and `params` are public fields on NotificationMessage.
        ServerMessage::Notification(n) if n.method == "textDocument/publishDiagnostics" => {
            let Some(p)   = n.params                      else { return };
            let Some(uri_s)   = p["uri"].as_str()         else { return };
            let Some(raw)     = p["diagnostics"].as_array() else { return };
            let uri: Url = Url::parse(uri_s)
                .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap());
            let items: Vec<LspDiagnostic> = raw.iter().filter_map(|d| Some(LspDiagnostic {
                line:     d["range"]["start"]["line"].as_u64()?      as u32,
                col:      d["range"]["start"]["character"].as_u64()? as u32,
                severity: d["severity"].as_u64().unwrap_or(2)        as u8,
                message:  d["message"].as_str()?.to_owned(),
                source:   d["source"].as_str().map(str::to_owned),
            })).collect();
            let _ = tx.send(LspEvent::Diagnostics { uri, items });
        }
        ServerMessage::Request(_) => {
            // Silently ignore server-initiated requests such as
            // `window/workDoneProgress/create` or `workspace/configuration`.
            // Most servers do not stall on unanswered capability-probe requests.
        }
        _ => {}
    }
}


/// Coordinates all language server sessions for the current workspace.
///
/// One [`Session`] is maintained per file extension.  Sessions are created
/// lazily on the first [`LspManager::send`] call for a given extension and
/// reused for all subsequent calls.
pub struct LspManager {
    /// Server configurations keyed by file extension.
    configs:  HashMap<String, LspServerConfig>,
    /// Workspace root passed to each server's `InitializeParams`.
    root:     PathBuf,
    /// Active server sessions, keyed by file extension.
    sessions: Arc<tokio::sync::Mutex<HashMap<String, Session>>>,
    /// Sender half of the event channel shared with all session tasks.
    evt_tx:   mpsc::UnboundedSender<LspEvent>,
    /// Receiver half consumed by [`LspManager::drain_events`].
    evt_rx:   mpsc::UnboundedReceiver<LspEvent>,
}

impl LspManager {
    /// Creates a new `LspManager` for the given server configurations and
    /// workspace root.  No server processes are started until [`send`] is
    /// called for a matching extension.
    ///
    /// # Arguments
    ///
    /// * `configs` - Map from file extension (e.g. `"rs"`) to server config.
    /// * `root`    - Workspace root directory forwarded in `InitializeParams`.
    ///
    /// # Returns
    ///
    /// A ready `LspManager` with no active sessions.
    pub fn new(configs: HashMap<String, LspServerConfig>, root: PathBuf) -> Self {
        let (evt_tx, evt_rx) = mpsc::unbounded_channel();
        Self {
            configs,
            root,
            sessions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            evt_tx,
            evt_rx,
        }
    }

    /// Whether a language server is configured for the given file extension,
    /// regardless of whether its session has been spawned yet.
    ///
    /// # Arguments
    ///
    /// * `file_ext` - File extension to look up (e.g. `"rs"`).
    pub fn has_server(&self, file_ext: &str) -> bool {
        self.configs.contains_key(file_ext)
    }

    /// Fire-and-forget dispatch: spawns the server session if needed, then
    /// forwards `action` to it on a Tokio task.
    ///
    /// If no server is configured for `file_ext` the call is silently dropped.
    /// If the server fails to start an [`LspEvent::Error`] is emitted instead.
    ///
    /// # Arguments
    ///
    /// * `file_ext` - File extension that selects the target session (e.g. `"rs"`).
    /// * `action`   - The editor operation to forward to the language server.
    pub fn send(&self, file_ext: &str, action: LspAction) {
        let sessions = Arc::clone(&self.sessions);
        let ext      = file_ext.to_owned();
        let tx       = self.evt_tx.clone();
        let cfg      = self.configs.get(&ext).cloned();
        let root     = self.root.clone();

        tokio::spawn(async move {
            let mut map = sessions.lock().await;
            if !map.contains_key(&ext) {
                let Some(cfg) = cfg else { return };
                match Session::start(&cfg, &root, ext.clone(), tx.clone()).await {
                    Ok(s)  => { map.insert(ext.clone(), s); }
                    Err(e) => {
                        let _ = tx.send(LspEvent::Error(format!("spawn {ext}: {e}")));
                        return;
                    }
                }
            }
            if let Some(s) = map.get(&ext).cloned() {
                // Release the lock before the potentially slow `dispatch` await.
                drop(map);
                s.dispatch(action, &tx).await;
            }
        });
    }

    /// Gracefully shuts down every active language server session.
    ///
    /// Each server receives the LSP `shutdown` request followed by the `exit`
    /// notification, per the protocol's teardown sequence, so servers like
    /// rust-analyzer terminate cleanly instead of panicking with
    /// `client exited without proper shutdown sequence`.
    ///
    /// Sessions are shut down concurrently, each bounded by a two-second
    /// timeout, and the entire teardown — including acquiring the session
    /// map lock, which a wedged spawn task could be holding — is capped at
    /// three seconds, so nothing can hang application exit.
    /// Must be awaited before the process terminates (i.e. at the end of the
    /// main event loop).
    pub async fn shutdown_all(self) {
        let teardown = async {
            let mut map = self.sessions.lock().await;
            if map.is_empty() {
                return;
            }

            let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
            for (ext, session) in map.drain() {
                tasks.spawn(async move {
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        session.shutdown(),
                    ).await {
                        Ok(())  => {},
                        Err(_)  => {},
                    }
                });
            }
            while tasks.join_next().await.is_some() {}
        };

        if tokio::time::timeout(std::time::Duration::from_secs(3), teardown).await.is_err() {
            // log::warn!("LSP teardown timed out; abandoning remaining sessions");
        }
    }

    /// Drains all pending [`LspEvent`] items without blocking.
    ///
    /// Should be called exactly once per render tick so that language server
    /// responses are applied to the editor state in a predictable order.
    ///
    /// # Returns
    ///
    /// A `Vec` of all events received since the last call, possibly empty.
    pub fn drain_events(&mut self) -> Vec<LspEvent> {
        std::iter::from_fn(|| self.evt_rx.try_recv().ok()).collect()
    }
}


/// Converts `HoverContents` to a plain markdown string.
///
/// In lsp-types 0.94, `MarkedString` is an enum with no `.value()` method,
/// so each variant must be matched explicitly.
///
/// # Arguments
///
/// * `contents` - The hover payload from the language server.
///
/// # Returns
///
/// A single markdown string suitable for display in the hover overlay.
fn hover_to_markdown(contents: HoverContents) -> String {
    match contents {
        HoverContents::Scalar(m)  => marked_string_value(m),
        HoverContents::Array(ms)  => ms.into_iter()
                                       .map(marked_string_value)
                                       .collect::<Vec<_>>()
                                       .join("\n\n"),
        HoverContents::Markup(mu) => mu.value,
    }
}

/// Extracts the text value from a [`MarkedString`] variant.
///
/// # Arguments
///
/// * `m` - A `MarkedString` item from a hover response.
///
/// # Returns
///
/// The inner string value regardless of which variant is active.
fn marked_string_value(m: MarkedString) -> String {
    match m {
        MarkedString::String(s)                          => s,
        MarkedString::LanguageString(LanguageString { value, .. }) => value,
    }
}

/// Maps an LSP `CompletionItemKind` integer to a short display badge.
fn completion_kind_label(kind: lsp_types::CompletionItemKind) -> String {
    use lsp_types::CompletionItemKind as K;
    match kind {
        K::TEXT           => "txt",
        K::METHOD         => "mth",
        K::FUNCTION       => "fn",
        K::CONSTRUCTOR    => "ctor",
        K::FIELD          => "fld",
        K::VARIABLE       => "var",
        K::CLASS          => "cls",
        K::INTERFACE      => "ifc",
        K::MODULE         => "mod",
        K::PROPERTY       => "prop",
        K::UNIT           => "unit",
        K::VALUE          => "val",
        K::ENUM           => "enum",
        K::KEYWORD        => "kw",
        K::SNIPPET        => "snip",
        K::COLOR          => "color",
        K::FILE           => "file",
        K::REFERENCE      => "ref",
        K::FOLDER         => "dir",
        K::ENUM_MEMBER    => "em",
        K::CONSTANT       => "const",
        K::STRUCT         => "struct",
        K::EVENT          => "evt",
        K::OPERATOR       => "op",
        K::TYPE_PARAMETER => "tp",
        _                 => "?",
    }.to_string()
}


/// Converts a filesystem path to a `file://` URI suitable for LSP messages.
///
/// Canonicalises the path when possible so the server receives an absolute URI.
/// Falls back to a best-effort `file:///…` URL if canonicalisation fails.
///
/// # Arguments
///
/// * `path` - The path to convert.
///
/// # Returns
///
/// A [`Url`] with the `file` scheme.
fn uri(path: &PathBuf) -> Url {
    let abs = path.canonicalize().unwrap_or_else(|_| path.clone());
    Url::from_file_path(&abs)
        .unwrap_or_else(|_| Url::parse(&format!("file:///{}", abs.display())).unwrap())
}

/// Extracts the file extension from a path as a `String`.
///
/// Returns an empty string for paths with no extension.
///
/// # Arguments
///
/// * `path` - The path whose extension is extracted.
///
/// # Returns
///
/// The extension string (e.g. `"rs"`) without a leading dot.
fn file_ext(path: &PathBuf) -> String {
    path.extension().and_then(|s: &std::ffi::OsStr| s.to_str()).unwrap_or("").to_owned()
}

/// Builds a [`TextDocumentPositionParams`] value for request helpers.
///
/// # Arguments
///
/// * `path` - Document path, converted to a `file://` URI.
/// * `line` - Zero-based line number.
/// * `col`  - Zero-based character column.
///
/// # Returns
///
/// A populated [`TextDocumentPositionParams`].
fn tdp(path: &PathBuf, line: u32, col: u32) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri(path) },
        position:      Position { line, character: col },
    }
}
