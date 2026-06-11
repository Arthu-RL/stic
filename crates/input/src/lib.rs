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


/// A trait managing interface input events mapping down onto application state.
pub trait InputHandler {
    /// Evaluates and processes target keyboard entry triggers.
    fn handle_key(app: &mut App, key: KeyEvent);

    /// Evaluates and parses peripheral mouse tracking metrics.
    /// Provides a blank default implementation for modes that ignore mouse gestures.
    fn handle_mouse(_app: &mut App, _mouse: MouseEvent) {}
}


/// Polls for incoming hardware terminal events and routes them to appropriate handlers.
/// Evaluates global hotkeys before delegating specific input keys to active mode layout handlers.
/// 
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
///
/// # Returns
///
/// A `Result` indicating success or an underlying event retrieval or execution error.
pub fn handle_input(app: &mut App) -> Result<()> {
    if !event::poll(std::time::Duration::from_millis(16))? {
        return Ok(());
    }

    match event::read()? {
        Event::Key(key) => {
            // Intercepts and parses global hotkeys regardless of active runtime mode configurations.
            // Returns `true` if a global shortcut was executed and event processing should halt.
            let ctrl: bool = key.modifiers.contains(KeyModifiers::CONTROL);
            let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);

            if ctrl && key.code == KeyCode::Char('p') {
                if app.mode == Mode::CommandPalette {
                    app.mode = Mode::Normal;
                } else {
                    app.command_palette.reset();
                    app.mode = Mode::CommandPalette;
                }
                return Ok(());
            }

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
            }
        }
        Event::Resize(_, _) => {}
        _ => {}
    }
    Ok(())
}


pub struct NormalHandler;

