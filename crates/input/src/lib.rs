//! Input crate – maps raw crossterm key events onto App state mutations.
//!
//! Design goals
//! ─────────────
//! * Zero allocations in the hot path (key already pressed events).
//! * Easy to extend: each mode has its own handler function.
//! * No hard-coded strings except the fallback message.


use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};

use app::{App, Mode};


/// Trait governing how a mode handles raw input events.
pub trait InputHandler {
    /// Processes a single keyboard event for the implementing mode.
    ///
    /// # Arguments
    ///
    /// * `app` - Mutable reference to the root application state.
    /// * `key` - The key event received from the terminal.
    fn handle_key(app: &mut App, key: KeyEvent);

    /// Processes a mouse event for the implementing mode.
    ///
    /// Provides a no-op default; override in modes that need pointer input.
    ///
    /// # Arguments
    ///
    /// * `app`   - Mutable reference to the root application state.
    /// * `mouse` - The mouse event received from the terminal.
    fn handle_mouse(_app: &mut App, _mouse: MouseEvent) {}
}


/// Polls for the next terminal event and routes it to the correct mode handler.
///
/// Global hotkeys (`Ctrl+P`, `Ctrl+Shift+Q`) are intercepted before the
/// per-mode dispatch so they work regardless of the active mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the root application state.
///
/// # Returns
///
/// `Ok(())` on success, or an `Err` if crossterm event polling fails.
pub fn handle_input(app: &mut App) -> Result<()> {
    if !event::poll(std::time::Duration::from_millis(16))? {
        return Ok(());
    }

    match event::read()? {
        Event::Key(key) => {
            let ctrl:  bool = key.modifiers.contains(KeyModifiers::CONTROL);
            let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);

            // Global: toggle Command Palette from any mode.
            if ctrl && key.code == KeyCode::Char('p') {
                if app.mode == Mode::CommandPalette {
                    app.mode = Mode::Normal;
                } else {
                    app.command_palette.reset();
                    app.mode = Mode::CommandPalette;
                }
                return Ok(());
            }

            // Global: save active file then force-quit.
            if ctrl && shift && (key.code == KeyCode::Char('Q') || key.code == KeyCode::Char('q')) {
                app.save_file();
                app.should_quit = true;
                return Ok(());
            }

            match app.mode {
                Mode::Normal         => NormalHandler::handle_key(app, key),
                Mode::Insert         => InsertHandler::handle_key(app, key),
                Mode::Command        => CommandHandler::handle_key(app, key),
                Mode::Search         => SearchHandler::handle_key(app, key),
                Mode::GotoLine       => GotoLineHandler::handle_key(app, key),
                Mode::CommandPalette => CommandPaletteHandler::handle_key(app, key),
                Mode::FileTree       => FileTreeHandler::handle_key(app, key),
                Mode::SaveAs         => SaveAsHandler::handle_key(app, key),
            }
        }
        Event::Mouse(mouse) => {
            match app.mode {
                Mode::Normal         => NormalHandler::handle_mouse(app, mouse),
                Mode::Insert         => InsertHandler::handle_mouse(app, mouse),
                Mode::Command        => CommandHandler::handle_mouse(app, mouse),
                Mode::Search         => SearchHandler::handle_mouse(app, mouse),
                Mode::GotoLine       => GotoLineHandler::handle_mouse(app, mouse),
                Mode::CommandPalette => CommandPaletteHandler::handle_mouse(app, mouse),
                Mode::FileTree       => FileTreeHandler::handle_mouse(app, mouse),
                Mode::SaveAs         => SaveAsHandler::handle_mouse(app, mouse),
            }
        }
        Event::Resize(_, _) => {}
        _ => {}
    }
    Ok(())
}


/// Input handler for [`Mode::Normal`].
pub struct NormalHandler;

