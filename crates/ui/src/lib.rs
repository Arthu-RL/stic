//! UI crate – the entire rendering pipeline.
//!
//! Layout (dynamic, togglable panels):
//!
//! ┌─[tab bar]──────────────────────────────────────────────────┐
//! │ ┌─[file tree]──┐ ┌─[editor pane]──────────────────────┐ │
//! │ │ │ │ (syntax-highlighted text + gutter) │ │
//! │ │ │ │ │ │
//! │ └──────────────┘ └─────────────────────────────────────┘ │
//! │ ┌─[terminal panel]───────────────────────────────────────┐ │
//! │ │ (stub – future pty integration) │ │
//! │ └─────────────────────────────────────────────────────────┘ │
//! ├─[status bar]────────────────────────────────────────────────┤
//! └─────────────────────────────────────────────────────────────┘
//!
//! The function [`render`] is the single public entry point.


use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};


use app::{App, Mode};
use buffer::DiagSeverity;


const BG:          Color = Color::Rgb(25,  29,  38);
const BG_PANEL:    Color = Color::Rgb(30,  34,  44);
const BG_ACTIVE:   Color = Color::Rgb(38,  44,  56);
const BG_LINE_HL:  Color = Color::Rgb(42,  48,  62);
const SEL_BG:      Color = Color::Rgb(55,  75,  105);
const FG:          Color = Color::Rgb(200, 210, 220);
const FG_DIM:      Color = Color::Rgb(90,  105, 125);
const FG_GUTTER:   Color = Color::Rgb(65,  80,  100);
const ACCENT:      Color = Color::Rgb(80,  160, 220);
const ACCENT2:     Color = Color::Rgb(100, 200, 140);
const ERROR_COL:   Color = Color::Rgb(220, 70,  70);
const WARN_COL:    Color = Color::Rgb(220, 180, 60);
const RULER_COL:   Color = Color::Rgb(45,  50,  65);


/// Executes the full user interface rendering pass across all togglable view panels.
///
/// # Arguments
///
/// * `frame` - The terminal screen layout execution frame engine buffer.
/// * `app` - Mutable reference to the central application execution engine instance.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area: Rect = frame.area();

    let editor_height: usize = compute_editor_height(area, app) as usize;
    app.editor.buf_mut().scroll_to_cursor(editor_height);

    let v_constraints: Vec<Constraint> = {
        let mut c: Vec<Constraint> = vec![
            Constraint::Length(1),
        ];
        c.push(Constraint::Min(3));
        if app.show_terminal {
            c.push(Constraint::Length(10));
        }
        if app.config.ui.show_status_bar {
            c.push(Constraint::Length(1));
        }
        c
    };

    let v_chunks: std::rc::Rc<[Rect]> = Layout::default()
        .direction(Direction::Vertical)
        .constraints(v_constraints)
        .split(area);

    let mut row: usize = 0;
    render_tabbar(frame, app, v_chunks[row]); row += 1;

    let body_area: Rect = v_chunks[row]; row += 1;

    let h_constraints: Vec<Constraint> = {
        let mut c: Vec<Constraint> = vec![];
        if app.file_tree.is_some() {
            c.push(Constraint::Length(28));
        }
        c.push(Constraint::Min(20));
        if app.show_diag {
            c.push(Constraint::Length(30));
        }
        c
    };

    let h_chunks: std::rc::Rc<[Rect]> = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(h_constraints)
        .split(body_area);

    let mut col: usize = 0;
    if app.file_tree.is_some() {
        render_file_tree(frame, app, h_chunks[col]);
        col += 1;
    }
    render_editor(frame, app, h_chunks[col]);
    col += 1;
    if app.show_diag {
        render_diagnostics(frame, app, h_chunks[col]);
    }

    if app.show_terminal {
        render_terminal_panel(frame, app, v_chunks[row]); row += 1;
    }
    if app.config.ui.show_status_bar {
        render_status_bar(frame, app, v_chunks[row]);
    }

    if app.mode == Mode::CommandPalette {
        app.command_palette.render(frame);
    }

    match app.mode {
        Mode::Search | Mode::GotoLine | Mode::Command | Mode::SaveAs => render_prompt(frame, app, area),
        _ => {}
    }
}


