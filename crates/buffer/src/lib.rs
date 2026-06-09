//! Buffer crate – rope-backed text buffer with undo/redo and search.
//!
//! # Design notes
//! * Text is stored in a [`ropey::Rope`] giving O(log n) insertions/deletions
//!   even on multi-MB files.
//! * Cursor is tracked in (line, col) coordinates where col is a *char* index
//!   (not a byte index), so Unicode code-points are handled correctly.
//! * A separate `desired_col` is preserved across vertical motion so the
//!   cursor "sticks" to its column when passing through shorter lines.
//! * Undo/redo is a pair of `Vec<EditRecord>` stacks with a configurable
//!   depth (default 2 000 records).

use anyhow::Result;
use ropey::Rope;
use std::path::{Path, PathBuf};

/// Tracks the active text terminal positioning inside a buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub line:        usize,
    pub col:         usize,
    pub desired_col: usize,
}

impl Cursor {
    /// Instantiates a new cursor at position (0, 0).
    ///
    /// # Returns
    ///
    /// A default initialized `Cursor` object.
    pub fn new() -> Self {
        Self::default()
    }

    /// Explicitly targets specific coordinate assignments.
    ///
    /// # Arguments
    ///
    /// * `line` - The zero-indexed text line row.
    /// * `col` - The character offset inside the targeted row.
    pub fn set(&mut self, line: usize, col: usize) {
        self.line        = line;
        self.col         = col;
        self.desired_col = col;
    }

    /// Deconstructs the core cursor coordinates.
    ///
    /// # Returns
    ///
    /// A nested tuple structure tracking `(line, col)`.
    #[inline]
    pub fn as_tuple(self) -> (usize, usize) {
        (self.line, self.col)
    }
}

/// A specific change record node storing insertion and removal text data states.
#[derive(Debug, Clone)]
pub struct EditRecord {
    pub char_idx:      usize,
    pub inserted:      String,
    pub deleted:       String,
    pub cursor_before: (usize, usize),
    pub cursor_after:  (usize, usize),
}

/// A transaction management container handling execution backtracks and restorations.
pub struct UndoStack {
    past:   Vec<EditRecord>,
    future: Vec<EditRecord>,
    limit:  usize,
}

impl UndoStack {
    /// Creates a bounded action tracking history module.
    ///
    /// # Arguments
    ///
    /// * `limit` - Max total amount of distinct operational steps saved.
    ///
    /// # Returns
    ///
    /// A blank `UndoStack` tracker.
    pub fn new(limit: usize) -> Self {
        Self { past: Vec::new(), future: Vec::new(), limit }
    }

    /// Commits a historical transformation node down to the stack trace.
    ///
    /// # Arguments
    ///
    /// * `record` - The finalized structure data outlining modifications.
    pub fn push(&mut self, record: EditRecord) {
        self.future.clear();
        self.past.push(record);
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
    }

    /// Pops the last item from the historical track to revert state.
    ///
    /// # Returns
    ///
    /// An `Option` wrapper containing the extracted `EditRecord`.
    pub fn undo(&mut self) -> Option<EditRecord> {
        let r = self.past.pop()?;
        self.future.push(r.clone());
        Some(r)
    }

    /// Pops the last reverted item from the future trace stack to re-apply state.
    ///
    /// # Returns
    ///
    /// An `Option` wrapper containing the extracted `EditRecord`.
    pub fn redo(&mut self) -> Option<EditRecord> {
        let r = self.future.pop()?;
        self.past.push(r.clone());
        Some(r)
    }

    /// Evaluates if backward chronological restoration routes exist.
    ///
    /// # Returns
    ///
    /// `true` if historical elements exist; otherwise, `false`.
    pub fn can_undo(&self) -> bool { !self.past.is_empty()   }

    /// Evaluates if forward chronological step components exist.
    ///
    /// # Returns
    ///
    /// `true` if elements are available inside the forward track; otherwise, `false`.
    pub fn can_redo(&self) -> bool { !self.future.is_empty() }
}

/// Denotes warning classification thresholds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagSeverity { Error, Warning, Info, Hint }

/// A diagnostics advisory error reporting structure.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub line:     usize,
    pub col:      usize,
    pub message:  String,
    pub severity: DiagSeverity,
}

/// An interactive open text document workspace backing content tracks, tracking pointers,
/// local path attachments, and diagnostics.
pub struct Buffer {
    pub rope:        Rope,
    pub cursor:      Cursor,
    pub path:        Option<PathBuf>,
    pub name:        String,
    pub modified:    bool,
    pub scroll_top:  usize,
    pub scroll_left: usize,
    pub diagnostics: Vec<Diagnostic>,
    undo:            UndoStack,
}