impl InputHandler for NormalHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        let ctrl:  bool = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);
        let alt:   bool = key.modifiers.contains(KeyModifiers::ALT);
        let _cfg        = &app.config.editor;

        if !ctrl && key.code != KeyCode::Esc {
            app.editor.buf_mut().clear_selection();
        }

        match key.code {
            KeyCode::Char('i')                          => app.mode = Mode::Insert,
            KeyCode::Char('I')                          => {
                app.editor.buf_mut().move_line_start();
                app.mode = Mode::Insert;
            }
            KeyCode::Char('o')                          => {
                app.editor.buf_mut().move_line_end();
                app.editor.buf_mut().insert_char('\n');
                app.mode = Mode::Insert;
            }
            KeyCode::Char('O')                          => {
                let line: usize = app.editor.buf().cursor.line;
                if line == 0 {
                    app.editor.buf_mut().goto_file_start();
                    app.editor.buf_mut().insert_char('\n');
                    app.editor.buf_mut().move_up(1);
                } else {
                    app.editor.buf_mut().move_up(1);
                    app.editor.buf_mut().move_line_end();
                    app.editor.buf_mut().insert_char('\n');
                }
                app.mode = Mode::Insert;
            }
            KeyCode::Char(':')                          => {
                app.prompt_input.clear();
                app.mode = Mode::Command;
            }

            KeyCode::Char('h') | KeyCode::Left if !alt  => app.editor.buf_mut().move_left(),
            KeyCode::Char('j') | KeyCode::Up             => app.editor.buf_mut().move_up(1),
            KeyCode::Char('k') | KeyCode::Down           => app.editor.buf_mut().move_down(1),
            KeyCode::Char('l') | KeyCode::Right if !alt  => app.editor.buf_mut().move_right(),
            KeyCode::Char('q') | KeyCode::Home if !ctrl  => app.editor.buf_mut().move_line_start(),
            KeyCode::Char('e') | KeyCode::End            => app.editor.buf_mut().move_line_end(),
            KeyCode::Char('g') if !ctrl                  => app.editor.buf_mut().goto_file_start(),
            KeyCode::Char('G')                           => app.editor.buf_mut().goto_file_end(),
            KeyCode::Char('a')                           => app.editor.buf_mut().move_word_backward(),
            KeyCode::Char('d') if !ctrl                  => app.editor.buf_mut().move_word_forward(),
            KeyCode::PageUp                              => app.editor.buf_mut().move_page_up(20),
            KeyCode::PageDown                            => app.editor.buf_mut().move_page_down(20),

            KeyCode::Char('s') if ctrl                   => app.save_file(),
            KeyCode::Char('z') if ctrl                   => app.editor.buf_mut().undo(),
            KeyCode::Char('y') if ctrl                   => app.editor.buf_mut().redo(),
            KeyCode::Char('c') if ctrl                   => {
                if let Some(text) = app.editor.buf().selected_text() {
                    app.clipboard = text;
                    app.set_message("Copied");
                }
            }
            KeyCode::Char('v') if ctrl                   => {
                let text = app.clipboard.clone();
                if !text.is_empty() {
                    app.editor.buf_mut().delete_selection();
                    app.editor.buf_mut().insert_str(&text);
                }
            }
            KeyCode::Char('f') if ctrl                   => {
                app.prompt_input.clear();
                app.search.last_match = None;
                app.mode = Mode::Search;
            }
            KeyCode::Char('g') if ctrl                   => {
                app.prompt_input.clear();
                app.mode = Mode::GotoLine;
            }
            KeyCode::Char('b') if ctrl                   => app.toggle_file_tree(),
            KeyCode::Char('t') if ctrl                   => app.show_terminal = !app.show_terminal,
            KeyCode::Char('d') if ctrl                   => app.show_diag = !app.show_diag,
            KeyCode::Char('w') if ctrl                   => app.editor.close_active(),
            KeyCode::Char('n') if ctrl                   => app.editor.new_buffer(),
            KeyCode::Char('q') if ctrl                   => app.try_quit(),

            KeyCode::Left  if alt                        => app.editor.prev_tab(),
            KeyCode::Right if alt                        => app.editor.next_tab(),

            KeyCode::F(3)                                => app.search_next(),
            KeyCode::F(12)                               => app.lsp_goto_def_key(),

            KeyCode::Char('x')                           => app.editor.buf_mut().delete_forward(),
            KeyCode::Delete                              => app.editor.buf_mut().delete_forward(),

            KeyCode::Char('D') if ctrl && shift          => app.editor.buf_mut().duplicate_line(),

            _ => {}
        }

        app.editor.buf_mut().scroll_to_cursor(24);
    }

    fn handle_mouse(app: &mut App, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp   => app.editor.buf_mut().move_up(3),
            MouseEventKind::ScrollDown => app.editor.buf_mut().move_down(3),
            MouseEventKind::Down(MouseButton::Left) => {
                let editor = app.editor.buf_mut();

                let target_line: usize = (mouse.row as usize).saturating_sub(1) + editor.scroll_top;

                let gutter_w: usize = if app.config.editor.line_numbers {
                    let digits: usize = editor.line_count().to_string().len().max(3);
                    digits + 2
                } else {
                    0
                };

                let target_col: usize = (mouse.column as usize).saturating_sub(gutter_w) + editor.scroll_left;

                editor.clear_selection();
                editor.goto_line_col(target_line, target_col, 24);
                editor.start_selection();
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let editor = app.editor.buf_mut();

                let target_line: usize = (mouse.row as usize).saturating_sub(1) + editor.scroll_top;

                let gutter_w: usize = if app.config.editor.line_numbers {
                    let digits: usize = editor.line_count().to_string().len().max(3);
                    digits + 2
                } else {
                    0
                };

                let target_col: usize = (mouse.column as usize).saturating_sub(gutter_w) + editor.scroll_left;
                editor.goto_line_col(target_line, target_col, 24);
            }
            _ => {}
        }
    }
}