/// Draws the top window tab selection panel bar displaying active tracking workspaces.
///
/// # Arguments
///
/// * `frame` - The drawing workspace frame container.
/// * `app` - Top-level execution state model layer reference.
/// * `area` - Layout boundaries allocation slice rectangle.
fn render_tabbar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span> = vec![];
    for (i, buf) in app.editor.buffers.iter().enumerate() {
        let is_active: bool = i == app.editor.active;
        let mod_flag: &str  = if buf.modified { "● " } else { "" };
        let label: String     = format!(" {}{} ", mod_flag, buf.name);
        let style: Style = if is_active {
            Style::default().fg(Color::White).bg(BG_ACTIVE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(FG_DIM).bg(BG_PANEL)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::styled("│", Style::default().fg(FG_DIM).bg(BG_PANEL)));
    }
    let line: Line<'_> = Line::from(spans);
    let w: Paragraph<'_> = Paragraph::new(line).style(Style::default().bg(BG_PANEL));
    frame.render_widget(w, area);
}


/// Builds cell text buffer segments layout mappings to output file texts onto screens.
///
/// # Arguments
///
/// * `frame` - Current display cell matrix writing layout wrapper.
/// * `app` - Mutable application engine referencing target documents.
/// * `area` - Target display boundaries wrapper region coordinates.
fn render_editor(frame: &mut Frame, app: &mut App, area: Rect) {
    let cfg: &config::EditorConfig = &app.config.editor;
    let buf: &buffer::Buffer = &app.editor.buffers[app.editor.active];
    let scroll_top: usize = buf.scroll_top;
    let visible_h: usize = area.height as usize;
    let cursor_line: usize = buf.cursor.line;
    let cursor_col: usize = buf.cursor.col;
    let in_insert: bool = app.mode == Mode::Insert;

    let gutter_w: u16 = if cfg.line_numbers {
        let digits: usize = buf.line_count().to_string().len().max(3);
        (digits + 2) as u16
    } else {
        0
    };

    let all_lines_for_hl: Vec<String> = (0..=scroll_top + visible_h)
        .map(|l: usize| buf.get_line(l))
        .collect();

    let ext: String = app.editor.active_extension();
    let highlighted: Vec<Vec<editor::HighlightedSpan>> = app.editor.highlighter.highlight(&all_lines_for_hl, &ext);

    for screen_row in 0..visible_h {
        let abs_line: usize = scroll_top + screen_row;
        let y: u16 = area.top() + screen_row as u16;
        let is_cur: bool = abs_line == cursor_line;
        let line_bg: Color = if is_cur && cfg.highlight_line { BG_LINE_HL } else { BG };

        for x in area.left()..area.right() {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_bg(line_bg);
            }
        }

        if cfg.line_numbers && gutter_w > 0 {
            let num_str: String = if cfg.relative_numbers && !is_cur {
                let rel: usize = (abs_line as isize - cursor_line as isize).unsigned_abs();
                format!("{:>width$} ", rel, width = (gutter_w - 1) as usize)
            } else {
                format!("{:>width$} ", abs_line + 1, width = (gutter_w - 1) as usize)
            };
            let g_style: Style = if is_cur {
                Style::default().fg(ACCENT).bg(line_bg)
            } else {
                Style::default().fg(FG_GUTTER).bg(line_bg)
            };
            let mut gx: u16 = area.left();
            for ch in num_str.chars() {
                if gx >= area.left() + gutter_w { break; }
                if let Some(cell) = frame.buffer_mut().cell_mut((gx, y)) {
                    cell.set_char(ch).set_style(g_style);
                }
                gx += 1;
            }
        }

        let text_x0: u16 = area.left() + gutter_w;
        let spans: &[editor::HighlightedSpan] = highlighted.get(abs_line).map(|v: &Vec<editor::HighlightedSpan>| v.as_slice()).unwrap_or(&[]);

        let ruler: u16 = cfg.ruler_column as u16;
        let mut col_offset: u16 = 0u16;
        'span_loop: for span in spans {
            for ch in span.text.chars() {
                if ch == '\n' || ch == '\r' { break 'span_loop; }
                let sx: u16 = text_x0 + col_offset;
                if sx >= area.right() { break 'span_loop; }

                let is_selected: bool = buf.in_selection(abs_line, col_offset as usize);
                
                let bg: Color = if is_selected {
                    SEL_BG
                } else if ruler > 0 && col_offset == ruler {
                    RULER_COL
                } else {
                    line_bg
                };

                let mut cell_style: Style = span.style.bg(bg);

                let is_cursor_cell: bool = is_cur && col_offset as usize == cursor_col;
                if is_cursor_cell {
                    cell_style = if in_insert {
                        Style::default().fg(BG).bg(ACCENT2).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(BG).bg(Color::White).add_modifier(Modifier::BOLD)
                    };
                }

                let diag_on_col: bool = buf.diags_on_line(abs_line)
                    .iter()
                    .any(|d: &&buffer::Diagnostic| d.col == col_offset as usize);
                if diag_on_col {
                    cell_style = cell_style.add_modifier(Modifier::UNDERLINED);
                }

                if let Some(cell) = frame.buffer_mut().cell_mut((sx, y)) {
                    cell.set_char(ch).set_style(cell_style);
                }
                col_offset += 1;
            }
        }

        if is_cur && (cursor_col as u16) >= col_offset {
            let sx: u16 = text_x0 + cursor_col as u16;
            if sx < area.right() {
                let curs_style: Style = if in_insert {
                    Style::default().fg(BG).bg(ACCENT2)
                } else {
                    Style::default().fg(BG).bg(Color::White)
                };
                if let Some(cell) = frame.buffer_mut().cell_mut((sx, y)) {
                    cell.set_char(' ').set_style(curs_style);
                }
            }
        }
    }

    if app.mode != Mode::CommandPalette {
        let cx: u16 = (area.left() + gutter_w + cursor_col as u16).min(area.right().saturating_sub(1));
        let cy: u16 = (area.top() + (cursor_line - scroll_top) as u16).min(area.bottom().saturating_sub(1));
        frame.set_cursor_position((cx, cy));
    }
}


