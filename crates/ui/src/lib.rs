//! UI crate – the entire rendering pipeline.
//!
//! Layout (dynamic, togglable panels):
//!
//! ┌─[tab bar]──────────────────────────────────────────────────┐
//! │ ┌─[file tree]──┐ ┌─[editor pane]──────────────────────┐  │
//! │ │              │ │  (syntax-highlighted text + gutter) │  │
//! │ │              │ │                                     │  │
//! │ └──────────────┘ └─────────────────────────────────────┘  │
//! │ ┌─[terminal panel]───────────────────────────────────────┐ │
//! │ │  (stub – future pty integration)                       │ │
//! │ └─────────────────────────────────────────────────────────┘ │
//! ├─[status bar]────────────────────────────────────────────────┤
//! └─────────────────────────────────────────────────────────────┘
//!
//! [`Ui::render`] is the single public entry point.


use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use app::{App, ComponentSize, Mode};
use buffer::DiagSeverity;


mod colors {
    use ratatui::style::Color;
    pub const BG:         Color = Color::Rgb(25,  29,  38);
    pub const BG_PANEL:   Color = Color::Rgb(30,  34,  44);
    pub const BG_ACTIVE:  Color = Color::Rgb(38,  44,  56);
    pub const BG_LINE_HL: Color = Color::Rgb(42,  48,  62);
    pub const SEL_BG:     Color = Color::Rgb(55,  75,  105);
    pub const FG:         Color = Color::Rgb(200, 210, 220);
    pub const FG_DIM:     Color = Color::Rgb(90,  105, 125);
    pub const FG_GUTTER:  Color = Color::Rgb(65,  80,  100);
    pub const ACCENT:     Color = Color::Rgb(80,  160, 220);
    pub const ACCENT2:    Color = Color::Rgb(100, 200, 140);
    pub const ERROR_COL:  Color = Color::Rgb(220, 70,  70);
    pub const WARN_COL:   Color = Color::Rgb(220, 180, 60);
    pub const RULER_COL:  Color = Color::Rgb(45,  50,  65);
}

use colors::*;


/// Zero-sized rendering coordinator.
///
/// Every rendering pass is a method on this type, keeping the public surface
/// minimal (`Ui::render`) while making the internal helpers easy to locate,
/// document, and test individually.
pub struct Ui;

impl Ui {
    /// Executes the full user-interface rendering pass across all togglable view panels.
    ///
    /// # Arguments
    ///
    /// * `frame` - The terminal screen layout execution frame engine buffer.
    /// * `app`   - Mutable reference to the central application execution engine instance.
    pub fn render(frame: &mut Frame, app: &mut App) {
        let area: Rect = frame.area();

        // Vertical layout: tabbar / body / [terminal] / [status bar]
        let v_constraints: Vec<Constraint> = {
            let mut c: Vec<Constraint> = vec![
                Constraint::Length(1), // tabbar
                Constraint::Min(3),    // body
            ];
            if app.show_terminal         { c.push(Constraint::Length(10)); }
            if app.config.ui.show_status_bar { c.push(Constraint::Length(1)); }
            c
        };

        let v_chunks: std::rc::Rc<[Rect]> = Layout::default()
            .direction(Direction::Vertical)
            .constraints(v_constraints)
            .split(area);

        let mut row: usize = 0;

        let tabbar_rect: Rect = v_chunks[row]; row += 1;
        Self::render_tabbar(frame, app, tabbar_rect);

        let body_area: Rect = v_chunks[row]; row += 1;

        // Horizontal layout: [file tree] / editor / [diagnostics]
        let h_constraints: Vec<Constraint> = {
            let mut c: Vec<Constraint> = vec![];
            if app.file_tree.is_some() { c.push(Constraint::Length(28)); }
            c.push(Constraint::Min(20));
            if app.show_diag            { c.push(Constraint::Length(30)); }
            c
        };

        let h_chunks: std::rc::Rc<[Rect]> = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(h_constraints)
            .split(body_area);

        let mut col: usize = 0;

        let file_tree_rect: Option<Rect> = if app.file_tree.is_some() {
            let r: Rect = h_chunks[col]; col += 1;
            Self::render_file_tree(frame, app, r);
            Some(r)
        } else {
            None
        };

        let editor_rect: Rect = h_chunks[col]; col += 1;
        Self::render_editor(frame, app, editor_rect);

        let diag_rect: Option<Rect> = if app.show_diag {
            let r: Rect = h_chunks[col];
            Self::render_diagnostics(frame, app, r);
            Some(r)
        } else {
            None
        };

        let terminal_rect: Option<Rect> = if app.show_terminal {
            let r: Rect = v_chunks[row]; row += 1;
            Self::render_terminal_panel(frame, r);
            Some(r)
        } else {
            None
        };

        let status_bar_rect: Option<Rect> = if app.config.ui.show_status_bar {
            let r: Rect = v_chunks[row];
            Self::render_status_bar(frame, app, r);
            Some(r)
        } else {
            None
        };

        // Overlays
        if app.mode == Mode::CommandPalette {
            app.command_palette.render(frame);
        }

        match app.mode {
            Mode::Search | Mode::GotoLine | Mode::Command | Mode::SaveAs => {
                Self::render_prompt(frame, app, area);
            }
            _ => {}
        }

        // Store measured layout sizes for the input crate 
        app.layout.screen      = Self::component_size(area);
        app.layout.tabbar      = Self::component_size(tabbar_rect);
        app.layout.editor      = Self::component_size(editor_rect);
        app.layout.file_tree   = file_tree_rect .map(Self::component_size).unwrap_or_default();
        app.layout.diagnostics = diag_rect      .map(Self::component_size).unwrap_or_default();
        app.layout.terminal    = terminal_rect  .map(Self::component_size).unwrap_or_default();
        app.layout.status_bar  = status_bar_rect.map(Self::component_size).unwrap_or_default();

        let palette_popup_h: u16 = area.height * 60 / 100;
        let palette_popup_w: u16 = area.width  * 55 / 100;
        app.layout.cmd_palette_list = ComponentSize {
            width:  palette_popup_w.saturating_sub(2),
            height: palette_popup_h.saturating_sub(5),
        };

        let _ = col;
        let _ = row;
    }

