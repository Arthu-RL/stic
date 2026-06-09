//! App crate – top-level application state and mode machine.

use anyhow::Result;
use std::path::Path;

use command_palette::CommandPalette;
use config::Config;
use editor::Editor;

/// Defines the operational context of the user interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Search,
    GotoLine,
    CommandPalette,
    FileTree,
}

/// Tracks the filesystem layout elements and active tree navigation indices.
pub struct FileTreeState {
    pub entries: Vec<(usize, String, std::path::PathBuf)>,
    pub selected: usize,
    pub root: std::path::PathBuf,
}

impl FileTreeState {
    /// Populates the directory tree structure by reading from a target directory node.
    ///
    /// # Arguments
    ///
    /// * `root` - Reference to the core workspace entry point directory path.
    ///
    /// # Returns
    ///
    /// A localized `FileTreeState` initialized with discovered child directory elements.
    pub fn from_dir(root: &Path) -> Self {
        let entries: Vec<(usize, String, std::path::PathBuf)> = walk_dir(root, 0, 3);
        Self { entries, selected: 0, root: root.to_path_buf() }
    }

    /// Increments the local structural component selection index upward.
    pub fn move_up(&mut self) {
        if self.selected > 0 { self.selected -= 1; }
    }

    /// Decrements the local structural component selection index downward.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() { self.selected += 1; }
    }

    /// Obtains the filesystem location path matching the current focused index.
    ///
    /// # Returns
    ///
    /// An `Option` reference wrapping the underlying platform path.
    pub fn selected_path(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(|(_, _, p)| p.as_path())
    }

    /// Forces a manual synchronization pass over the workspace root directory tracking list.
    pub fn refresh(&mut self) {
        self.entries = walk_dir(&self.root, 0, 3);
    }
}

/// Recursively discovers filesystem structures down to fixed navigation boundary checkpoints.
///
/// # Arguments
///
/// * `dir` - Current path context layer target pointer reference.
/// * `depth` - Traversal offset tracking metrics level.
/// * `max_depth` - Total safe recursion layer boundaries cap.
///
/// # Returns
///
/// A flattened sequence containing depth counters, customized presentation names, and system paths.
fn walk_dir(dir: &Path, depth: usize, max_depth: usize) -> Vec<(usize, String, std::path::PathBuf)> {
    if depth > max_depth { return vec![]; }
    let mut out: Vec<(usize, String, std::path::PathBuf)> = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return out; };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e: &std::fs::DirEntry| {
        let is_file: bool = e.file_type().map(|t| t.is_file()).unwrap_or(true);
        (is_file as u8, e.file_name())
    });
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let prefix = if path.is_dir() { format!("{}", name) } else { format!("  {}", name) };
        out.push((depth, prefix, path.clone()));
        if path.is_dir() {
            out.extend(walk_dir(&path, depth + 1, max_depth));
        }
    }
    out
}

/// Retains active search criteria buffers along with coordinates matching the most recent search.
pub struct SearchState {
    pub query:   String,
    pub last_match: Option<(usize, usize)>,
}

impl SearchState {
    /// Constructs an empty search configuration state tracker.
    ///
    /// # Returns
    ///
    /// A blank initialized `SearchState` instance.
    pub fn new() -> Self { Self { query: String::new(), last_match: None } }
}

/// The main orchestrator managing editor models, layouts, peripheral interfaces, configurations, and panels.
pub struct App {
    pub editor: Editor,
    pub command_palette: CommandPalette,
    pub mode: Mode,
    pub config: Config,
    pub should_quit: bool,
    pub message: Option<String>,
    message_ticks: u8,
    pub prompt_input: String,
    pub search: SearchState,
    pub file_tree: Option<FileTreeState>,
    pub show_terminal: bool,
    pub show_diag: bool,
}

impl App {
    /// Initializes application states, configurations, and core layouts.
    ///
    /// # Returns
    ///
    /// An operational, top-level `App` engine wrapper block.
    pub fn new() -> Self {
        let config: Config = Config::load();
        let editor: Editor = Editor::new(config.clone());
        let show_ft: bool   = config.ui.show_file_tree;
        let show_term: bool = config.ui.show_terminal;
        let show_diag: bool = config.ui.show_diagnostics;

        let file_tree: Option<FileTreeState> = if show_ft {
            std::env::current_dir().ok().map(|d| FileTreeState::from_dir(&d))
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
        }
    }

    /// Dispatches short-lived alert descriptions across terminal screen notification systems.
    ///
    /// # Arguments
    ///
    /// * `msg` - The targeted string text data sequence to expose.
    pub fn set_message<S: Into<String>>(&mut self, msg: S) {
        self.message       = Some(msg.into());
        self.message_ticks = 60;
    }

    /// Drives the background life cycles tracking systems to clear old transient notifications.
    pub fn tick(&mut self) {
        if let Some(t) = self.message_ticks.checked_sub(1) {
            self.message_ticks = t;
            if t == 0 { self.message = None; }
        }
    }