impl InputHandler for NormalHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        let ctrl: bool  = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);
        let alt: bool   = key.modifiers.contains(KeyModifiers::ALT);
        let _cfg   = &app.config.editor;

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
            KeyCode::Char('j') | KeyCode::Up            => app.editor.buf_mut().move_up(1),
            KeyCode::Char('k') | KeyCode::Down          => app.editor.buf_mut().move_down(1),
            KeyCode::Char('l') | KeyCode::Right if !alt => app.editor.buf_mut().move_right(),
            KeyCode::Char('q') | KeyCode::Home if !ctrl => app.editor.buf_mut().move_line_start(),
            KeyCode::Char('e') | KeyCode::End           => app.editor.buf_mut().move_line_end(),
            KeyCode::Char('g') if !ctrl                 => app.editor.buf_mut().goto_file_start(),
            KeyCode::Char('G')                          => app.editor.buf_mut().goto_file_end(),
            KeyCode::Char('a')                          => app.editor.buf_mut().move_word_forward(),
            KeyCode::Char('d') if !ctrl                 => app.editor.buf_mut().move_word_backward(),
            KeyCode::PageUp                             => app.editor.buf_mut().move_page_up(20),
            KeyCode::PageDown                           => app.editor.buf_mut().move_page_down(20),

            KeyCode::Char('s') if ctrl                  => app.save_file(),
            KeyCode::Char('z') if ctrl                  => app.editor.buf_mut().undo(),
            KeyCode::Char('y') if ctrl                  => app.editor.buf_mut().redo(),
            KeyCode::Char('c') if ctrl                  => {
                if let Some(text) = app.editor.buf().selected_text() {
                    app.clipboard = text;
                    app.set_message("Copied");
                }
            }
            KeyCode::Char('v') if ctrl                  => {
                let text = app.clipboard.clone();
                if !text.is_empty() {
                    app.editor.buf_mut().delete_selection();
                    app.editor.buf_mut().insert_str(&text);
                }
            }
            KeyCode::Char('f') if ctrl                  => {
                app.prompt_input.clear();
                app.search.last_match = None;
                app.mode = Mode::Search;
            }
            KeyCode::Char('g') if ctrl                  => {
                app.prompt_input.clear();
                app.mode = Mode::GotoLine;
            }
            KeyCode::Char('b') if ctrl                  => app.toggle_file_tree(),
            KeyCode::Char('t') if ctrl                  => app.show_terminal = !app.show_terminal,
            KeyCode::Char('d') if ctrl                  => app.show_diag = !app.show_diag,
            KeyCode::Char('w') if ctrl                  => app.editor.close_active(),
            KeyCode::Char('n') if ctrl                  => app.editor.new_buffer(),
            KeyCode::Char('q') if ctrl                  => app.try_quit(),

            KeyCode::Left  if alt                       => app.editor.prev_tab(),
            KeyCode::Right if alt                       => app.editor.next_tab(),

            KeyCode::F(3)                               => app.search_next(),
            KeyCode::F(12)                              => app.set_message("LSP: go_to_def not yet wired"),

            KeyCode::Char('x')                          => app.editor.buf_mut().delete_forward(),
            KeyCode::Delete                             => app.editor.buf_mut().delete_forward(),

            KeyCode::Char('D') if ctrl && shift         => app.editor.buf_mut().duplicate_line(),

            _ => {}
        }

        let h: usize = 24;
        app.editor.buf_mut().scroll_to_cursor(h);
    }

    fn handle_mouse(app: &mut App, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => app.editor.buf_mut().move_up(3),
            MouseEventKind::ScrollDown => app.editor.buf_mut().move_down(3),
            MouseEventKind::Down(MouseButton::Left) => {
                let editor: &mut buffer::Buffer = app.editor.buf_mut();

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
            },
            MouseEventKind::Drag(MouseButton::Left) => {
                let editor: &mut buffer::Buffer = app.editor.buf_mut();

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


pub struct InsertHandler;

impl InputHandler for InsertHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        let ctrl: bool = key.modifiers.contains(KeyModifiers::CONTROL);
        let cfg  = app.config.editor.clone();

        match key.code {
            KeyCode::Esc => {
                app.editor.buf_mut().clear_selection();
                if app.editor.buf().cursor.col > 0 {
                    app.editor.buf_mut().move_left();
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
                        let start_col: usize = app.editor.buf().cursor.col;
                        while app.editor.buf().cursor.col > 0 {
                            let col: usize = app.editor.buf().cursor.col;
                            let line: usize = app.editor.buf().cursor.line;
                            let line_str: String = app.editor.buf().get_line(line);
                            let ch: Option<char> = line_str.chars().nth(col.saturating_sub(1));
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
            }
            KeyCode::Enter => {
                app.editor.buf_mut().delete_selection();
                let indent: String = if cfg.auto_indent {
                    let cur_line: usize = app.editor.buf().cursor.line;
                    let ls: String = app.editor.buf().get_line(cur_line);
                    ls.chars().take_while(|c: &char| c.is_whitespace()).collect::<String>()
                } else {
                    String::new()
                };
                app.editor.buf_mut().insert_char('\n');
                if !indent.is_empty() {
                    app.editor.buf_mut().insert_str(&indent);
                }
            }
            KeyCode::Tab => {
                app.editor.buf_mut().delete_selection();
                app.editor.buf_mut().insert_tab(cfg.tab_size, cfg.use_spaces);
            }
            KeyCode::Backspace => {
                if !app.editor.buf_mut().delete_selection() {
                    app.editor.buf_mut().delete_backward();
                }
            }
            KeyCode::Delete => {
                if !app.editor.buf_mut().delete_selection() {
                    app.editor.buf_mut().delete_forward();
                }
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
        let h: usize = 24;
        app.editor.buf_mut().scroll_to_cursor(h);
    }

    fn handle_mouse(app: &mut App, mouse: MouseEvent) {
        // Shared editing interaction behaviors match NormalHandler specifications
        NormalHandler::handle_mouse(app, mouse);
    }
}


pub struct CommandHandler;

impl CommandHandler {
    /// Evaluates and processes textual console instructions written inside the command bar.
    ///
    /// # Arguments
    ///
    /// * `app` - Mutable reference to the central application state engine instance.
    /// * `cmd` - The trimmed instruction string slice to execute.
    pub fn execute_colon_cmd(app: &mut App, cmd: &str) {
        match cmd {
            "w" | "write"          => app.save_file(),
            "q" | "quit"           => app.try_quit(),
            "wq" | "x"             => { app.save_file(); app.try_quit(); }
            "q!" | "quit!"         => app.should_quit = true,
            "wq!"                  => { app.save_file(); app.should_quit = true; }
            s if s.starts_with("e ") => {
                let path: &std::path::Path = std::path::Path::new(s[2..].trim());
                if let Err(e) = app.open_file(path) {
                    app.set_message(format!("Error: {e}"));
                }
            }
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
            KeyCode::Esc           => { app.prompt_input.clear(); app.mode = Mode::Normal; }
            KeyCode::Enter         => {
                let cmd = app.prompt_input.trim().to_string();
                app.prompt_input.clear();
                app.mode = Mode::Normal;
                Self::execute_colon_cmd(app, &cmd);
            }
            KeyCode::Char(c)       => app.prompt_input.push(c),
            KeyCode::Backspace     => { app.prompt_input.pop(); }
            _ => {}
        }
    }
}


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
                let q: String = app.prompt_input.clone();
                if let Some((line, col)) = app.editor.buf().search_forward(&q) {
                    app.editor.buf_mut().cursor.set(line, col);
                    app.editor.buf_mut().scroll_to_cursor(24);
                    app.search.last_match = Some((line, col));
                }
            }
            KeyCode::Backspace => {
                app.prompt_input.pop();
            }
            _ => {}
        }
    }
}