    /// Draws the top window tab-selection bar.
    fn render_tabbar(frame: &mut Frame, app: &App, area: Rect) {
        let mut spans: Vec<Span> = vec![];
        for (i, buf) in app.editor.buffers.iter().enumerate() {
            let is_active: bool = i == app.editor.active;
            let mod_flag: &str  = if buf.modified { "● " } else { "" };
            let label: String = format!(" {}{} ", mod_flag, buf.name);
            let style: Style = if is_active {
                Style::default().fg(Color::White).bg(BG_ACTIVE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(FG_DIM).bg(BG_PANEL)
            };
            spans.push(Span::styled(label, style));
            spans.push(Span::styled("│", Style::default().fg(FG_DIM).bg(BG_PANEL)));
        }
        let line: Line<'_>   = Line::from(spans);
        let w: Paragraph<'_> = Paragraph::new(line).style(Style::default().bg(BG_PANEL));
        frame.render_widget(w, area);
    }

    /// Renders the syntax-highlighted editor pane, gutter, cursor, and selection.
    ///
    /// # Performance
    ///
    /// Only the `visible_h` lines starting at `scroll_top` are collected and
    /// highlighted each frame.  Cost is O(viewport height) regardless of file
    /// size — previously it was O(scroll_top + viewport), growing linearly as
    /// the user scrolled.
    fn render_editor(frame: &mut Frame, app: &mut App, area: Rect) {
        let cfg: &config::EditorConfig = &app.config.editor;
        let buf: &buffer::Buffer       = &app.editor.buffers[app.editor.active];

        let scroll_top: usize = buf.scroll_top;
        let scroll_left: usize = buf.scroll_left;
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

        let visible_lines: Vec<String> = (scroll_top..scroll_top + visible_h)
            .map(|l: usize| buf.get_line(l))
            .collect();

        let ext: String = app.editor.active_extension();
        let highlighted: Vec<Vec<editor::HighlightedSpan>> =
            app.editor.highlighter.highlight(&visible_lines, &ext);

        for screen_row in 0..visible_h {
            let abs_line: usize = scroll_top + screen_row;
            let y: u16 = area.top() + screen_row as u16;
            let is_cur: bool = abs_line == cursor_line;
            let line_bg: Color = if is_cur && cfg.highlight_line { BG_LINE_HL } else { BG };

            // Fill full-width background for the row.
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
            let spans: &[editor::HighlightedSpan] = highlighted
                .get(screen_row)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            let line_diags: Vec<&buffer::Diagnostic> = buf.diags_on_line(abs_line);

            let mut logical_col: usize = 0;
            'span_loop: for span in spans {
                for ch in span.text.chars() {
                    if ch == '\n' || ch == '\r' { break 'span_loop; }

                    if logical_col >= scroll_left {
                        let screen_col: u16 = (logical_col - scroll_left) as u16;
                        let sx: u16 = text_x0 + screen_col;
                        if sx >= area.right() { break 'span_loop; }

                        let is_selected: bool = buf.in_selection(abs_line, logical_col);
                        let bg: Color = if is_selected {
                            SEL_BG
                        } else if cfg.ruler_column > 0 && logical_col == cfg.ruler_column {
                            RULER_COL
                        } else {
                            line_bg
                        };

                        let mut cell_style: Style = span.style.bg(bg);

                        if is_cur && logical_col == cursor_col {
                            cell_style = if in_insert {
                                Style::default().fg(BG).bg(ACCENT2).add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(BG).bg(Color::White).add_modifier(Modifier::BOLD)
                            };
                        }

                        if line_diags.iter().any(|d| d.col == logical_col) {
                            cell_style = cell_style.add_modifier(Modifier::UNDERLINED);
                        }

                        if let Some(cell) = frame.buffer_mut().cell_mut((sx, y)) {
                            cell.set_char(ch).set_style(cell_style);
                        }
                    }

                    logical_col += 1;
                }
            }

            // Draw cursor block when it falls past the last span character
            // (empty line, or cursor positioned after all text on the line).
            if is_cur && cursor_col >= logical_col && cursor_col >= scroll_left {
                let screen_col: u16 = (cursor_col - scroll_left) as u16;
                let sx: u16 = text_x0 + screen_col;
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
            if let Some(row_offset) = cursor_line.checked_sub(scroll_top) {
                if row_offset < visible_h {
                    let vis_col: usize = cursor_col.saturating_sub(scroll_left);
                    let cx: u16 = (area.left() + gutter_w + vis_col as u16)
                        .min(area.right().saturating_sub(1));
                    let cy: u16 = (area.top() + row_offset as u16)
                        .min(area.bottom().saturating_sub(1));
                    frame.set_cursor_position((cx, cy));
                }
            }
        }
    }

    /// Draws the filesystem tree sidebar.
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

        let Some(ft) = &app.file_tree else { return };

        let flat: Vec<(usize, &app::FileTreeNode)> = ft.visible_flat();
        let total: usize = flat.len();

        if total == 0 {
            let p: Paragraph<'_> = Paragraph::new("  loading…")
                .style(Style::default().fg(FG_DIM).bg(BG_PANEL));
            frame.render_widget(p, inner);
            return;
        }

        let visible_h:  usize = inner.height as usize;
        let scroll_top: usize = ft.scroll_top.min(total.saturating_sub(1));

        let items: Vec<ListItem> = flat
            .iter()
            .enumerate()
            .skip(scroll_top)
            .take(visible_h)
            .map(|(i, (depth, node))| {
                let is_sel: bool = i == ft.selected;

                let icon: &str = if node.entry.is_dir {
                    if node.loading       { "⊙ " }
                    else if node.expanded { "▼ " }
                    else                  { "▶ " }
                } else {
                    "  "
                };

                let (name_fg, icon_fg) = if node.entry.is_dir {
                    (ACCENT, ACCENT)
                } else {
                    (FG, FG_DIM)
                };

                let bg: Color = if is_sel { BG_ACTIVE } else { BG_PANEL };
                let name_style: Style = if is_sel {
                    Style::default().fg(Color::White).bg(bg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(name_fg).bg(bg)
                };

                let indent: String = "  ".repeat(*depth);
                Line::from(vec![
                    Span::styled(indent,                    Style::default().bg(bg)),
                    Span::styled(icon,  Style::default().fg(icon_fg).bg(bg)),
                    Span::styled(node.entry.name.as_str(), name_style),
                ])
            })
            .map(ListItem::new)
            .collect();

        let list: List<'_> = List::new(items).style(Style::default().bg(BG_PANEL));
        frame.render_widget(list, inner);
    }

    /// Renders the diagnostics sidebar (errors / warnings from the LSP).
    fn render_diagnostics(frame: &mut Frame, app: &App, area: Rect) {
        let block: Block<'_> = Block::default()
            .title(" Diagnostics ")
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(FG_DIM))
            .style(Style::default().bg(BG_PANEL));
        let inner: Rect = block.inner(area);
        frame.render_widget(block, area);

        let buf: &buffer::Buffer = app.editor.buf();
        let items: Vec<ListItem> = buf.diagnostics.iter().map(|d| {
            let (icon, col): (&str, Color) = match d.severity {
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

    /// Renders the stub integrated-terminal panel.
    fn render_terminal_panel(frame: &mut Frame, area: Rect) {
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
                Span::styled(
                    "(integrated terminal - coming soon, use Ctrl+T to toggle)",
                    Style::default().fg(FG_DIM),
                ),
            ])
        ]);
        frame.render_widget(hint, inner);
    }

    /// Renders the bottom status bar (mode indicator, filename, cursor position, undo flags).
    fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
        let buf: &buffer::Buffer = app.editor.buf();

        let (mode_str, mode_fg, mode_bg): (&str, Color, Color) = match &app.mode {
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

        let mode_style: Style =
            Style::default().fg(mode_fg).bg(mode_bg).add_modifier(Modifier::BOLD);
        for ch in mode_str.chars() {
            if x >= area.right() { break; }
            if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
                cell.set_char(ch).set_style(mode_style);
            }
            x += 1;
        }

        let mod_flag: &str   = if buf.modified { " ●" } else { "" };
        let fname: String    = format!("  {}{}", buf.name, mod_flag);
        let fn_style: Style  =
            Style::default().fg(if buf.modified { WARN_COL } else { FG }).bg(bar_bg);
        for ch in fname.chars() {
            if x >= area.right() { break; }
            if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
                cell.set_char(ch).set_style(fn_style);
            }
            x += 1;
        }

        if let Some(msg) = &app.message {
            let s: String        = format!("   {}", msg);
            let msg_style: Style = Style::default().fg(WARN_COL).bg(bar_bg);
            for ch in s.chars() {
                if x >= area.right() { break; }
                if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
                    cell.set_char(ch).set_style(msg_style);
                }
                x += 1;
            }
        }

