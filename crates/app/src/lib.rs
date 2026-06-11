//! Top-level application state and mode machine.
//!
//! The `App` struct is the single owner of every major subsystem – editor
//! buffers, configuration, LSP manager, file-tree, search state – and acts as
//! the authoritative source-of-truth threaded through the event loop.

use std::path::{Path, PathBuf};

use anyhow::Result;

use command_palette::CommandPalette;
use config::Config;
use editor::Editor;
use lsp::{LspAction, LspEvent, LspManager};


/// Editor interaction mode, governing how keystrokes are interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Default modal navigation; keypresses trigger editor commands.
    Normal,
    /// Text insertion mode; keypresses append characters to the buffer.
    Insert,
    /// Command-line input bar for entering editor commands (`:` commands).
    Command,
    /// Incremental text search mode.
    Search,
    /// Jump-to-line number input mode.
    GotoLine,
    /// Fuzzy command palette overlay mode.
    CommandPalette,
    /// File-tree navigation panel mode.
    FileTree,
    /// Save-As filename prompt; `prompt_input` accumulates the destination path.
    SaveAs,
}


// ── File-tree data model ──────────────────────────────────────────────────────

/// A single entry returned by an async directory scan.
#[derive(Debug, Clone)]
pub struct FsEntry {
    /// Display name (no path prefix).
    pub name:   String,
    /// Absolute path.
    pub path:   PathBuf,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// One node in the lazy file tree.
///
/// Children start as `None` (not yet loaded) and are populated when the user
/// expands the directory via an async background scan.
#[derive(Debug)]
pub struct FileTreeNode {
    /// The filesystem entry this node represents.
    pub entry:    FsEntry,
    /// Whether this directory node is currently expanded.
    pub expanded: bool,
    /// `None` = not yet loaded; `Some(_)` = loaded (possibly empty).
    pub children: Option<Vec<FileTreeNode>>,
    /// `true` while a background scan is in-flight for this directory.
    pub loading:  bool,
}

/// Internal message produced by a background directory scan task.
struct ScanResult {
    /// The directory whose direct children were read.
    parent:  PathBuf,
    /// The sorted entries that were found.
    entries: Vec<FsEntry>,
}

/// Persistent navigation state for the file-tree panel.
///
/// ## Architecture
///
/// The tree is **lazy**: only the direct children of the root are loaded at
/// construction time; subdirectory contents are fetched on demand when the
/// user expands a node.  All `tokio::fs` I/O runs on a background task and
/// the result is sent back over an unbounded channel.  `drain_scan_results`
/// (called once per tick from `App::tick`) applies the results to the tree
/// without blocking the UI thread.
///
/// The displayed list is always derived on demand by `visible_flat`, which
/// does a depth-first traversal of expanded nodes.  This avoids keeping a
/// stale cached flat list and is O(n_visible) — cheap for a bounded tree.
pub struct FileTreeState {
    root:       PathBuf,
    /// Root-level children (depth 0).  Empty until the initial scan lands.
    pub nodes:  Vec<FileTreeNode>,
    /// Index into the *visible* flat list.
    pub selected:   usize,
    /// First visible row (vertical scroll offset).
    pub scroll_top: usize,
    scan_tx: tokio::sync::mpsc::UnboundedSender<ScanResult>,
    scan_rx: tokio::sync::mpsc::UnboundedReceiver<ScanResult>,
}

impl FileTreeState {
    /// Creates a new `FileTreeState` rooted at `root` and immediately kicks
    /// off an async scan of the root directory.  The tree will be empty until
    /// the first tick drains the scan result.
    ///
    /// # Arguments
    ///
    /// * `root` - Absolute path of the directory to use as the tree root.
    ///
    /// # Returns
    ///
    /// A `FileTreeState` ready to receive scan results on the next tick.
    pub fn new(root: PathBuf) -> Self {
        let (scan_tx, scan_rx) = tokio::sync::mpsc::unbounded_channel();
        let tx  = scan_tx.clone();
        let dir = root.clone();
        tokio::spawn(async move {
            let entries = scan_dir(&dir).await;
            let _ = tx.send(ScanResult { parent: dir, entries });
        });
        Self { root, nodes: Vec::new(), selected: 0, scroll_top: 0, scan_tx, scan_rx }
    }

