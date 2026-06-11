//! Editor crate – manages open buffers and syntax highlighting.


use anyhow::Result;
use std::path::Path;

use buffer::Buffer;
use config::Config;

pub mod highlight;
pub use highlight::{HighlightedSpan, Highlighter};


/// Owns all open buffers and handles management of active workspaces and the syntax highlighter.
pub struct Editor {
    pub buffers:     Vec<Buffer>,
    pub active:      usize,
    pub highlighter: Highlighter,
    pub config:      Config,
}


impl Editor {
    /// Creates a new `Editor` coordinator initialized with a single empty buffer instance.
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration containing interface and style settings.
    ///
    /// # Returns
    ///
    /// A default initialized `Editor` core structure.
    pub fn new(config: Config) -> Self {
        let highlighter: Highlighter = Highlighter::new(&config.ui.theme);
        let mut ed: Editor = Self {
            buffers:     Vec::new(),
            active:      0,
            highlighter,
            config,
        };
        ed.buffers.push(Buffer::new_empty());
        ed
    }

    /// Loads a file into a buffer or focuses its existing tab if already present in memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Reference path targeting the destination workspace file.
    ///
    /// # Returns
    ///
    /// A blank `Result` confirming successful lookup or filesystem retrieval operations.
    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        if let Some(idx) = self.buffers.iter().position(|b: &Buffer| b.path.as_deref() == Some(path)) {
            self.active = idx;
            return Ok(());
        }
        let buf: Buffer = Buffer::from_path(path)?;
        self.buffers.push(buf);
        self.active = self.buffers.len() - 1;
        Ok(())
    }

    /// Appends a blank untitled text space environment tab into the workspace stream.
    pub fn new_buffer(&mut self) {
        self.buffers.push(Buffer::new_empty());
        self.active = self.buffers.len() - 1;
    }

    /// Disposes of the active document workspace tab while keeping a defensive buffer invariant.
    pub fn close_active(&mut self) {
        if self.buffers.len() > 1 {
            self.buffers.remove(self.active);
            if self.active >= self.buffers.len() {
                self.active = self.buffers.len() - 1;
            }
        } else {
            self.buffers[0] = Buffer::new_empty();
        }
    }

    /// Cycle tracking focus references sequentially forward to the next index.
    pub fn next_tab(&mut self) {
        if !self.buffers.is_empty() {
            self.active = (self.active + 1) % self.buffers.len();
        }
    }

    /// Cycle tracking focus references sequentially backward to the previous index.
    pub fn prev_tab(&mut self) {
        if !self.buffers.is_empty() {
            self.active = self.active.checked_sub(1).unwrap_or(self.buffers.len() - 1);
        }
    }

    /// Sets focus explicitly onto a distinct array placement layout track position.
    ///
    /// # Arguments
    ///
    /// * `idx` - Numerical entry offset value targeting preferred views.
    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.buffers.len() {
            self.active = idx;
        }
    }

    /// Read-only accessor retrieval utility shortcut targeting the active context layout workspace.
    ///
    /// # Returns
    ///
    /// An immutable reference to the selected document `Buffer`.
    #[inline]
    pub fn buf(&self) -> &Buffer { &self.buffers[self.active] }

    /// Read-write accessor retrieval utility shortcut targeting the active context layout workspace.
    ///
    /// # Returns
    ///
    /// A mutable reference to the selected document `Buffer`.
    #[inline]
    pub fn buf_mut(&mut self) -> &mut Buffer { &mut self.buffers[self.active] }

    /// Triggers persistence synchronization operations over focused files.
    ///
    /// # Returns
    ///
    /// A completion status `Result` capturing file tracking system metrics.
    pub fn save_active(&mut self) -> Result<()> { self.buf_mut().save() }

    /// Determines the extension label of the current workspace file.
    ///
    /// # Returns
    ///
    /// The string name type extension format associated with active tracks.
    pub fn active_extension(&self) -> String { self.buf().extension() }

    /// Scans internal files to check if any track items contain unsaved changes.
    ///
    /// # Returns
    ///
    /// `true` if modified documents exist; otherwise, `false`.
    pub fn any_modified(&self) -> bool { self.buffers.iter().any(|b: &Buffer| b.modified) }
}