    /// Allocates text space buffers populated with files loaded from specified references.
    ///
    /// # Arguments
    ///
    /// * `path` - Target system filesystem storage pointer.
    ///
    /// # Returns
    ///
    /// A blank `Result` variant documenting execution updates.
    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        self.editor.open_file(path)?;
        self.set_message(format!("Opened {}", path.display()));
        Ok(())
    }

    /// Orders immediate operational write updates saving text spaces onto target drives.
    pub fn save_file(&mut self) {
        match self.editor.save_active() {
            Ok(()) => {
                let name = self.editor.buf().name.clone();
                self.set_message(format!("Saved {}", name));
            }
            Err(e) => self.set_message(format!("Save error: {e}")),
        }
    }

    /// Translates selection updates from command components into concrete internal mutations.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique stable identifier tracking the targeted action.
    pub fn execute_palette_command(&mut self, id: &str) {
        match id {
            "new_file"         => { self.editor.new_buffer(); self.mode = Mode::Insert; }
            "save_file"        => { self.save_file(); self.mode = Mode::Normal; }
            "close_tab"        => { self.editor.close_active(); self.mode = Mode::Normal; }
            "undo"             => { self.editor.buf_mut().undo(); self.mode = Mode::Normal; }
            "redo"             => { self.editor.buf_mut().redo(); self.mode = Mode::Normal; }
            "find"             => { self.prompt_input.clear(); self.mode = Mode::Search; }
            "find_next"        => { self.search_next(); self.mode = Mode::Normal; }
            "find_prev"        => { self.search_prev(); self.mode = Mode::Normal; }
            "go_to_line"       => { self.prompt_input.clear(); self.mode = Mode::GotoLine; }
            "duplicate_line"   => { self.editor.buf_mut().duplicate_line(); self.mode = Mode::Normal; }
            "toggle_file_tree" => { self.toggle_file_tree(); self.mode = Mode::Normal; }
            "toggle_terminal"  => { self.show_terminal = !self.show_terminal; self.mode = Mode::Normal; }
            "toggle_diag"      => { self.show_diag = !self.show_diag; self.mode = Mode::Normal; }
            "next_tab"         => { self.editor.next_tab(); self.mode = Mode::Normal; }
            "prev_tab"         => { self.editor.prev_tab(); self.mode = Mode::Normal; }
            "open_config"      => { self.open_config(); self.mode = Mode::Normal; }
            "quit"             => { self.try_quit(); }
            "force_quit"       => { self.should_quit = true; }
            "go_to_def" | "find_refs" | "hover_doc" | "rename_symbol"
            | "code_action" | "symbol_search" => {
                self.set_message("LSP: not yet connected. See docs/lsp.md");
                self.mode = Mode::Normal;
            }
            _ => { self.mode = Mode::Normal; }
        }
    }

    /// Evaluates forward lookup matches moving tracking focuses to new pattern locations.
    pub fn search_next(&mut self) {
        let q = self.search.query.clone();
        if let Some((line, col)) = self.editor.buf().search_forward(&q) {
            self.editor.buf_mut().cursor.set(line, col);
            let h = 24;
            self.editor.buf_mut().scroll_to_cursor(h);
            self.search.last_match = Some((line, col));
        } else {
            self.set_message(format!("Pattern not found: {}", q));
        }
    }

    /// Evaluates backward lookup matches moving tracking focuses to new pattern locations.
    pub fn search_prev(&mut self) {
        let q = self.search.query.clone();
        if let Some((line, col)) = self.editor.buf().search_backward(&q) {
            self.editor.buf_mut().cursor.set(line, col);
            let h = 24;
            self.editor.buf_mut().scroll_to_cursor(h);
            self.search.last_match = Some((line, col));
        } else {
            self.set_message(format!("Pattern not found: {}", q));
        }
    }

    /// Toggles the existence of the sidebar layout components folder tree panel.
    pub fn toggle_file_tree(&mut self) {
        if self.file_tree.is_some() {
            self.file_tree = None;
        } else {
            let root = self.editor.buf().path.as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            self.file_tree = Some(FileTreeState::from_dir(&root));
        }
    }

    /// Accesses system files to launch editor tracking controls on top-level configurations.
    fn open_config(&mut self) {
        match Config::write_default() {
            Ok(path) => {
                let _ = self.editor.open_file(&path);
                self.set_message(format!("Config at {}", path.display()));
            }
            Err(e) => self.set_message(format!("Config error: {e}")),
        }
    }

    /// Evaluates buffer tracking parameters to determine if a safe execution shutdown is allowed.
    pub fn try_quit(&mut self) {
        if self.editor.any_modified() {
            self.set_message("Unsaved changes! Save first or use Force Quit (Ctrl+Shift+Q)");
        } else {
            self.should_quit = true;
        }
    }
}

impl Default for App {
    /// Default initialization wrapper referencing standard instance creation routines.
    fn default() -> Self { Self::new() }
}