pub struct GotoLineHandler;

impl InputHandler for GotoLineHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc   => { app.prompt_input.clear(); app.mode = Mode::Normal; }
            KeyCode::Enter => {
                let input: String = app.prompt_input.trim().to_string();
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


pub struct CommandPaletteHandler;

impl InputHandler for CommandPaletteHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc           => { app.command_palette.reset(); app.mode = Mode::Normal; }
            KeyCode::Enter         => {
                if let Some(id) = app.command_palette.selected_id() {
                    let id: String = id.to_string();
                    app.command_palette.reset();
                    app.mode = Mode::Normal;
                    app.execute_palette_command(&id);
                }
            }
            KeyCode::Up            => app.command_palette.move_up(),
            KeyCode::Down          => app.command_palette.move_down(),
            KeyCode::Char(c)       => app.command_palette.push_char(c),
            KeyCode::Backspace     => app.command_palette.pop_char(),
            _ => {}
        }
    }
}


pub struct FileTreeHandler;

impl InputHandler for FileTreeHandler {
    fn handle_key(app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ft) = &mut app.file_tree { ft.move_down(); }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ft) = &mut app.file_tree { ft.move_up(); }
            }
            KeyCode::Enter => {
                let path: Option<std::path::PathBuf> = app.file_tree.as_ref()
                    .and_then(|ft: &app::FileTreeState| ft.selected_path())
                    .map(|p: &std::path::Path| p.to_path_buf());
                if let Some(p) = path {
                    if p.is_file() {
                        if let Err(e) = app.open_file(&p) {
                            app.set_message(format!("Error: {e}"));
                        }
                        app.mode = Mode::Normal;
                    }
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
                if let Some(ft) = &mut app.file_tree { 
                    ft.move_up(); 
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(ft) = &mut app.file_tree { 
                    ft.move_down(); 
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(ft) = &mut app.file_tree {
                    let clicked_row: usize = mouse.row as usize;
                    ft.select_row(clicked_row);
                    
                    let path: Option<std::path::PathBuf> = ft.selected_path()
                        .map(|p: &std::path::Path| p.to_path_buf());
                    
                    if let Some(p) = path {
                        if p.is_file() {
                            if let Err(e) = app.open_file(&p) {
                                app.set_message(format!("Error: {e}"));
                            }
                            app.mode = Mode::Normal;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}