    /// Returns a flattened view of the visible tree: `(depth, node)` pairs for
    /// every node that is currently reachable (i.e. all ancestors are expanded).
    ///
    /// Allocation is O(n_visible) and bounded by the depth-3 scan limit,
    /// making it safe to call every render frame.
    pub fn visible_flat(&self) -> Vec<(usize, &FileTreeNode)> {
        fn collect<'a>(
            nodes: &'a [FileTreeNode],
            depth: usize,
            out:   &mut Vec<(usize, &'a FileTreeNode)>,
        ) {
            for node in nodes {
                out.push((depth, node));
                if node.expanded {
                    if let Some(ch) = &node.children {
                        collect(ch, depth + 1, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.nodes, 0, &mut out);
        out
    }

    /// Returns the path of the currently selected entry, if any.
    ///
    /// # Returns
    ///
    /// The selected entry's absolute `PathBuf`, or `None` when the tree
    /// is still loading.
    pub fn selected_path(&self) -> Option<PathBuf> {
        let flat = self.visible_flat();
        flat.get(self.selected).map(|(_, n)| n.entry.path.clone())
    }

    /// Moves the selection one row up, adjusting scroll to keep it visible.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll_top {
                self.scroll_top = self.selected;
            }
        }
    }

    /// Moves the selection one row down, adjusting scroll to keep it visible.
    ///
    /// # Arguments
    ///
    /// * `visible_height` - Number of rows the panel can display.
    pub fn move_down(&mut self, visible_height: usize) {
        let max = self.visible_flat().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
            let bottom = self.scroll_top + visible_height.saturating_sub(1);
            if self.selected > bottom {
                self.scroll_top += 1;
            }
        }
    }

    /// Toggles the expand/collapse state of the currently selected directory.
    ///
    /// - If the node is a **collapsed directory** whose children have not yet
    ///   been loaded, a background scan is spawned and `loading` is set to
    ///   `true` until the result arrives.
    /// - Directories at depth ≥ 3 are not expanded (depth limit).
    /// - Files are silently ignored.
    pub fn toggle_selected(&mut self) {
        let (depth, path, is_dir) = {
            let flat = self.visible_flat();
            let Some((d, node)) = flat.get(self.selected) else { return };
            if !node.entry.is_dir { return; }
            (*d, node.entry.path.clone(), true)
        };
        if !is_dir || depth >= 3 { return; }

        let needs_load;
        if let Some(node) = find_node_mut(&mut self.nodes, &path) {
            if node.expanded {
                node.expanded = false;
                return;
            }
            node.expanded  = true;
            needs_load = node.children.is_none() && !node.loading;
            if needs_load { node.loading = true; }
        } else {
            return;
        }

        if needs_load {
            let tx  = self.scan_tx.clone();
            let dir = path.clone();
            tokio::spawn(async move {
                let entries = scan_dir(&dir).await;
                let _ = tx.send(ScanResult { parent: dir, entries });
            });
        }
    }

    /// If the selected node is an expanded directory, collapse it.
    /// Otherwise, jump the selection to the nearest ancestor directory
    /// that is visible in the current flat view.
    pub fn collapse_or_jump_parent(&mut self) {
        let (path, is_dir, expanded) = {
            let flat = self.visible_flat();
            let Some((_, node)) = flat.get(self.selected) else { return };
            (node.entry.path.clone(), node.entry.is_dir, node.expanded)
        };

        if is_dir && expanded {
            if let Some(node) = find_node_mut(&mut self.nodes, &path) {
                node.expanded = false;
            }
            return;
        }

        // Move selection to the parent directory row if it is visible.
        if let Some(parent) = path.parent() {
            let parent = parent.to_path_buf();
            let flat = self.visible_flat();
            if let Some(idx) = flat.iter().position(|(_, n)| n.entry.path == parent) {
                self.selected = idx;
                if self.selected < self.scroll_top {
                    self.scroll_top = self.selected;
                }
            }
        }
    }