        let ext: String      = app.editor.active_extension();
        let ext_disp: String = if ext.is_empty() { "text".to_string() } else { ext };
        let can_u: &str      = if buf.can_undo() { "U" } else { "-" };
        let can_r: &str      = if buf.can_redo() { "R" } else { "-" };
        let pos_str: String  = format!(
            " {} {}  {}/{}  {}:{} ",
            ext_disp,
            format!("[{}{}]", can_u, can_r),
            app.editor.active + 1,
            app.editor.buffers.len(),
            buf.cursor.line + 1,
            buf.cursor.col  + 1,
        );
        let pos_start: u16   = area.right().saturating_sub(pos_str.len() as u16);
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

    /// Renders the transient command / search / goto-line / save-as prompt overlay.
    fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
        let (prefix, prompt_bg): (&str, Color) = match app.mode {
            Mode::Search   => ("/  ",       Color::Rgb(60, 30, 70)),
            Mode::GotoLine => (": ",        Color::Rgb(40, 50, 70)),
            Mode::Command  => (": ",        Color::Rgb(40, 50, 70)),
            Mode::SaveAs   => ("Save As: ", Color::Rgb(20, 60, 50)),
            _ => return,
        };
        let h: u16  = area.bottom().saturating_sub(1);
        let r: Rect = Rect { x: area.left(), y: h, width: area.width, height: 1 };
        frame.render_widget(Clear, r);

        let text: String = format!("{}{}", prefix, app.prompt_input);
        let style: Style = Style::default().fg(Color::White).bg(prompt_bg);
        for (i, ch) in text.chars().enumerate() {
            if let Some(cell) = frame.buffer_mut().cell_mut((r.left() + i as u16, r.top())) {
                cell.set_char(ch).set_style(style);
            }
        }
        frame.set_cursor_position((r.left() + text.len() as u16, r.top()));
    }

    /// Converts a [`Rect`] into a [`ComponentSize`] for layout tracking.
    #[inline]
    fn component_size(r: Rect) -> ComponentSize {
        ComponentSize { width: r.width, height: r.height }
    }
}
