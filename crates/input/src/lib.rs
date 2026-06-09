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


/// Polls for incoming hardware terminal events and routes them to appropriate handlers.
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
        Event::Key(key) => handle_key(app, key),
        Event::Mouse(mouse) => handle_mouse(app, mouse),
        Event::Resize(_, _) => {}
        _ => {}
    }
    Ok(())
}


/// Evaluates global hotkeys before delegating specific input keys to active mode layout handlers.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The incoming keyboard input event to evaluate.
fn handle_key(app: &mut App, key: KeyEvent) {
    let ctrl: bool = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);
    let _alt: bool = key.modifiers.contains(KeyModifiers::ALT);

    if ctrl && key.code == KeyCode::Char('p') {
        if app.mode == Mode::CommandPalette {
            app.mode = Mode::Normal;
        } else {
            app.command_palette.reset();
            app.mode = Mode::CommandPalette;
        }
        return;
    }

    if ctrl && shift && key.code == KeyCode::Char('Q') {
        app.should_quit = true;
        return;
    }

    match app.mode.clone() {
        Mode::Normal => normal_mode(app, key),
        Mode::Insert => insert_mode(app, key),
        Mode::Command => command_mode(app, key),
        Mode::Search => search_mode(app, key),
        Mode::GotoLine => goto_line_mode(app, key),
        Mode::CommandPalette => palette_mode(app, key),
        Mode::FileTree => filetree_mode(app, key),
    }
}


/// Handles keyboard inputs when the application layer is configured in Normal mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn normal_mode(app: &mut App, key: KeyEvent) {
    let ctrl: bool  = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift: bool = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt: bool   = key.modifiers.contains(KeyModifiers::ALT);
    let _cfg   = &app.config.editor;

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

    let h = 24;
    app.editor.buf_mut().scroll_to_cursor(h);
}


/// Handles keyboard inputs when the application layer is configured in Insert mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn insert_mode(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let cfg  = app.config.editor.clone();

    match key.code {
        KeyCode::Esc => {
            if app.editor.buf().cursor.col > 0 {
                app.editor.buf_mut().move_left();
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char(c) if ctrl => {
            match c {
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
            app.editor.buf_mut().insert_char(c);
        }
        KeyCode::Enter => {
            let indent: String = if cfg.auto_indent {
                let cur_line: usize = app.editor.buf().cursor.line;
                let ls: String = app.editor.buf().get_line(cur_line);
                ls.chars().take_while(|c| c.is_whitespace()).collect::<String>()
            } else {
                String::new()
            };
            app.editor.buf_mut().insert_char('\n');
            if !indent.is_empty() {
                app.editor.buf_mut().insert_str(&indent);
            }
        }
        KeyCode::Backspace     => app.editor.buf_mut().delete_backward(),
        KeyCode::Delete        => app.editor.buf_mut().delete_forward(),
        KeyCode::Tab           => app.editor.buf_mut().insert_tab(cfg.tab_size, cfg.use_spaces),
        KeyCode::Home          => app.editor.buf_mut().move_line_start(),
        KeyCode::End           => app.editor.buf_mut().move_line_end(),
        KeyCode::Left          => app.editor.buf_mut().move_left(),
        KeyCode::Right         => app.editor.buf_mut().move_right(),
        KeyCode::Up            => { app.editor.buf_mut().move_up(1); app.mode = Mode::Normal; }
        KeyCode::Down          => { app.editor.buf_mut().move_down(1); app.mode = Mode::Normal; }
        KeyCode::PageUp        => app.editor.buf_mut().move_page_up(20),
        KeyCode::PageDown      => app.editor.buf_mut().move_page_down(20),
        _ => {}
    }
    let h = 24;
    app.editor.buf_mut().scroll_to_cursor(h);
}


/// Handles keyboard inputs when the application layer is configured in Command mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn command_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc           => { app.prompt_input.clear(); app.mode = Mode::Normal; }
        KeyCode::Enter         => {
            let cmd = app.prompt_input.trim().to_string();
            app.prompt_input.clear();
            app.mode = Mode::Normal;
            execute_colon_cmd(app, &cmd);
        }
        KeyCode::Char(c)       => app.prompt_input.push(c),
        KeyCode::Backspace     => { app.prompt_input.pop(); }
        _ => {}
    }
}


/// Evaluates and processes textual console instructions written inside the command bar.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `cmd` - The trimmed instruction string slice to execute.
fn execute_colon_cmd(app: &mut App, cmd: &str) {
    match cmd {
        "w" | "write"          => app.save_file(),
        "q" | "quit"           => app.try_quit(),
        "wq" | "x"             => { app.save_file(); app.try_quit(); }
        "q!" | "quit!"         => app.should_quit = true,
        "wq!"                  => { app.save_file(); app.should_quit = true; }
        s if s.starts_with("e ") => {
            let path = std::path::Path::new(s[2..].trim());
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


/// Handles keyboard inputs when the application layer is configured in Search mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn search_mode(app: &mut App, key: KeyEvent) {
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
        KeyCode::Backspace => {
            app.prompt_input.pop();
        }
        _ => {}
    }
}


/// Handles keyboard inputs when the application layer is configured in Go-To-Line mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn goto_line_mode(app: &mut App, key: KeyEvent) {
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


/// Handles keyboard inputs when the application layer is configured in Command Palette mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn palette_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc           => { app.command_palette.reset(); app.mode = Mode::Normal; }
        KeyCode::Enter         => {
            if let Some(id) = app.command_palette.selected_id() {
                let id = id.to_string();
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


/// Handles keyboard inputs when the application layer is configured in File Tree mode.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `key` - The keyboard input event containing key modifiers and parameters.
fn filetree_mode(app: &mut App, key: KeyEvent) {
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


/// Handles and parses incoming mouse movement and scroll metrics.
///
/// # Arguments
///
/// * `app` - Mutable reference to the central application state engine instance.
/// * `mouse` - The peripheral mouse interaction event payload.
fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.editor.buf_mut().move_up(3),
        MouseEventKind::ScrollDown => app.editor.buf_mut().move_down(3),
        MouseEventKind::Down(MouseButton::Left) => {
            let mouse_row_pos: usize = mouse.row as usize;
            app.editor.buf_mut().goto_line(mouse_row_pos.saturating_sub(1), 24);
        },
        MouseEventKind::Drag(MouseButton::Left) => {
            todo!()
        },
        _ => {}
    }
}