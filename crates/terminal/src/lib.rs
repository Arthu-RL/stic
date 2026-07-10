//! Cross-platform integrated terminal for Stic.
//!
//! Spawns the user's shell in a real pseudo-terminal via [`portable_pty`] so
//! it behaves identically to a native terminal (job control, `cd`, colors,
//! interactive programs) on Linux, macOS, and Windows alike — no per-OS code
//! needed anywhere else in the app.
//!
//! Output bytes are continuously fed into a [`vt100::Parser`] on a background
//! thread, which maintains a faithful terminal screen grid (cursor, colors,
//! wrapped lines). [`PtySession::render`] draws that grid directly with the
//! [`tui_term`] widget, so the `ui` crate only needs a `PtySession` and never
//! has to know about PTYs, ANSI parsing, or platform shells.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use ratatui::{layout::Rect, Frame};
use tui_term::widget::{Cursor, PseudoTerminal};


/// A single interactive shell session backed by a native pseudo-terminal.
///
/// Reading, ANSI parsing, and rendering are all handled internally so
/// callers only need [`write_input`](Self::write_input), [`resize`](Self::resize),
/// and [`render`](Self::render).
pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child:  Box<dyn Child + Send + Sync>,
    /// Shared with the background reader thread; holds the parsed screen grid.
    parser: Arc<Mutex<vt100::Parser>>,
    rows:   u16,
    cols:   u16,
}

impl PtySession {
    /// Spawns the platform's default interactive shell into a fresh
    /// pseudo-terminal rooted at `cwd`.
    ///
    /// The shell is `$SHELL` on Unix or `%COMSPEC%` on Windows, falling back
    /// to `/bin/sh` / `cmd.exe` respectively when unset.
    ///
    /// # Arguments
    ///
    /// * `cwd`  - Working directory the shell starts in.
    /// * `rows` - Initial terminal height in character rows (clamped to `1`).
    /// * `cols` - Initial terminal width in character columns (clamped to `1`).
    ///
    /// # Returns
    ///
    /// A ready `PtySession` with the shell already running, or an error if
    /// the pseudo-terminal or shell process could not be created.
    pub fn spawn(cwd: &Path, rows: u16, cols: u16) -> Result<Self> {
        let rows: u16 = rows.max(1);
        let cols: u16 = cols.max(1);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .context("failed to allocate a pseudo-terminal")?;

        let mut cmd: CommandBuilder = CommandBuilder::new(default_shell());
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave
            .spawn_command(cmd)
            .context("failed to spawn shell process")?;
        drop(pair.slave); // slave fd is only needed for the initial spawn

        let writer = pair.master.take_writer().context("failed to open pty writer")?;
        let mut reader = pair.master.try_clone_reader().context("failed to open pty reader")?;

        let parser: Arc<Mutex<vt100::Parser>> = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 4096)));
        let parser_bg: Arc<Mutex<vt100::Parser>> = Arc::clone(&parser);

        // Background reader: pumps shell output into the vt100 parser so the
        // render thread only ever needs a brief lock to read the latest screen.
        std::thread::spawn(move || {
            let mut buf: [u8; 8192] = [0; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if let Ok(mut p) = parser_bg.lock() {
                            p.process(&buf[..n]);
                        }
                    }
                }
            }
        });

        Ok(Self { writer, master: pair.master, child, parser, rows, cols })
    }

    /// Writes raw bytes to the shell's stdin, as if typed at a real terminal.
    ///
    /// # Arguments
    ///
    /// * `bytes` - Raw input bytes (already encoded, e.g. `\x1b[A` for Up).
    pub fn write_input(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.write_all(bytes).context("failed to write to pty")?;
        self.writer.flush().context("failed to flush pty writer")?;
        Ok(())
    }

    /// Resizes the pseudo-terminal and its screen grid, if the size actually
    /// changed. Safe to call every render frame.
    ///
    /// # Arguments
    ///
    /// * `rows` - New terminal height in character rows.
    /// * `cols` - New terminal width in character columns.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows: u16 = rows.max(1);
        let cols: u16 = cols.max(1);
        if rows == self.rows && cols == self.cols {
            return;
        }
        let _ = self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_size(rows, cols);
        }
        self.rows = rows;
        self.cols = cols;
    }

    /// Draws the current terminal screen into `area` using the [`tui_term`]
    /// widget, hiding the cursor block when the panel is not focused.
    ///
    /// # Arguments
    ///
    /// * `frame`   - The active ratatui render frame.
    /// * `area`    - Screen region to draw into.
    /// * `focused` - Whether the terminal has keyboard focus (shows a solid cursor).
    pub fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let Ok(parser) = self.parser.lock() else { return };
        let mut cursor: Cursor = Cursor::default();
        if !focused {
            cursor.hide();
        }
        let widget = PseudoTerminal::new(parser.screen()).cursor(cursor);
        frame.render_widget(widget, area);
    }

    /// Checks whether the shell process is still running, without blocking.
    ///
    /// # Returns
    ///
    /// `true` if the process has not exited yet (or its status could not be
    /// determined), `false` once it has terminated.
    pub fn is_alive(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for PtySession {
    /// Terminates the shell process when the session is dropped so closing
    /// the panel never leaves an orphaned process running in the background.
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Resolves the platform's default interactive shell executable.
///
/// # Returns
///
/// `$SHELL` on Unix (fallback `/bin/sh`) or `%COMSPEC%` on Windows (fallback
/// `cmd.exe`).
fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}