    /// Sets the selection to the row nearest `terminal_row` and returns the
    /// path of the newly selected entry (for double-click-style open).
    ///
    /// # Arguments
    ///
    /// * `terminal_row` - Raw terminal row from a mouse event.
    ///
    /// # Returns
    ///
    /// The selected entry's path after updating the selection.
    pub fn click_row(&mut self, terminal_row: usize) -> Option<PathBuf> {
        let visible_count = self.visible_flat().len();
        if visible_count == 0 { return None; }
        self.selected = (self.scroll_top + terminal_row).min(visible_count - 1);
        self.selected_path()
    }

    /// Clears the tree and re-queues an async scan of the root directory.
    pub fn refresh(&mut self) {
        self.nodes.clear();
        self.selected   = 0;
        self.scroll_top = 0;
        let tx  = self.scan_tx.clone();
        let dir = self.root.clone();
        tokio::spawn(async move {
            let entries = scan_dir(&dir).await;
            let _ = tx.send(ScanResult { parent: dir, entries });
        });
    }

    /// Drains all completed scan results from the background channel and
    /// patches the tree.
    ///
    /// Must be called exactly once per event-loop tick.  Non-blocking.
    pub fn drain_scan_results(&mut self) {
        while let Ok(result) = self.scan_rx.try_recv() {
            if result.parent == self.root {
                // Initial root load.
                self.nodes = make_nodes(result.entries);
            } else if let Some(node) = find_node_mut(&mut self.nodes, &result.parent) {
                node.loading  = false;
                node.children = Some(make_nodes(result.entries));
            }
        }
    }
}

// ── File-tree helpers ─────────────────────────────────────────────────────────

/// Converts a list of `FsEntry` values into a fresh list of leaf `FileTreeNode`s.
fn make_nodes(entries: Vec<FsEntry>) -> Vec<FileTreeNode> {
    entries.into_iter().map(|e| FileTreeNode {
        entry:    e,
        expanded: false,
        children: None,
        loading:  false,
    }).collect()
}

/// Reads the direct children of `dir` asynchronously, returning them sorted
/// with directories first then files, both alphabetically.
///
/// Dot-files are skipped.  Errors reading individual entries are silently
/// ignored so a single broken symlink does not abort the scan.
async fn scan_dir(dir: &Path) -> Vec<FsEntry> {
    let Ok(mut rd) = tokio::fs::read_dir(dir).await else { return vec![] };
    let mut entries: Vec<FsEntry> = Vec::new();
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') { continue; }
        let Ok(meta) = e.metadata().await else { continue };
        entries.push(FsEntry { is_dir: meta.is_dir(), path: e.path(), name });
    }
    entries.sort_unstable_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    entries
}

/// Recursively searches `nodes` for the node whose path equals `target`,
/// returning a mutable reference to it.
fn find_node_mut<'a>(
    nodes:  &'a mut Vec<FileTreeNode>,
    target: &Path,
) -> Option<&'a mut FileTreeNode> {
    for node in nodes.iter_mut() {
        if node.entry.path == target {
            return Some(node);
        }
        if let Some(children) = &mut node.children {
            if let Some(found) = find_node_mut(children, target) {
                return Some(found);
            }
        }
    }
    None
}


/// Transient state for incremental text search within the active buffer.
pub struct SearchState {
    /// Current search query string.
    pub query:      String,
    /// Position of the most recently highlighted match `(line, col)`.
    pub last_match: Option<(usize, usize)>,
}

impl SearchState {
    /// Creates an empty `SearchState` with no query and no previous match.
    ///
    /// # Returns
    ///
    /// An initialized default `SearchState`.
    pub fn new() -> Self { Self { query: String::new(), last_match: None } }
}

impl Default for SearchState { fn default() -> Self { Self::new() } }


/// Floating documentation overlay populated by LSP hover responses.
#[derive(Debug, Default)]
pub struct HoverOverlay {
    /// Markdown-formatted content received from the language server.
    pub content: String,
    /// Whether the overlay should be rendered on the current frame.
    pub visible: bool,
}