/// Input handler for [`Mode::Insert`].
pub struct InsertHandler;

impl InputHandler for InsertHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        let ctrl: bool = key.modifiers.contains(KeyModifiers::CONTROL);
        let cfg  = app.config.editor.clone();

        match key.code {
            KeyCode::Esc => {
                let buf: &mut buffer::Buffer = app.editor.buf_mut();
                buf.clear_selection();
                if buf.cursor.col > 0 {
                    buf.move_left();
                }
                app.mode = Mode::Normal;
            }

            KeyCode::Char(c) if ctrl => {
                match c {
                    'c' => {
                        if let Some(text) = app.editor.buf().selected_text() {
                            app.clipboard = text;
                            app.set_message("Copied");
                        }
                    }
                    'v' => {
                        let text = app.clipboard.clone();
                        if !text.is_empty() {
                            app.editor.buf_mut().delete_selection();
                            app.editor.buf_mut().insert_str(&text);
                        }
                    }
                    's' => app.save_file(),
                    'z' => app.editor.buf_mut().undo(),
                    'y' => app.editor.buf_mut().redo(),
                    'w' => {
                        // Delete word backward (Ctrl+W).
                        let start_col: usize = app.editor.buf().cursor.col;
                        while app.editor.buf().cursor.col > 0 {
                            let col:      usize  = app.editor.buf().cursor.col;
                            let line:     usize  = app.editor.buf().cursor.line;
                            let line_str: String = app.editor.buf().get_line(line);
                            let ch = line_str.chars().nth(col.saturating_sub(1));
                            if ch.map(|c: char| c.is_whitespace()).unwrap_or(false) && col < start_col { break; }
                            app.editor.buf_mut().delete_backward();
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Char(c) => {
                app.editor.buf_mut().delete_selection();
                app.editor.buf_mut().insert_char(c);
                app.notify_lsp_change();
            }
            KeyCode::Enter => {
                app.editor.buf_mut().delete_selection();
                let indent: String = if cfg.auto_indent {
                    let cur_line: usize = app.editor.buf().cursor.line;
                    let ls: String = app.editor.buf().get_line(cur_line);
                    ls.chars().take_while(|c: &char| c.is_whitespace()).collect()
                } else {
                    String::new()
                };
                app.editor.buf_mut().insert_char('\n');
                if !indent.is_empty() {
                    app.editor.buf_mut().insert_str(&indent);
                }
                app.notify_lsp_change();
            }
            KeyCode::Tab => {
                app.editor.buf_mut().delete_selection();
                app.editor.buf_mut().insert_tab(cfg.tab_size, cfg.use_spaces);
                app.notify_lsp_change();
            }
            KeyCode::Backspace => {
                if !app.editor.buf_mut().delete_selection() {
                    app.editor.buf_mut().delete_backward();
                }
                app.notify_lsp_change();
            }
            KeyCode::Delete => {
                if !app.editor.buf_mut().delete_selection() {
                    app.editor.buf_mut().delete_forward();
                }
                app.notify_lsp_change();
            }
            KeyCode::Home  => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_line_start(); }
            KeyCode::End   => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_line_end();   }
            KeyCode::Left  => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_left();       }
            KeyCode::Right => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_right();      }
            KeyCode::Up    => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_up(1);   app.mode = Mode::Normal; }
            KeyCode::Down  => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_down(1); app.mode = Mode::Normal; }
            KeyCode::PageUp   => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_page_up(20);   }
            KeyCode::PageDown => { app.editor.buf_mut().clear_selection(); app.editor.buf_mut().move_page_down(20); }

            _ => {}
        }
        app.editor.buf_mut().scroll_to_cursor(24);
    }

    fn handle_mouse(app: &mut App, mouse: MouseEvent) {
        NormalHandler::handle_mouse(app, mouse);
    }
}