/// Draws filesystem list layouts mapping discovered file nodes onto left sidebars.
///
/// # Arguments
///
/// * `frame` - Terminal layout drawing canvas target window.
/// * `app` - Main context state pointer.
/// * `area` - Layout geometry parameters wrapper container.
fn render_file_tree(frame: &mut Frame, app: &App, area: Rect) {
    let focused: bool = app.mode == Mode::FileTree;
    let border_style: Style = if focused {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(FG_DIM)
    };
    let block: Block<'_> = Block::default()
        .title(" Files ")
        .borders(Borders::RIGHT)
        .border_style(border_style)
        .style(Style::default().bg(BG_PANEL));
    let inner: Rect = block.inner(area);
    frame.render_widget(block, area);

    if let Some(ft) = &app.file_tree {
        let items: Vec<ListItem> = ft.entries.iter().enumerate().map(|(i, (depth, name, _))| {
            let indent: String = "  ".repeat(*depth);
            let label: String  = format!("{}{}", indent, name);
            let style: Style  = if i == ft.selected {
                Style::default().fg(Color::White).bg(BG_ACTIVE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG)
            };
            ListItem::new(label).style(style)
        }).collect();
        let list: List<'_> = List::new(items).style(Style::default().bg(BG_PANEL));
        frame.render_widget(list, inner);
    }
}