/// Root application state: owns all major subsystems and acts as the single
/// source of truth threaded through the event loop.
pub struct App {
    /// The multi-buffer text editor subsystem.
    pub editor:          Editor,
    /// Fuzzy command palette overlay.
    pub command_palette: CommandPalette,
    /// Current interaction mode driving keypress routing.
    pub mode:            Mode,
    /// Runtime configuration loaded from disk.
    pub config:          Config,
    /// Set to `true` to signal the event loop to exit.
    pub should_quit:     bool,
    /// Transient status-bar message cleared after `message_ticks` ticks.
    pub message:         Option<String>,
    /// Shared mutable string backing search / goto-line / command prompts.
    pub prompt_input:    String,
    /// Active text-search state.
    pub search:          SearchState,
    /// File-tree panel state; `None` when the panel is hidden.
    pub file_tree:       Option<FileTreeState>,
    /// Whether the integrated terminal panel is visible.
    pub show_terminal:   bool,
    /// Whether the diagnostics panel is visible.
    pub show_diag:       bool,
    /// In-process clipboard for copy/paste operations.
    pub clipboard:       String,
    /// Hover-documentation overlay rendered above the cursor.
    pub hover:           HoverOverlay,
    /// LSP manager; `None` when no servers are configured.
    pub lsp:             Option<LspManager>,

    /// Remaining ticks before the transient status message is cleared.
    message_ticks: u8,
}

impl App {
    /// Constructs a fully initialized application state, loading configuration
    /// from disk and preparing LSP sessions to be spawned lazily on first use.
    ///
    /// # Returns
    ///
    /// A ready-to-run `App` instance with the mode set to `Normal`.
    pub fn new() -> Self {
        let config: Config    = Config::load();
        let editor: Editor    = Editor::new(config.clone());
        let show_ft: bool   = config.ui.show_file_tree;
        let show_term: bool = config.ui.show_terminal;
        let show_diag: bool = config.ui.show_diagnostics;

        let file_tree: Option<FileTreeState> = if show_ft {
            std::env::current_dir().ok().map(FileTreeState::new)
        } else {
            None
        };

        // Initialise the LSP manager; individual server sessions are spawned
        // lazily on the first `send` call for a given file extension.
        let lsp: Option<LspManager> = if !config.lsp.servers.is_empty() {
            let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            Some(LspManager::new(config.lsp.servers.clone(), root))
        } else {
            None
        };

        Self {
            editor,
            command_palette: CommandPalette::new(),
            mode:            Mode::Normal,
            config,
            should_quit:     false,
            message:         None,
            message_ticks:   0,
            prompt_input:    String::new(),
            search:          SearchState::new(),
            file_tree,
            show_terminal:   show_term,
            show_diag,
            clipboard:       String::new(),
            hover:           HoverOverlay::default(),
            lsp,
        }
    }

    /// Posts a transient status-bar message that auto-clears after sixty ticks.
    ///
    /// # Arguments
    ///
    /// * `msg` - Human-readable status text to display in the status bar.
    pub fn set_message<S: Into<String>>(&mut self, msg: S) {
        self.message       = Some(msg.into());
        self.message_ticks = 60;
    }

    /// Advances one event-loop tick: drains pending LSP events and file-tree
    /// scan results, then ages out the transient status message.
    pub fn tick(&mut self) {
        self.drain_lsp_events();

        if let Some(ft) = &mut self.file_tree {
            ft.drain_scan_results();
        }

        if let Some(t) = self.message_ticks.checked_sub(1) {
            self.message_ticks = t;
            if t == 0 { self.message = None; }
        }
    }

    /// Processes all pending [`LspEvent`] items from the manager and updates
    /// editor state accordingly.
    ///
    /// Called once per tick so that LSP responses are applied synchronously
    /// within the main thread without blocking the event loop.
    fn drain_lsp_events(&mut self) {
        let Some(lsp) = &mut self.lsp else { return };

        for event in lsp.drain_events() {
            match event {
                // Server finished initialising – open the currently active file.
                LspEvent::Ready { ext } => {
                    self.set_message(format!("LSP ready (.{ext})"));
                    let path: Option<PathBuf>    = self.editor.buf().path.clone();
                    let text: String    = self.editor.buf().text();
                    let version: i32 = self.editor.buf().version;
                    if let (Some(path), Some(lsp)) = (path, &mut self.lsp) {
                        lsp.send(&ext, LspAction::DidOpen { path, text, version });
                    }
                }

                // Diagnostics from the server – attach to the active buffer.
                LspEvent::Diagnostics { uri: _, items } => {
                    let buf: &mut buffer::Buffer = self.editor.buf_mut();
                    buf.diagnostics = items
                        .into_iter()
                        .map(|d: lsp::LspDiagnostic| buffer::Diagnostic {
                            line:     d.line     as usize,
                            col:      d.col      as usize,
                            severity: lsp_severity_to_buf(d.severity),
                            message:  d.message,
                        })
                        .collect();
                }

                // Hover response – populate and show the overlay.
                LspEvent::Hover { markdown } => {
                    if !markdown.is_empty() {
                        self.hover.content = markdown;
                        self.hover.visible = true;
                    }
                }

                // Go-to-definition response – jump the cursor to the location.
                LspEvent::Definition { path, line, col } => {
                    let _ = self.editor.open_file(&path);
                    self.editor.buf_mut().cursor.set(line as usize, col as usize);
                    self.editor.buf_mut().scroll_to_cursor(24);
                    self.set_message(format!("Definition at {}:{}", line + 1, col + 1));
                }

                LspEvent::Error(e) => {
                    self.set_message(format!("LSP: {e}"));
                }
            }
        }
    }