impl Buffer {
    /// Generates a blank, unsaved text area window environment.
    ///
    /// # Returns
    ///
    /// An empty, initialized default configuration variant `Buffer`.
    pub fn new_empty() -> Self {
        Self {
            rope:        Rope::new(),
            cursor:      Cursor::new(),
            path:        None,
            name:        "[New]".into(),
            modified:    false,
            scroll_top:  0,
            scroll_left: 0,
            diagnostics: Vec::new(),
            undo:        UndoStack::new(2_000),
        }
    }

    /// Extracts textual information out from a storage path reference.
    ///
    /// # Arguments
    ///
    /// * `path` - Reference to the target operating system storage resource path.
    ///
    /// # Returns
    ///
    /// A descriptive `Result` containing the hydrated text workspace structure.
    pub fn from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Ok(Self {
            rope:        Rope::from_str(&content),
            cursor:      Cursor::new(),
            path:        Some(path.to_path_buf()),
            name,
            modified:    false,
            scroll_top:  0,
            scroll_left: 0,
            diagnostics: Vec::new(),
            undo:        UndoStack::new(2_000),
        })
    }

    /// Synchronizes internal content updates directly onto local storage fields.
    ///
    /// # Returns
    ///
    /// A blank `Result` type flagging completion or underlying system I/O errors.
    pub fn save(&mut self) -> Result<()> {
        let path = self.path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("no file path – use Save As"))?;
        let text = self.rope.to_string();
        std::fs::write(path, text)?;
        self.modified = false;
        Ok(())
    }

    /// Sets the target path parameters before performing write procedures.
    ///
    /// # Arguments
    ///
    /// * `path` - The new path reference address.
    ///
    /// # Returns
    ///
    /// A blank `Result` wrapper signifying completion status.
    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        self.path = Some(path.to_path_buf());
        self.name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.save()
    }

    /// Adjusts line position offsets higher.
    ///
    /// # Arguments
    ///
    /// * `n` - Row transition iteration amount.
    pub fn move_up(&mut self, n: usize) {
        self.cursor.line = self.cursor.line.saturating_sub(n);
        self.clamp_col_to_desired();
    }

    /// Adjusts line position offsets downward.
    ///
    /// # Arguments
    ///
    /// * `n` - Row transition iteration amount.
    pub fn move_down(&mut self, n: usize) {
        let max = self.rope.len_lines().saturating_sub(1);
        self.cursor.line = (self.cursor.line + n).min(max);
        self.clamp_col_to_desired();
    }

    /// Displaces the terminal tracker single cursor step units to the left.
    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col         -= 1;
            self.cursor.desired_col  = self.cursor.col;
        } else if self.cursor.line > 0 {
            self.cursor.line        -= 1;
            self.cursor.col          = self.line_len(self.cursor.line);
            self.cursor.desired_col  = self.cursor.col;
        }
    }

    /// Displaces the terminal tracker single cursor step units to the right.
    pub fn move_right(&mut self) {
        let len = self.line_len(self.cursor.line);
        if self.cursor.col < len {
            self.cursor.col         += 1;
            self.cursor.desired_col  = self.cursor.col;
        } else if self.cursor.line + 1 < self.rope.len_lines() {
            self.cursor.line        += 1;
            self.cursor.col          = 0;
            self.cursor.desired_col  = 0;
        }
    }

    /// Directs tracker offsets over home line text boundaries.
    pub fn move_line_start(&mut self) {
        let first_non_ws = self.get_line(self.cursor.line)
            .chars()
            .position(|c| !c.is_whitespace())
            .unwrap_or(0);
        if self.cursor.col == first_non_ws {
            self.cursor.set(self.cursor.line, 0);
        } else {
            self.cursor.set(self.cursor.line, first_non_ws);
        }
    }

    /// Directs tracker offsets directly to the end boundary of the line.
    pub fn move_line_end(&mut self) {
        let col = self.line_len(self.cursor.line);
        self.cursor.set(self.cursor.line, col);
    }

    /// Scales the cursor back upward across visible page frames.
    ///
    /// # Arguments
    ///
    /// * `page_height` - Viewport line window length metric.
    pub fn move_page_up(&mut self, page_height: usize) {
        self.cursor.line = self.cursor.line.saturating_sub(page_height);
        if self.scroll_top > page_height {
            self.scroll_top -= page_height;
        } else {
            self.scroll_top = 0;
        }
        self.clamp_col_to_desired();
    }

    /// Scales the cursor down across visible page frames.
    ///
    /// # Arguments
    ///
    /// * `page_height` - Viewport line window length metric.
    pub fn move_page_down(&mut self, page_height: usize) {
        let max = self.rope.len_lines().saturating_sub(1);
        self.cursor.line = (self.cursor.line + page_height).min(max);
        self.scroll_top  = (self.scroll_top + page_height).min(max);
        self.clamp_col_to_desired();
    }

    /// Steps tracking focus point increments over forward text clusters.
    pub fn move_word_forward(&mut self) {
        let total = self.rope.len_chars();
        let mut idx = self.char_idx();
        while idx < total && self.char_at(idx).map(|c| c.is_whitespace()).unwrap_or(true) {
            idx += 1;
        }
        while idx < total && !self.char_at(idx).map(|c| c.is_whitespace()).unwrap_or(true) {
            idx += 1;
        }
        self.set_cursor_by_char(idx);
    }

    /// Steps tracking focus point decrements over backward text clusters.
    pub fn move_word_backward(&mut self) {
        let mut idx = self.char_idx();
        if idx == 0 { return; }
        idx -= 1;
        while idx > 0 && self.char_at(idx).map(|c| c.is_whitespace()).unwrap_or(true) {
            idx -= 1;
        }
        while idx > 0 && !self.char_at(idx - 1).map(|c| c.is_whitespace()).unwrap_or(true) {
            idx -= 1;
        }
        self.set_cursor_by_char(idx);
    }

    /// Jumps directly onto specific row elements updating window view alignment.
    ///
    /// # Arguments
    ///
    /// * `line` - Target row index position.
    /// * `visible_height` - Active terminal sizing dimension height tracking value.
    pub fn goto_line(&mut self, line: usize, visible_height: usize) {
        let max: usize = self.rope.len_lines().saturating_sub(1);
        self.cursor.line = line.min(max);
        self.clamp_col_to_desired();
        self.scroll_to_cursor(visible_height);
    }

    /// Warps current tracking assignments back to absolute zero file starts.
    pub fn goto_file_start(&mut self) {
        self.cursor.set(0, 0);
        self.scroll_top  = 0;
        self.scroll_left = 0;
    }

    /// Warps current tracking assignments down to total absolute file ends.
    pub fn goto_file_end(&mut self) {
        let last = self.rope.len_lines().saturating_sub(1);
        self.cursor.set(last, 0);
    }

    /// Appends single unicode scalar values into the rope mapping structures.
    ///
    /// # Arguments
    ///
    /// * `ch` - The designated insertion character item.
    pub fn insert_char(&mut self, ch: char) {
        let idx: usize    = self.char_idx();
        let before: (usize, usize) = self.cursor.as_tuple();
        self.rope.insert_char(idx, ch);
        self.modified = true;
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.col   = 0;
        } else {
            self.cursor.col   += 1;
        }
        self.cursor.desired_col = self.cursor.col;
        let after: (usize, usize) = self.cursor.as_tuple();
        self.undo.push(EditRecord {
            char_idx: idx,
            inserted: ch.to_string(),
            deleted:  String::new(),
            cursor_before: before,
            cursor_after:  after,
        });
    }

    /// Commits full text fragments slice segments down inside data streams.
    ///
    /// # Arguments
    ///
    /// * `s` - The slice block string sequence.
    pub fn insert_str(&mut self, s: &str) {
        for ch in s.chars() { self.insert_char(ch); }
    }

    /// Commits structured space tab indentation steps forward.
    ///
    /// # Arguments
    ///
    /// * `tab_size` - Size block multiplier value.
    /// * `use_spaces` - Flag forcing spacer pads over actual literal character code maps.
    pub fn insert_tab(&mut self, tab_size: usize, use_spaces: bool) {
        if use_spaces {
            let pad = tab_size - (self.cursor.col % tab_size);
            self.insert_str(&" ".repeat(pad));
        } else {
            self.insert_char('\t');
        }
    }

    /// Triggers character extractions directly preceding active tracker coordinates.
    pub fn delete_backward(&mut self) {
        if self.cursor.col == 0 && self.cursor.line == 0 { return; }
        let before  = self.cursor.as_tuple();
        self.move_left();
        let idx     = self.char_idx();
        if idx >= self.rope.len_chars() { return; }
        let deleted = self.rope.char(idx).to_string();
        self.rope.remove(idx..idx + 1);
        self.modified = true;
        let after   = self.cursor.as_tuple();
        self.undo.push(EditRecord {
            char_idx:      idx,
            inserted:      String::new(),
            deleted,
            cursor_before: before,
            cursor_after:  after,
        });
    }

    /// Triggers active targeted cursor element character drops.
    pub fn delete_forward(&mut self) {
        let idx = self.char_idx();
        if idx >= self.rope.len_chars() { return; }
        let before  = self.cursor.as_tuple();
        let deleted = self.rope.char(idx).to_string();
        self.rope.remove(idx..idx + 1);
        self.modified = true;
        self.clamp_col_to_desired();
        self.undo.push(EditRecord {
            char_idx:      idx,
            inserted:      String::new(),
            deleted,
            cursor_before: before,
            cursor_after:  before,
        });
    }

    /// Clears string content elements running up from cursors out toward line endings.
    pub fn delete_to_eol(&mut self) {
        let start    = self.char_idx();
        let line_end = self.rope.line_to_char(self.cursor.line)
            + self.rope.line(self.cursor.line).len_chars().saturating_sub(1);
        if start >= line_end { return; }
        let deleted: String = self.rope.chars_at(start).take(line_end - start).collect();
        self.rope.remove(start..line_end);
        self.modified = true;
        let pos = self.cursor.as_tuple();
        self.undo.push(EditRecord {
            char_idx: start, inserted: String::new(), deleted,
            cursor_before: pos, cursor_after: pos,
        });
    }

    /// Copies and reprints the active string line elements exactly underneath.
    pub fn duplicate_line(&mut self) {
        let line_str = self.get_line(self.cursor.line);
        let eol_idx  = self.rope.line_to_char(self.cursor.line)
            + self.rope.line(self.cursor.line).len_chars();
        let insert_str = if line_str.ends_with('\n') {
            line_str.clone()
        } else {
            format!("\n{}", line_str)
        };
        self.rope.insert(eol_idx, &insert_str);
        self.cursor.line  += 1;
        self.modified      = true;
    }

    /// Pulls out previous adjustment records to step back tree buffer positions.
    pub fn undo(&mut self) {
        if let Some(r) = self.undo.undo() {
            if !r.inserted.is_empty() {
                let end = (r.char_idx + r.inserted.chars().count()).min(self.rope.len_chars());
                if r.char_idx < end { self.rope.remove(r.char_idx..end); }
            }
            if !r.deleted.is_empty() {
                self.rope.insert(r.char_idx, &r.deleted);
            }
            self.cursor.set(r.cursor_before.0, r.cursor_before.1);
            self.modified = true;
        }
    }

    /// Re-evaluates state changes to advance forward tracking buffer timelines.
    pub fn redo(&mut self) {
        if let Some(r) = self.undo.redo() {
            if !r.deleted.is_empty() {
                let end = (r.char_idx + r.deleted.chars().count()).min(self.rope.len_chars());
                if r.char_idx < end { self.rope.remove(r.char_idx..end); }
            }
            if !r.inserted.is_empty() {
                self.rope.insert(r.char_idx, &r.inserted);
            }
            self.cursor.set(r.cursor_after.0, r.cursor_after.1);
            self.modified = true;
        }
    }

    /// Evaluates if historical rollback elements are available.
    ///
    /// # Returns
    ///
    /// `true` if undo items are ready; otherwise, `false`.
    pub fn can_undo(&self) -> bool { self.undo.can_undo() }

    /// Evaluates if forward trace components are available.
    ///
    /// # Returns
    ///
    /// `true` if forward items are ready; otherwise, `false`.
    pub fn can_redo(&self) -> bool { self.undo.can_redo() }

    /// Performs text matches searching forward, wrapping past EOF.
    ///
    /// # Arguments
    ///
    /// * `query` - Targeted literal sub-match text phrase.
    ///
    /// # Returns
    ///
    /// An `Option` tuple holding sequence pairs for matching rows and character columns.
    pub fn search_forward(&self, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() { return None; }
        let content = self.rope.to_string().to_lowercase();
        let q       = query.to_lowercase();
        let start   = (self.char_idx() + 1).min(self.rope.len_chars());
        let start_b = self.rope.char_to_byte(start);

        let hit = content[start_b..].find(&q)
            .map(|o| start_b + o)
            .or_else(|| content.find(&q));

        hit.map(|byte_idx| {
            let char_idx = self.rope.byte_to_char(byte_idx);
            let line     = self.rope.char_to_line(char_idx);
            let col      = char_idx - self.rope.line_to_char(line);
            (line, col)
        })
    }

    /// Performs text matches searching backward, wrapping past top boundary files.
    ///
    /// # Arguments
    ///
    /// * `query` - Targeted literal sub-match text phrase.
    ///
    /// # Returns
    ///
    /// An `Option` tuple holding sequence pairs for matching rows and character columns.
    pub fn search_backward(&self, query: &str) -> Option<(usize, usize)> {
        if query.is_empty() { return None; }
        let content = self.rope.to_string().to_lowercase();
        let q       = query.to_lowercase();
        let end_b   = self.rope.char_to_byte(self.char_idx());

        let hit = content[..end_b].rfind(&q)
            .or_else(|| content.rfind(&q));

        hit.map(|byte_idx| {
            let char_idx = self.rope.byte_to_char(byte_idx);
            let line     = self.rope.char_to_line(char_idx);
            let col      = char_idx - self.rope.line_to_char(line);
            (line, col)
        })
    }

    /// Recalculates screen viewport positioning bounds relative to text tracker modifications.
    ///
    /// # Arguments
    ///
    /// * `visible_height` - Viewport terminal height dimensions constraint metric.
    pub fn scroll_to_cursor(&mut self, visible_height: usize) {
        let off = 5_usize;
        if self.cursor.line < self.scroll_top.saturating_add(off) {
            self.scroll_top = self.cursor.line.saturating_sub(off);
        }
        if visible_height > 0 {
            let bottom = self.scroll_top + visible_height.saturating_sub(off + 1);
            if self.cursor.line > bottom {
                self.scroll_top = self.cursor.line + off + 1 - visible_height;
            }
        }
    }

    /// Calculates row lengths without terminal newline trailing elements.
    ///
    /// # Arguments
    ///
    /// * `line` - Targeted line slice row index value.
    ///
    /// # Returns
    ///
    /// Character count layout length parameters.
    #[inline]
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() { return 0; }
        let s   = self.rope.line(line);
        let len = s.len_chars();
        if len > 0 && s.char(len - 1) == '\n' { len - 1 } else { len }
    }

    /// Extracts total flattened indices from character columns.
    ///
    /// # Returns
    ///
    /// The absolute single sequence numerical pointer coordinates.
    #[inline]
    pub fn char_idx(&self) -> usize {
        if self.rope.len_lines() == 0 { return 0; }
        let line_start = self.rope.line_to_char(self.cursor.line);
        let max_col    = self.line_len(self.cursor.line);
        line_start + self.cursor.col.min(max_col)
    }

    /// Pulls out singular char references relative to specified index pointers.
    ///
    /// # Arguments
    ///
    /// * `idx` - Single target reference scale coordinates.
    ///
    /// # Returns
    ///
    /// Optional match tracking wrappers containing extracted code points.
    #[inline]
    fn char_at(&self, idx: usize) -> Option<char> {
        (idx < self.rope.len_chars()).then(|| self.rope.char(idx))
    }

    /// Coordinates column splits down from absolute tracking index pointers.
    ///
    /// # Arguments
    ///
    /// * `idx` - Single target file position pointer.
    fn set_cursor_by_char(&mut self, idx: usize) {
        let idx  = idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(idx);
        let col  = idx - self.rope.line_to_char(line);
        self.cursor.set(line, col);
    }

    /// Enforces tracking safety boundaries constraint fields on text column lengths.
    fn clamp_col_to_desired(&mut self) {
        let max        = self.line_len(self.cursor.line);
        self.cursor.col = self.cursor.desired_col.min(max);
    }

    /// Collects total rope line item counts.
    ///
    /// # Returns
    ///
    /// Numerical tracking range sums.
    #[inline]
    pub fn line_count(&self) -> usize { self.rope.len_lines() }

    /// Captures fully copies of text tracking sequences.
    ///
    /// # Arguments
    ///
    /// * `n` - Selected row boundary sequence.
    ///
    /// # Returns
    ///
    /// Owned instance copy text wrapper elements.
    pub fn get_line(&self, n: usize) -> String {
        if n >= self.rope.len_lines() { return String::new(); }
        self.rope.line(n).to_string()
    }

    /// Resolves file format classification groupings extensions.
    ///
    /// # Returns
    ///
    /// Extracted slice format extension types or default fallback empty values.
    pub fn extension(&self) -> String {
        self.path.as_ref()
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Segregates diagnostics tracking data down to target line blocks.
    ///
    /// # Arguments
    ///
    /// * `line` - Target index row query element.
    ///
    /// # Returns
    ///
    /// Sorted context sequence vectors.
    pub fn diags_on_line(&self, line: usize) -> Vec<&Diagnostic> {
        let mut v: Vec<_> = self.diagnostics.iter().filter(|d| d.line == line).collect();
        v.sort_by_key(|d| d.col);
        v
    }
}