/// Exposes diagnostic warning alert structures in a dedicated sidebar region.
///
/// # Arguments
///
/// * `frame` - Target workspace presentation drawing handles layout canvas.
/// * `app` - Core state context wrapper tracking active files.
/// * `area` - Boundary layout properties coordinate layout structure.
fn render_diagnostics(frame: &mut Frame, app: &App, area: Rect) {
    let block: Block<'_> = Block::default()
        .title(" Diagnostics ")
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(FG_DIM))
        .style(Style::default().bg(BG_PANEL));
    let inner: Rect = block.inner(area);
    frame.render_widget(block, area);

    let buf: &buffer::Buffer   = app.editor.buf();
    let items: Vec<ListItem> = buf.diagnostics.iter().map(|d| {
        let (icon, col) = match d.severity {
            DiagSeverity::Error   => ("✖", ERROR_COL),
            DiagSeverity::Warning => ("⚠", WARN_COL),
            DiagSeverity::Info    => ("ℹ", ACCENT),
            DiagSeverity::Hint    => ("·", FG_DIM),
        };
        let label: String = format!("{} {}:{} {}", icon, d.line + 1, d.col + 1, d.message);
        ListItem::new(label).style(Style::default().fg(col))
    }).collect();

    if items.is_empty() {
        let p: Paragraph<'_> = Paragraph::new("No diagnostics")
            .style(Style::default().fg(FG_DIM));
        frame.render_widget(p, inner);
    } else {
        let list: List<'_> = List::new(items).style(Style::default().bg(BG_PANEL));
        frame.render_widget(list, inner);
    }
}


/// Renders a terminal emulator layout frame panel space inside base workspaces.
///
/// # Arguments
///
/// * `frame` - Target render frame context canvas buffer handle.
/// * `app` - Core application context instance structure.
/// * `area` - Rectangular boundary dimensions configuration wrapper.
fn render_terminal_panel(frame: &mut Frame, _app: &App, area: Rect) {
    let block: Block<'_> = Block::default()
        .title(" Terminal ")
        .borders(Borders::TOP)
        .border_style(Style::default().fg(FG_DIM))
        .style(Style::default().bg(Color::Rgb(18, 20, 28)));
    let inner: Rect = block.inner(area);
    frame.render_widget(block, area);

    let hint: Paragraph<'_> = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("$ ", Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD)),
            Span::styled("(integrated terminal - coming soon, use Ctrl+T to toggle)", Style::default().fg(FG_DIM)),
        ])
    ]);
    frame.render_widget(hint, inner);
}