/// Input handler for [`Mode::Command`] (`:` command-line).
pub struct CommandHandler;

impl CommandHandler {
    /// Executes a colon command string entered in the command bar.
    ///
    /// Supported commands are documented in the module-level keybinding
    /// reference under "Command Mode".
    ///
    /// # Arguments
    ///
    /// * `app` - Mutable reference to the root application state.
    /// * `cmd` - Trimmed command string (without the leading `:`).
    pub fn execute_colon_cmd(app: &mut App, cmd: &str) {
        match cmd {
            "w" | "write"   => app.save_file(),
            "q" | "quit"    => app.try_quit(),
            "wq" | "x"      => { app.save_file(); app.try_quit(); }
            "q!" | "quit!"  => app.should_quit = true,
            "wq!"           => { app.save_file(); app.should_quit = true; }
            // :w <path>  –– Save As to the given path.
            s if s.starts_with("w ") => {
                let path = s[2..].trim();
                app.save_as_file(path);
            }
            // :e <path>  –– Open / switch to file.
            s if s.starts_with("e ") => {
                let path = std::path::Path::new(s[2..].trim());
                if let Err(e) = app.open_file(path) {
                    app.set_message(format!("Error: {e}"));
                }
            }
            // :<number>  –– Jump to line.
            s if s.parse::<usize>().is_ok() => {
                let n: usize = s.parse().unwrap();
                app.editor.buf_mut().goto_line(n.saturating_sub(1), 24);
            }
            _ => app.set_message(format!("Unknown command: :{cmd}")),
        }
    }
}

impl InputHandler for CommandHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc       => { app.prompt_input.clear(); app.mode = Mode::Normal; }
            KeyCode::Enter     => {
                let cmd = app.prompt_input.trim().to_string();
                app.prompt_input.clear();
                app.mode = Mode::Normal;
                Self::execute_colon_cmd(app, &cmd);
            }
            KeyCode::Char(c)   => app.prompt_input.push(c),
            KeyCode::Backspace => { app.prompt_input.pop(); }
            _ => {}
        }
    }
}


/// Input handler for [`Mode::Search`].
pub struct SearchHandler;

impl InputHandler for SearchHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc   => { app.mode = Mode::Normal; }
            KeyCode::Enter => {
                app.search.query = app.prompt_input.clone();
                app.prompt_input.clear();
                app.mode = Mode::Normal;
                app.search_next();
            }
            KeyCode::Char(c) => {
                app.prompt_input.push(c);
                let q = app.prompt_input.clone();
                if let Some((line, col)) = app.editor.buf().search_forward(&q) {
                    app.editor.buf_mut().cursor.set(line, col);
                    app.editor.buf_mut().scroll_to_cursor(24);
                    app.search.last_match = Some((line, col));
                }
            }
            KeyCode::Backspace => { app.prompt_input.pop(); }
            _ => {}
        }
    }
}


/// Input handler for [`Mode::GotoLine`].
pub struct GotoLineHandler;

