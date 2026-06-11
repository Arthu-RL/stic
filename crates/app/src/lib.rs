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


/// Persistent navigation state for the file-tree panel.
pub struct FileTreeState {
    /// Flat list of `(indent_depth, display_label, absolute_path)` entries.
    pub entries:  Vec<(usize, String, PathBuf)>,
    /// Zero-based index of the currently highlighted row.
    pub selected: usize,
    root:         PathBuf,
}

impl FileTreeState {
    /// Builds a new `FileTreeState` rooted at `root`, walking up to three
    /// directory levels deep.
    ///
    /// # Arguments
    ///
    /// * `root` - Directory to use as the tree root.
    ///
    /// # Returns
    ///
    /// A fully populated `FileTreeState`.
    pub fn from_dir(root: &Path) -> Self {
        let entries = walk_dir(root, 0, 3);
        Self { entries, selected: 0, root: root.to_path_buf() }
    }

    /// Moves the selection highlight up one row, clamping at the top.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Moves the selection highlight down one row, clamping at the bottom.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    /// Returns the path of the currently highlighted entry, if any.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the selected `Path`.
    pub fn selected_path(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(|(_, _, p)| p.as_path())
    }

    /// Re-scans the root directory to reflect any filesystem changes.
    pub fn refresh(&mut self) {
        let root = self.root.clone();
        self.entries = walk_dir(&root, 0, 3);
    }

    /// Selects the entry at the given row index, clamping to valid bounds.
    ///
    /// # Arguments
    ///
    /// * `row` - Zero-based row index to select.
    pub fn select_row(&mut self, row: usize) {
        if !self.entries.is_empty() {
            self.selected = row.min(self.entries.len() - 1);
        }
    }
}

/// Recursively enumerates a directory up to `max_depth` levels deep.
///
/// Entries are sorted with directories first, then files, both in
/// case-insensitive alphabetical order.
///
/// # Arguments
///
/// * `dir`       - Root directory to enumerate.
/// * `depth`     - Current recursion depth (pass `0` on the initial call).
/// * `max_depth` - Maximum recursion depth allowed.
///
/// # Returns
///
/// A flat list of `(indent_depth, display_label, absolute_path)` tuples.
fn walk_dir(dir: &Path, depth: usize, max_depth: usize) -> Vec<(usize, String, PathBuf)> {
    if depth > max_depth { return vec![]; }
    let Ok(read) = std::fs::read_dir(dir) else { return vec![]; };

    let mut entries: Vec<std::fs::DirEntry> = read.flatten().collect();
    // Directories first, then files, both alphabetical.
    entries.sort_unstable_by_key(|e: &std::fs::DirEntry| {
        let is_file: bool = e.file_type().map(|t: std::fs::FileType| t.is_file()).unwrap_or(true);
        (is_file as u8, e.file_name())
    });

    let mut out: Vec<(usize, String, PathBuf)> = Vec::new();
    for e in entries {
        let path: PathBuf  = e.path();
        let name: String  = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') { continue; }
        let label: String = if path.is_dir() { name } else { format!("  {name}") };
        out.push((depth, label, path.clone()));
        if path.is_dir() {
            out.extend(walk_dir(&path, depth + 1, max_depth));
        }
    }
    out
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
            std::env::current_dir().ok().map(|d: PathBuf| FileTreeState::from_dir(&d))
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

    /// Advances one event-loop tick: drains pending LSP events and ages out
    /// the transient status message.
    pub fn tick(&mut self) {
        self.drain_lsp_events();

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
            "toggle_file_tree" => { self.toggle_file_tree();                 self.mode = Mode::Normal; }
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

    /// Toggles the file-tree panel, rebuilding it rooted at the directory of
    /// the active file (or the current working directory as a fallback).
    pub fn toggle_file_tree(&mut self) {
        if self.file_tree.is_some() {
            self.file_tree = None;
        } else {
            let root: PathBuf = self.editor.buf().path.as_ref()
                .and_then(|p: &PathBuf| p.parent())
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.file_tree = Some(FileTreeState::from_dir(&root));
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