/// Computes status bar line layouts containing information regarding context modes, file names, positions, and logs.
///
/// # Arguments
///
/// * `frame` - Target execution view rendering canvas framework handle.
/// * `app` - Core state context tracker wrapper.
/// * `area` - Dimension target boundary specifications map structure.
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let buf: &buffer::Buffer = app.editor.buf();

    let (mode_str, mode_fg, mode_bg) = match &app.mode {
        Mode::Normal         => (" NORMAL  ", Color::Black, ACCENT),
        Mode::Insert         => (" INSERT  ", Color::Black, ACCENT2),
        Mode::Command        => (" COMMAND ", Color::Black, Color::Rgb(220, 180, 60)),
        Mode::Search         => (" SEARCH  ", Color::Black, Color::Rgb(200, 100, 200)),
        Mode::GotoLine       => (" GOTO    ", Color::Black, Color::Rgb(220, 130, 60)),
        Mode::CommandPalette => (" PALETTE ", Color::Black, Color::Rgb(100, 180, 220)),
        Mode::FileTree       => (" TREE    ", Color::Black, Color::Rgb(160, 200, 100)),
        Mode::SaveAs         => (" SAVE AS ", Color::Black, Color::Rgb(80,  200, 160)),
    };

    let bar_bg: Color = Color::Rgb(22, 25, 34);

    for x in area.left()..area.right() {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
            cell.set_char(' ').set_bg(bar_bg).set_fg(FG);
        }
    }

    let mut x: u16 = area.left();

    let mode_style: Style = Style::default().fg(mode_fg).bg(mode_bg).add_modifier(Modifier::BOLD);
    for ch in mode_str.chars() {
        if x >= area.right() { break; }
        if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
            cell.set_char(ch).set_style(mode_style);
        }
        x += 1;
    }

    let mod_flag: &str = if buf.modified { " ●" } else { "" };
    let fname: String = format!("  {}{}", buf.name, mod_flag);
    let fn_style: Style = Style::default().fg(if buf.modified { WARN_COL } else { FG }).bg(bar_bg);
    for ch in fname.chars() {
        if x >= area.right() { break; }
        if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
            cell.set_char(ch).set_style(fn_style);
        }
        x += 1;
    }

    if let Some(msg) = &app.message {
        let s: String = format!("   {}", msg);
        let msg_style: Style = Style::default().fg(WARN_COL).bg(bar_bg);
        for ch in s.chars() {
            if x >= area.right() { break; }
            if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
                cell.set_char(ch).set_style(msg_style);
            }
            x += 1;
        }
    }

    let ext: String = app.editor.active_extension();
    let ext_disp: String = if ext.is_empty() { "text".to_string() } else { ext };
    let can_u: &str = if buf.can_undo() { "U" } else { "-" };
    let can_r: &str = if buf.can_redo() { "R" } else { "-" };
    let pos_str: String  = format!(
        " {} {}  {}/{}  {}:{} ",
        ext_disp, format!("[{}{}]", can_u, can_r),
        app.editor.active + 1, app.editor.buffers.len(),
        buf.cursor.line + 1, buf.cursor.col + 1,
    );
    let pos_start: u16 = area.right().saturating_sub(pos_str.len() as u16);
    let pos_style: Style = Style::default().fg(FG_DIM).bg(bar_bg);
    for (i, ch) in pos_str.chars().enumerate() {
        let px: u16 = pos_start + i as u16;
        if px < area.right() {
            if let Some(cell) = frame.buffer_mut().cell_mut((px, area.top())) {
                cell.set_char(ch).set_style(pos_style);
            }
        }
    }
}


/// Displays transient interactive command lines overlaying base application screens.
///
/// # Arguments
///
/// * `frame` - Main application display context render frame.
/// * `app` - Global interface configuration memory state wrapper.
/// * `area` - Screen constraint values layout container parameters.
fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
    let (prefix, prompt_bg) = match app.mode {
        Mode::Search   => ("/  ",          Color::Rgb(60, 30,  70)),
        Mode::GotoLine => (": ",           Color::Rgb(40, 50,  70)),
        Mode::Command  => (": ",           Color::Rgb(40, 50,  70)),
        Mode::SaveAs   => ("Save As: ",    Color::Rgb(20, 60,  50)),
        _ => return,
    };
    let h: u16  = area.bottom().saturating_sub(1);
    let w: u16  = area.width;
    let r: Rect  = Rect { x: area.left(), y: h, width: w, height: 1 };
    frame.render_widget(Clear, r);

    let text: String  = format!("{}{}", prefix, app.prompt_input);
    let style = Style::default().fg(Color::White).bg(prompt_bg);
    for (i, ch) in text.chars().enumerate() {
        if let Some(cell) = frame.buffer_mut().cell_mut((r.left() + i as u16, r.top())) {
            cell.set_char(ch).set_style(style);
        }
    }
    frame.set_cursor_position((r.left() + text.len() as u16, r.top()));
}


/// Evaluates active visible panels to calculate remaining line height metrics.
///
/// # Arguments
///
/// * `area` - Full raw coordinate window wrapper parameters block.
/// * `app` - State memory model structure tracing visible properties.
///
/// # Returns
///
/// Calculated vertical dimension spans remaining for central workspaces.
fn compute_editor_height(area: Rect, app: &App) -> u16 {
    let mut h: u16 = area.height;
    h = h.saturating_sub(1);
    if app.config.ui.show_status_bar { h = h.saturating_sub(1); }
    if app.show_terminal { h = h.saturating_sub(10); }
    h
}