impl InputHandler for GotoLineHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc   => { app.prompt_input.clear(); app.mode = Mode::Normal; }
            KeyCode::Enter => {
                let input = app.prompt_input.trim().to_string();
                app.prompt_input.clear();
                app.mode = Mode::Normal;
                if let Ok(n) = input.parse::<usize>() {
                    app.editor.buf_mut().goto_line(n.saturating_sub(1), 24);
                } else {
                    app.set_message(format!("Not a line number: '{input}'"));
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => app.prompt_input.push(c),
            KeyCode::Backspace => { app.prompt_input.pop(); }
            _ => {}
        }
    }
}


/// Input handler for [`Mode::CommandPalette`].
pub struct CommandPaletteHandler;

impl InputHandler for CommandPaletteHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc       => { app.command_palette.reset(); app.mode = Mode::Normal; }
            KeyCode::Enter     => {
                if let Some(id) = app.command_palette.selected_id() {
                    let id = id.to_string();
                    app.command_palette.reset();
                    app.mode = Mode::Normal;
                    app.execute_palette_command(&id);
                }
            }
            KeyCode::Up        => app.command_palette.move_up(),
            KeyCode::Down      => app.command_palette.move_down(12),
            KeyCode::Char(c)   => app.command_palette.push_char(c),
            KeyCode::Backspace => app.command_palette.pop_char(),
            _ => {}
        }
    }
}


/// Input handler for [`Mode::FileTree`].
///
/// ## Keybindings
///
/// | Key             | Action                                                  |
/// |-----------------|---------------------------------------------------------|
/// | `j` / `↓`       | Move selection down                                     |
/// | `k` / `↑`       | Move selection up                                       |
/// | `Space` / `l` / `→` | Expand directory / collapse if already expanded    |
/// | `h` / `←`       | Collapse directory, or jump to parent if already closed |
/// | `Enter`         | Open file in editor (directories: toggle expand)        |
/// | `r`             | Refresh tree (re-scans root)                            |
/// | `Esc` / `q`     | Return to Normal mode                                   |
pub struct FileTreeHandler;

impl InputHandler for FileTreeHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.toggle_file_tree(),
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ft) = &mut app.file_tree { ft.move_down(25); }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ft) = &mut app.file_tree { ft.move_up(); }
            }
            KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                if let Some(ft) = &mut app.file_tree { ft.toggle_selected(); }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(ft) = &mut app.file_tree { ft.collapse_or_jump_parent(); }
            }
            KeyCode::Enter => {
                let path = app.file_tree.as_mut().and_then(|ft| {
                    // Check whether selected item is a file; toggle dirs here too.
                    let p = ft.selected_path()?;
                    if p.is_dir() {
                        ft.toggle_selected();
                        None // directories don't open in the editor
                    } else {
                        Some(p)
                    }
                });
                if let Some(p) = path {
                    if let Err(e) = app.open_file(&p) {
                        app.set_message(format!("Error: {e}"));
                    }
                    app.mode = Mode::Normal;
                }
            }

            KeyCode::Char('r') => {
                if let Some(ft) = &mut app.file_tree { ft.refresh(); }
            }
            _ => {}
        }
    }

    fn handle_mouse(app: &mut App, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(ft) = &mut app.file_tree { ft.move_up(); }
            }
            MouseEventKind::ScrollDown => {
                if let Some(ft) = &mut app.file_tree { ft.move_down(25); }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(ft) = &mut app.file_tree {
                    let relative_row = (mouse.row as usize).saturating_sub(1);
                    let path = ft.click_row(relative_row);
                    if let Some(p) = path {
                        if p.is_file() {
                            if let Err(e) = app.open_file(&p) {
                                app.set_message(format!("Error: {e}"));
                            }
                            app.mode = Mode::Normal;
                        } else if p.is_dir() {
                            ft.toggle_selected();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}


/// Input handler for [`Mode::SaveAs`].
///
/// The user types a destination path into the bottom prompt bar.
/// `Enter` confirms and writes the file; `Esc` cancels.
pub struct SaveAsHandler;

impl InputHandler for SaveAsHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                app.prompt_input.clear();
                app.set_message("Save As: cancelled");
                app.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let path = app.prompt_input.trim().to_string();
                app.prompt_input.clear();
                app.mode = Mode::Normal;
                app.save_as_file(&path);
            }
            KeyCode::Char(c)   => app.prompt_input.push(c),
            KeyCode::Backspace => { app.prompt_input.pop(); }
            _ => {}
        }
    }
}