    /// Opens a file into a new buffer tab and notifies the LSP of the open
    /// event.  If the file is already open the tab is focused instead.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to open.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or any I/O error encountered.
    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        self.editor.open_file(path)?;
        let ext: String        = self.editor.active_extension();
        let text: String       = self.editor.buf().text();
        let version: i32    = self.editor.buf().version;
        let path_owned: PathBuf = path.to_path_buf();
        if let Some(lsp) = &mut self.lsp {
            lsp.send(&ext, LspAction::DidOpen { path: path_owned, text, version });
        }
        self.set_message(format!("Opened {}", path.display()));
        Ok(())
    }

    /// Saves the active buffer to disk and notifies the LSP of the save event.
    ///
    /// If the buffer has no file path yet (i.e. it is a new unsaved buffer),
    /// this method automatically switches to [`Mode::SaveAs`] so the user
    /// can enter a destination filename instead of silently failing.
    pub fn save_file(&mut self) {
        if self.editor.buf().path.is_none() {
            self.prompt_input.clear();
            self.set_message("Save As: type a filename and press Enter");
            self.mode = Mode::SaveAs;
            return;
        }
        match self.editor.save_active() {
            Ok(()) => {
                let name: String = self.editor.buf().name.clone();
                let ext: String  = self.editor.active_extension();
                let path: Option<PathBuf> = self.editor.buf().path.clone();
                if let (Some(path), Some(lsp)) = (path, &mut self.lsp) {
                    lsp.send(&ext, LspAction::DidSave { path });
                }
                self.set_message(format!("Saved {name}"));
            }
            Err(e) => self.set_message(format!("Save error: {e}")),
        }
    }

    /// Saves the active buffer under a new path supplied by the user.
    ///
    /// Creates any missing parent directories so that a path like
    /// `notes/todo.txt` works even if `notes/` does not yet exist.
    /// After a successful write the LSP receives a `textDocument/didOpen`
    /// for the new URI so the server starts tracking the file.
    ///
    /// # Arguments
    ///
    /// * `path_str` - Raw filename string entered by the user; leading/trailing
    ///   whitespace is trimmed automatically.
    pub fn save_as_file(&mut self, path_str: &str) {
        let path_str = path_str.trim();
        if path_str.is_empty() {
            self.set_message("Save As: cancelled (empty filename)");
            return;
        }
        let path: PathBuf = PathBuf::from(path_str);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    self.set_message(format!("Save As: cannot create directory: {e}"));
                    return;
                }
            }
        }
        match self.editor.buf_mut().save_as(&path) {
            Ok(()) => {
                let name: String = self.editor.buf().name.clone();
                let ext: String = self.editor.active_extension();
                let text: String = self.editor.buf().text();
                let version: i32 = self.editor.buf().version;
                if let Some(lsp) = &mut self.lsp {
                    lsp.send(&ext, LspAction::DidOpen { path, text, version });
                }
                self.set_message(format!("Saved {name}"));
            }
            Err(e) => self.set_message(format!("Save As error: {e}")),
        }
    }

    /// Dispatches a command palette action identified by its canonical `id`.
    ///
    /// # Arguments
    ///
    /// * `id` - The command identifier string registered in `CommandPalette`.
    pub fn execute_palette_command(&mut self, id: &str) {
        match id {
            "new_file"         => { self.editor.new_buffer();                self.mode = Mode::Insert; }
            "save_file"        => { self.save_file();                        self.mode = Mode::Normal; }
            "save_as"          => { self.prompt_input.clear();
                                    self.set_message("Save As: type a filename and press Enter");
                                    self.mode = Mode::SaveAs; }
            "close_tab"        => { self.close_active_tab();                 self.mode = Mode::Normal; }
            "undo"             => { self.editor.buf_mut().undo();            self.mode = Mode::Normal; }
            "redo"             => { self.editor.buf_mut().redo();            self.mode = Mode::Normal; }
            "find"             => { self.prompt_input.clear();               self.mode = Mode::Search; }
            "find_next"        => { self.search_next();                      self.mode = Mode::Normal; }
            "find_prev"        => { self.search_prev();                      self.mode = Mode::Normal; }
            "go_to_line"       => { self.prompt_input.clear();               self.mode = Mode::GotoLine; }
            "duplicate_line"   => { self.editor.buf_mut().duplicate_line();  self.mode = Mode::Normal; }
            "delete_line"      => { self.editor.buf_mut().delete_line();     self.mode = Mode::Normal; }
            "toggle_file_tree" => { self.toggle_file_tree(); }
            "toggle_terminal"  => { self.show_terminal = !self.show_terminal; self.mode = Mode::Normal; }
            "toggle_diag"      => { self.show_diag = !self.show_diag;        self.mode = Mode::Normal; }
            "next_tab"         => { self.editor.next_tab();                  self.mode = Mode::Normal; }
            "prev_tab"         => { self.editor.prev_tab();                  self.mode = Mode::Normal; }
            "open_config"      => { self.open_config();                      self.mode = Mode::Normal; }
            "quit"             => { self.try_quit(); }
            "force_quit"       => { self.should_quit = true; }
            "go_to_def"   => { self.lsp_goto_definition(); self.mode = Mode::Normal; }
            "hover_doc"   => { self.lsp_hover();           self.mode = Mode::Normal; }
            "completions" => { self.lsp_completions();     self.mode = Mode::Normal; }
            "find_refs" | "rename_symbol" | "code_action" => {
                self.set_message("LSP: not yet implemented");
                self.mode = Mode::Normal;
            }

            _ => { self.mode = Mode::Normal; }
        }
    }

    /// Sends an LSP hover request for the symbol at the current cursor position.
    ///
    /// The response is delivered asynchronously and processed in a future
    /// call to [`App::tick`], at which point `self.hover` is populated.
    fn lsp_hover(&mut self) {
        let ext  = self.editor.active_extension();
        let path = self.editor.buf().path.clone();
        let line = self.editor.buf().cursor.line as u32;
        let col  = self.editor.buf().cursor.col  as u32;
        match (path, &mut self.lsp) {
            (Some(path), Some(lsp)) => {
                lsp.send(&ext, LspAction::Hover { path, line, col });
            }
            _ => self.set_message("LSP: no server for this file type"),
        }
    }

    /// Convenience wrapper called from the `F12` keybinding in Normal mode.
    ///
    /// Delegates to [`App::lsp_goto_definition`]; exposed as `pub` so the
    /// input crate can call it without going through the command palette.
    pub fn lsp_goto_def_key(&mut self) {
        self.lsp_goto_definition();
    }

    /// Sends an LSP go-to-definition request for the symbol under the cursor.
    ///
    /// The response is delivered asynchronously; when received it moves the
    /// cursor and opens the target file via [`App::tick`].
    fn lsp_goto_definition(&mut self) {
        let ext  = self.editor.active_extension();
        let path = self.editor.buf().path.clone();
        let line = self.editor.buf().cursor.line as u32;
        let col  = self.editor.buf().cursor.col  as u32;
        match (path, &mut self.lsp) {
            (Some(path), Some(lsp)) => {
                lsp.send(&ext, LspAction::GotoDef { path, line, col });
            }
            _ => self.set_message("LSP: no server for this file type"),
        }
    }

    /// Placeholder for LSP completion requests; not yet implemented.
    fn lsp_completions(&mut self) {
        self.set_message("LSP: completions not yet implemented");
    }

    /// Sends a full-text `textDocument/didChange` notification to the LSP for
    /// the active buffer after every edit.
    ///
    /// Full-text synchronisation is used for simplicity; incremental diffs are
    /// a future optimisation when performance becomes a concern.
    pub fn notify_lsp_change(&mut self) {
        let ext:     String          = self.editor.active_extension();
        let path:    Option<PathBuf> = self.editor.buf().path.clone();
        let text:    String          = self.editor.buf().text();
        let version: i32             = self.editor.buf().version;
        if let (Some(path), Some(lsp)) = (path, &mut self.lsp) {
            lsp.send(&ext, LspAction::DidChange { path, text, version });
        }
    }

    /// Advances to the next occurrence of the current search query, wrapping
    /// past end-of-file if necessary.
    pub fn search_next(&mut self) {
        let q = self.search.query.clone();
        if let Some((line, col)) = self.editor.buf().search_forward(&q) {
            self.editor.buf_mut().cursor.set(line, col);
            self.editor.buf_mut().scroll_to_cursor(24);
            self.search.last_match = Some((line, col));
        } else {
            self.set_message(format!("Pattern not found: {q}"));
        }
    }

    /// Moves to the previous occurrence of the current search query, wrapping
    /// past the beginning of the file if necessary.
    pub fn search_prev(&mut self) {
        let q = self.search.query.clone();
        if let Some((line, col)) = self.editor.buf().search_backward(&q) {
            self.editor.buf_mut().cursor.set(line, col);
            self.editor.buf_mut().scroll_to_cursor(24);
            self.search.last_match = Some((line, col));
        } else {
            self.set_message(format!("Pattern not found: {}", q));
        }
    }

    /// Toggles the file-tree panel and switches the editor mode accordingly.
    ///
    /// Opening the panel sets the mode to [`Mode::FileTree`] so that
    /// navigation keys are immediately active.  Closing it returns to
    /// [`Mode::Normal`].
    pub fn toggle_file_tree(&mut self) {
        if self.file_tree.is_some() {
            self.file_tree = None;
            self.mode      = Mode::Normal;
        } else {
            let root: PathBuf = self.editor.buf().path.as_ref()
                .and_then(|p: &PathBuf| p.parent())
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.file_tree = Some(FileTreeState::new(root));
            self.mode      = Mode::FileTree;
        }
    }

    /// Closes the active buffer tab and emits a `textDocument/didClose`
    /// notification to the LSP so it can release server-side resources.
    fn close_active_tab(&mut self) {
        let path: Option<PathBuf> = self.editor.buf().path.clone();
        let ext:  String          = self.editor.active_extension();
        if let (Some(path), Some(lsp)) = (path, &mut self.lsp) {
            lsp.send(&ext, LspAction::DidClose { path });
        }
        self.editor.close_active();
    }

    /// Writes the default configuration file to disk if it does not already
    /// exist, then opens it in a new buffer for editing.
    fn open_config(&mut self) {
        match Config::write_default() {
            Ok(path) => {
                let _ = self.editor.open_file(&path);
                self.set_message(format!("Config at {}", path.display()));
            }
            Err(e) => self.set_message(format!("Config error: {e}")),
        }
    }

    /// Requests a graceful application exit, refusing and prompting the user
    /// to save first if any buffer contains unsaved changes.
    pub fn try_quit(&mut self) {
        if self.editor.any_modified() {
            self.set_message("Unsaved changes! Save first or use Force Quit (Ctrl+Shift+Q)");
        } else {
            self.should_quit = true;
        }
    }
}

impl Default for App {
    fn default() -> Self { Self::new() }
}


/// Converts an LSP numeric severity code into the buffer's typed [`buffer::DiagSeverity`].
///
/// LSP severity values: `1` = Error, `2` = Warning, `3` = Information, `4` = Hint.
/// Any unrecognised value falls through to `Warning`.
///
/// # Arguments
///
/// * `severity` - Numeric severity byte from the LSP diagnostic.
///
/// # Returns
///
/// The corresponding [`buffer::DiagSeverity`] variant.
fn lsp_severity_to_buf(severity: u8) -> buffer::DiagSeverity {
    match severity {
        1 => buffer::DiagSeverity::Error,
        3 => buffer::DiagSeverity::Info,
        4 => buffer::DiagSeverity::Hint,
        _ => buffer::DiagSeverity::Warning,
    }
}
