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
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use app::{App, ComponentSize, Mode};
use buffer::DiagSeverity;


mod colors {
    use ratatui::style::Color;

    // Surfaces, darkest (canvas) to most elevated (floating popups). Each
    // step up is a deliberate, small lightness bump so panels read as
    // physically stacked rather than just "a different gray".
    pub const BG:           Color = Color::Rgb(16, 18, 24);
    pub const BG_PANEL:     Color = Color::Rgb(20, 23, 31);
    /// Slightly elevated above `BG_PANEL`, used for floating popups (hover
    /// docs, completions, command palette) so they read as "above" the
    /// panels beneath them.
    pub const BG_POPUP:     Color = Color::Rgb(27, 31, 42);
    pub const BG_ACTIVE:    Color = Color::Rgb(30, 35, 47);
    pub const BG_LINE_HL:   Color = Color::Rgb(24, 28, 38);
    /// Recessed well behind the integrated terminal — darker than `BG` so
    /// the shell reads as its own surface, tucked below the editor.
    pub const BG_TERMINAL:  Color = Color::Rgb(11, 13, 18);
    /// Darkest surface in the app; anchors the bottom status bar.
    pub const BG_STATUSBAR: Color = Color::Rgb(12, 14, 19);

    pub const SEL_BG:     Color = Color::Rgb(53,  82,  126);
    pub const FG:         Color = Color::Rgb(202, 211, 245);
    /// Reserved for the few things that should visually "pop": the line:col
    /// counter, bright popup titles, active-tab text.
    pub const FG_BRIGHT:  Color = Color::Rgb(228, 233, 250);
    pub const FG_DIM:     Color = Color::Rgb(97,  107, 130);
    pub const FG_GUTTER:  Color = Color::Rgb(68,  78,  100);

    pub const ACCENT:     Color = Color::Rgb(122, 162, 247);
    pub const ACCENT2:    Color = Color::Rgb(115, 218, 202);
    pub const WARN_COL:   Color = Color::Rgb(224, 175, 104);
    pub const ERROR_COL:  Color = Color::Rgb(240, 113, 120);
    pub const INFO_COL:   Color = Color::Rgb(158, 177, 240);
    // Additional hues used to give each editor mode its own distinct,
    // harmonious badge color in the status bar (see `Ui::mode_badge`).
    pub const MAGENTA:    Color = Color::Rgb(187, 154, 247);
    pub const GREEN:      Color = Color::Rgb(158, 206, 106);
    pub const ORANGE:     Color = Color::Rgb(224, 138, 90);
    pub const CYAN:       Color = Color::Rgb(94,  195, 214);
    pub const EMERALD:    Color = Color::Rgb(80,  200, 160);
    pub const SLATE:      Color = Color::Rgb(42,  47,  61);

    pub const RULER_COL:  Color = Color::Rgb(30, 34, 46);
    /// Subtle separators between status-bar segments and panel dividers.
    pub const DIVIDER:    Color = Color::Rgb(40, 45, 58);
    /// Unfocused panel border — quieter than `FG_DIM` so chrome recedes
    /// behind text of the same dimness rather than competing with it.
    pub const BORDER:     Color = Color::Rgb(46, 52, 68);
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

        // Horizontal layout: [file tree] / editor
        let h_constraints: Vec<Constraint> = {
            let mut c: Vec<Constraint> = vec![];
            if app.file_tree.is_some() { c.push(Constraint::Length(28)); }
            c.push(Constraint::Min(20));
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

        let terminal_rect: Option<Rect> = if app.show_terminal {
            let r: Rect = v_chunks[row]; row += 1;
            Self::render_terminal_panel(frame, app, r);
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

        // Floating overlays: hover doc and completion popup.
        Self::render_hover_overlay(frame, app, area);
        Self::render_completions_popup(frame, app);

        // Store measured layout sizes for the input crate 
        app.layout.screen      = Self::component_size(area);
        app.layout.tabbar      = Self::component_size(tabbar_rect);
        app.layout.editor      = Self::component_size(editor_rect);
        app.layout.file_tree   = file_tree_rect .map(Self::component_size).unwrap_or_default();
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
    ///
    /// Each tab's rendered text width must stay in lock-step with
    /// [`input::tab_index_at_col`]'s layout assumptions (`" " + "● "? + name + " "`)
    /// — only `Style` (color/modifiers) varies below, never the text content.
    fn render_tabbar(frame: &mut Frame, app: &App, area: Rect) {
        let mut spans: Vec<Span> = vec![];
        for (i, buf) in app.editor.buffers.iter().enumerate() {
            let is_active: bool = i == app.editor.active;

            let base_style: Style = if is_active {
                Style::default().fg(FG_BRIGHT).bg(BG_ACTIVE)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                    .underline_color(ACCENT)
            } else {
                Style::default().fg(FG_DIM).bg(BG_PANEL)
            };
            let dot_style: Style = base_style.fg(WARN_COL);

            spans.push(Span::styled(" ", base_style));
            if buf.modified {
                spans.push(Span::styled("● ", dot_style));
            }
            spans.push(Span::styled(format!("{} ", buf.name), base_style));
            spans.push(Span::styled("│", Style::default().fg(DIVIDER).bg(BG_PANEL)));
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
            let line_diags: Vec<&buffer::Diagnostic> = buf.diags_on_line(abs_line);

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

                // Worst-severity marker in the gutter's trailing column, so
                // lines with problems are spottable without scanning text.
                if let Some(color) = Self::worst_diag_color(&line_diags) {
                    let marker_col: u16 = area.left() + gutter_w - 1;
                    if let Some(cell) = frame.buffer_mut().cell_mut((marker_col, y)) {
                        cell.set_char('▎').set_style(Style::default().fg(color).bg(line_bg));
                    }
                }
            }

            let text_x0: u16 = area.left() + gutter_w;
            let spans: &[editor::HighlightedSpan] = highlighted
                .get(screen_row)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

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
                                Style::default().fg(BG).bg(FG_BRIGHT).add_modifier(Modifier::BOLD)
                            };
                        }

                        if let Some(d) = line_diags.iter().find(|d| d.col == logical_col) {
                            cell_style = cell_style
                                .add_modifier(Modifier::UNDERLINED)
                                .underline_color(Self::diag_severity_color(&d.severity));
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
                        Style::default().fg(BG).bg(FG_BRIGHT)
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
    ///
    /// Uses a `TOP` border (in addition to the existing `RIGHT` divider) so
    /// the " Files " title is actually visible above the list — previously
    /// the block had no top edge, so the list was rendered directly over the
    /// title row and the input crate's click math (which already assumed a
    /// header row via `saturating_sub(2)`) silently selected the row above
    /// whatever was clicked.
    fn render_file_tree(frame: &mut Frame, app: &App, area: Rect) {
        let focused: bool = app.mode == Mode::FileTree;
        let border_style: Style = if focused {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(BORDER)
        };
        let block: Block<'_> = Block::default()
            .title(" Files ")
            .title_style(Style::default().fg(if focused { ACCENT } else { FG_DIM }).add_modifier(Modifier::BOLD))
            .borders(Borders::TOP | Borders::RIGHT)
            .border_type(BorderType::Rounded)
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
                let bg: Color = if is_sel { BG_ACTIVE } else { BG_PANEL };

                let (icon, icon_fg): (&str, Color) = if node.entry.is_dir {
                    let glyph: &str = if node.loading       { "⊙ " }
                                       else if node.expanded { "▼ " }
                                       else                  { "▶ " };
                    (glyph, ACCENT)
                } else {
                    Self::file_glyph(&node.entry.name)
                };

                let name_fg: Color = if node.entry.is_dir { ACCENT } else { FG };
                let name_style: Style = if is_sel {
                    Style::default().fg(FG_BRIGHT).bg(bg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(name_fg).bg(bg)
                };

                // Left accent bar echoes the selected-row convention from the
                // editor gutter, so the highlighted entry is unmistakable
                // even before reading its text.
                let sel_bar: &str = if is_sel { "▎" } else { " " };
                let sel_bar_style: Style = Style::default()
                    .fg(if is_sel { ACCENT } else { bg })
                    .bg(bg);

                let indent: String = "  ".repeat(*depth);
                Line::from(vec![
                    Span::styled(sel_bar,                   sel_bar_style),
                    Span::styled(indent,                    Style::default().bg(bg)),
                    Span::styled(icon,  Style::default().fg(icon_fg).bg(bg)),
                    Span::styled(node.entry.name.as_str(), name_style),
                ])
            })
            .map(ListItem::new)
            .collect();

        let list: List<'_> = List::new(items).style(Style::default().bg(BG_PANEL));
        frame.render_widget(list, inner);

        if total > visible_h {
            let sb_rect = Rect { x: area.right().saturating_sub(1), y: inner.y, width: 1, height: inner.height };
            let mut sb_state = ScrollbarState::new(total).position(scroll_top);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::default().fg(DIVIDER).bg(BG_PANEL))
                .thumb_style(Style::default().fg(ACCENT));
            frame.render_stateful_widget(scrollbar, sb_rect, &mut sb_state);
        }
    }

    /// Renders the integrated PTY-backed terminal panel.
    ///
    /// Delegates the actual screen contents to [`terminal::PtySession::render`]
    /// once a shell has been spawned via `Ctrl+T`; the border is highlighted
    /// while [`Mode::Terminal`] has keyboard focus.
    fn render_terminal_panel(frame: &mut Frame, app: &mut App, area: Rect) {
        let focused: bool = app.mode == Mode::Terminal;
        let border_style: Style = if focused {
            Style::default().fg(ACCENT)
        } else {
            Style::default().fg(BORDER)
        };
        let block: Block<'_> = Block::default()
            .title(" Terminal ")
            .title_style(Style::default().fg(if focused { ACCENT } else { FG_DIM }).add_modifier(Modifier::BOLD))
            .borders(Borders::TOP)
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .style(Style::default().bg(BG_TERMINAL));
        let inner: Rect = block.inner(area);
        frame.render_widget(block, area);

        if let Some(term) = &mut app.terminal {
            term.resize(inner.height, inner.width);
            term.render(frame, inner, focused);
        } else {
            let hint: Paragraph<'_> = Paragraph::new(vec![
                Line::from(vec![
                    Span::styled("$ ", Style::default().fg(ACCENT2).add_modifier(Modifier::BOLD)),
                    Span::styled(
                        "press Ctrl+T to start a shell",
                        Style::default().fg(FG_DIM),
                    ),
                ])
            ]);
            frame.render_widget(hint, inner);
        }
    }

    /// Renders the bottom status bar (mode indicator, filename, cursor position, undo flags).
    fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
        let buf: &buffer::Buffer = app.editor.buf();

        // Every mode gets its own hue from the shared palette so the badge
        // reads as one coherent system rather than arbitrary bright colors;
        // `Terminal` alone uses a neutral slate + bright text since it's a
        // "you're outside the editor" state, not an editing mode.
        let (mode_str, mode_fg, mode_bg): (&str, Color, Color) = match &app.mode {
            Mode::Normal         => (" NORMAL  ", BG, ACCENT),
            Mode::Insert         => (" INSERT  ", BG, ACCENT2),
            Mode::Command        => (" COMMAND ", BG, WARN_COL),
            Mode::Search         => (" SEARCH  ", BG, MAGENTA),
            Mode::GotoLine       => (" GOTO    ", BG, ORANGE),
            Mode::CommandPalette => (" PALETTE ", BG, CYAN),
            Mode::FileTree       => (" TREE    ", BG, GREEN),
            Mode::SaveAs         => (" SAVE AS ", BG, EMERALD),
            Mode::Terminal       => (" TERMINAL", FG_BRIGHT, SLATE),
        };

        let bar_bg: Color = BG_STATUSBAR;

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
            let div: String      = "  │ ".to_string();
            let div_style: Style = Style::default().fg(DIVIDER).bg(bar_bg);
            for ch in div.chars() {
                if x >= area.right() { break; }
                if let Some(cell) = frame.buffer_mut().cell_mut((x, area.top())) {
                    cell.set_char(ch).set_style(div_style);
                }
                x += 1;
            }

            let s: String         = format!("▸ {}", msg);
            let msg_style: Style  = Style::default().fg(WARN_COL).bg(bar_bg);
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
        let lsp_on: bool     = app.lsp_available();

        let div_style: Style  = Style::default().fg(DIVIDER).bg(bar_bg);
        let dim_style: Style  = Style::default().fg(FG_DIM).bg(bar_bg);
        let lsp_style: Style  = Style::default()
            .fg(if lsp_on { ACCENT2 } else { FG_DIM })
            .bg(bar_bg);

        // Right-aligned cluster: LSP indicator │ ext │ undo/redo │ tab N/M │ line:col
        let segments: [(String, Style); 9] = [
            (format!(" {} LSP ", if lsp_on { "●" } else { "○" }), lsp_style),
            ("│ ".into(),                                          div_style),
            (format!("{} ", ext_disp),                             dim_style),
            ("│ ".into(),                                          div_style),
            (format!("[{can_u}{can_r}] "),                         dim_style),
            ("│ ".into(),                                          div_style),
            (format!("{}/{} ", app.editor.active + 1, app.editor.buffers.len()), dim_style),
            ("│ ".into(),                                          div_style),
            (format!("{}:{} ", buf.cursor.line + 1, buf.cursor.col + 1), Style::default().fg(FG_BRIGHT).bg(bar_bg).add_modifier(Modifier::BOLD)),
        ];

        let total_w: u16   = segments.iter().map(|(s, _)| s.chars().count() as u16).sum();
        let mut px: u16    = area.right().saturating_sub(total_w);
        for (seg, style) in &segments {
            for ch in seg.chars() {
                if px < area.right() {
                    if let Some(cell) = frame.buffer_mut().cell_mut((px, area.top())) {
                        cell.set_char(ch).set_style(*style);
                    }
                }
                px += 1;
            }
        }
    }

    /// Renders the transient command / search / goto-line / save-as prompt overlay.
    fn render_prompt(frame: &mut Frame, app: &App, area: Rect) {
        let (prefix, prompt_bg): (&str, Color) = match app.mode {
            Mode::Search   => ("/  ",       Color::Rgb(38, 26, 48)),
            Mode::GotoLine => (": ",        Color::Rgb(22, 30, 46)),
            Mode::Command  => (": ",        Color::Rgb(22, 30, 46)),
            Mode::SaveAs   => ("Save As: ", Color::Rgb(15, 38, 34)),
            _ => return,
        };
        let h: u16  = area.bottom().saturating_sub(1);
        let r: Rect = Rect { x: area.left(), y: h, width: area.width, height: 1 };
        frame.render_widget(Clear, r);

        let text: String = format!("{}{}", prefix, app.prompt_input);
        let style: Style = Style::default().fg(FG_BRIGHT).bg(prompt_bg);
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

    /// Maps a diagnostic severity onto its display color, shared by the
    /// inline underline and the gutter marker so the two always agree.
    fn diag_severity_color(sev: &DiagSeverity) -> Color {
        match sev {
            DiagSeverity::Error   => ERROR_COL,
            DiagSeverity::Warning => WARN_COL,
            DiagSeverity::Info    => INFO_COL,
            DiagSeverity::Hint    => FG_DIM,
        }
    }

    /// Returns the color for the most severe diagnostic on a line, if any,
    /// for the gutter marker (`Error` > `Warning` > `Info` > `Hint`).
    fn worst_diag_color(diags: &[&buffer::Diagnostic]) -> Option<Color> {
        diags.iter()
            .map(|d| &d.severity)
            .min_by_key(|sev| match sev {
                DiagSeverity::Error   => 0,
                DiagSeverity::Warning => 1,
                DiagSeverity::Info    => 2,
                DiagSeverity::Hint    => 3,
            })
            .map(Self::diag_severity_color)
    }

    /// Maps a file-tree entry's name onto a small colored glyph, giving the
    /// panel a lightweight "file type at a glance" cue without needing a
    /// Nerd Font — every symbol here is plain Unicode.
    fn file_glyph(name: &str) -> (&'static str, Color) {
        let ext: String = name.rsplit('.').next().unwrap_or("").to_lowercase();
        match ext.as_str() {
            "rs"                                                          => ("● ", ORANGE),
            "toml" | "yaml" | "yml" | "ini" | "cfg" | "lock"               => ("● ", FG_DIM),
            "json"                                                        => ("● ", WARN_COL),
            "md" | "txt" | "rst"                                          => ("● ", INFO_COL),
            "py" | "js" | "ts" | "jsx" | "tsx" | "go" | "c" | "cpp" | "h"
                | "hpp" | "lua" | "sh" | "rb" | "java"                    => ("● ", ACCENT),
            _                                                             => ("· ", FG_DIM),
        }
    }

    /// Returns the cursor's screen `(x, y)` position within the full terminal area.
    ///
    /// Row 0 is the tab bar; the editor body starts at row 1.  The gutter and
    /// file-tree panel widths are added to derive the correct column.
    fn cursor_screen_pos(app: &App) -> (u16, u16) {
        let buf        = app.editor.buf();
        let gutter_w: u16 = if app.config.editor.line_numbers {
            (buf.line_count().to_string().len().max(3) + 2) as u16
        } else {
            0
        };
        let ft_w:   u16 = app.layout.file_tree.width;
        let row_off: u16 = buf.cursor.line.saturating_sub(buf.scroll_top) as u16;
        let col_off: u16 = buf.cursor.col.saturating_sub(buf.scroll_left) as u16;
        let cx: u16 = ft_w + gutter_w + col_off;
        let cy: u16 = 1 + row_off; // +1 for the tab bar
        (cx, cy)
    }

    /// Renders the LSP hover-documentation floating overlay.
    ///
    /// The popup appears above the cursor when there is room, otherwise below.
    /// It is dismissed whenever `hover.visible` is `false`.
    fn render_hover_overlay(frame: &mut Frame, app: &App, area: Rect) {
        if !app.hover.visible || app.hover.content.is_empty() {
            return;
        }

        const MAX_LINES: usize = 14;
        let (cx, cy) = Self::cursor_screen_pos(app);

        let total_lines: usize = app.hover.content.lines().count();
        let lines: Vec<&str>   = app.hover.content.lines().take(MAX_LINES).collect();
        if lines.is_empty() { return; }
        let truncated: bool = total_lines > MAX_LINES;

        let popup_h: u16 = lines.len() as u16 + if truncated { 1 } else { 0 } + 2;
        let popup_w: u16 = lines.iter()
            .map(|l| l.len() as u16)
            .max()
            .unwrap_or(20)
            .min(area.width.saturating_sub(4))
            .max(20);

        let y: u16 = if cy >= popup_h + 1 {
            cy.saturating_sub(popup_h + 1)
        } else {
            cy.saturating_add(1).min(area.bottom().saturating_sub(popup_h))
        };
        let x: u16 = cx.min(area.right().saturating_sub(popup_w));

        if popup_h == 0 || popup_w == 0 || y >= area.bottom() || x >= area.right() {
            return;
        }

        let popup_rect = Rect {
            x, y,
            width:  popup_w,
            height: popup_h,
        };

        frame.render_widget(Clear, popup_rect);

        let mut text: Vec<Line> = lines.iter()
            .map(|l| Line::from(Span::styled(*l, Style::default().fg(FG))))
            .collect();
        if truncated {
            let more: usize = total_lines - MAX_LINES;
            text.push(Line::from(Span::styled(
                format!("… {more} more line{}", if more == 1 { "" } else { "s" }),
                Style::default().fg(FG_DIM).add_modifier(Modifier::ITALIC),
            )));
        }

        let p = Paragraph::new(text)
            .block(Block::default()
                .title(" Docs ")
                .title_style(Style::default().fg(INFO_COL).add_modifier(Modifier::BOLD))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(INFO_COL))
                .style(Style::default().bg(BG_POPUP)));
        frame.render_widget(p, popup_rect);
    }

    /// Maps an LSP completion-kind badge (e.g. `"fn"`, `"var"`, `"kw"`) onto a
    /// distinguishing accent color; unrecognized/empty badges fall back to
    /// [`FG_DIM`]. See `completion_kind_label` in the `lsp` crate for the
    /// full set of short codes this matches.
    fn completion_kind_color(kind: &str) -> Color {
        match kind {
            "fn" | "mth" | "ctor"                    => ACCENT,
            "var" | "fld" | "prop" | "val"            => ACCENT2,
            "kw" | "op"                                => WARN_COL,
            "cls" | "ifc" | "struct" | "enum" | "tp"  => INFO_COL,
            "const" | "em"                             => WARN_COL,
            _                                           => FG_DIM,
        }
    }

    /// Renders the LSP completion popup.
    ///
    /// Shows up to 8 items at a time in a floating list below (or above) the
    /// cursor, scrolling to keep the selected item in view when there are
    /// more. A short, color-coded kind badge (e.g. `fn`, `var`, `kw`) is
    /// shown on the right when available.
    ///
    /// Records the popup's on-screen rect and visible window into
    /// `app.completions` every frame so the `input` crate can hit-test mouse
    /// clicks/scrolls against it (see `InsertHandler::handle_mouse`).
    fn render_completions_popup(frame: &mut Frame, app: &mut App) {
        if !app.completions.visible || app.completions.items.is_empty() {
            app.completions.popup_rect = None;
            return;
        }
        let area: Rect = frame.area();

        let (cx, cy) = Self::cursor_screen_pos(app);

        const MAX_VISIBLE: usize = 8;
        let total: usize      = app.completions.items.len();
        let selected: usize   = app.completions.selected;
        let item_count: usize = total.min(MAX_VISIBLE);
        let popup_h: u16      = item_count as u16 + 2; // +2 for border
        let popup_w: u16      = 38_u16.min(area.width / 2).max(20);

        let y: u16 = if cy + popup_h + 1 <= area.bottom() {
            cy + 1
        } else {
            cy.saturating_sub(popup_h)
        };
        let x: u16 = cx.min(area.right().saturating_sub(popup_w));

        if popup_h == 0 || popup_w == 0 || y >= area.bottom() || x >= area.right() {
            app.completions.popup_rect = None;
            return;
        }

        let popup_rect = Rect { x, y, width: popup_w, height: popup_h };
        app.completions.popup_rect = Some(app::PopupRect {
            x: popup_rect.x, y: popup_rect.y, width: popup_rect.width, height: popup_rect.height,
        });

        frame.render_widget(Clear, popup_rect);

        let title: String = format!(" {}/{} ", selected + 1, total);
        let block: Block<'_> = Block::default()
            .title(title)
            .title_style(Style::default().fg(FG_DIM))
            .title_alignment(ratatui::layout::Alignment::Right)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG_POPUP));
        let inner = block.inner(popup_rect);
        frame.render_widget(block, popup_rect);

        // Keep `selected` inside the visible window, scrolling as needed.
        let max_start: usize = total.saturating_sub(MAX_VISIBLE);
        let start: usize     = selected.saturating_sub(MAX_VISIBLE.saturating_sub(1)).min(max_start);
        app.completions.visible_start = start;

        let has_scrollbar: bool = total > MAX_VISIBLE;
        let inner_w: usize = inner.width.saturating_sub(if has_scrollbar { 1 } else { 0 }) as usize;

        let items: Vec<ListItem> = app.completions.items.iter()
            .enumerate()
            .skip(start)
            .take(MAX_VISIBLE)
            .map(|(i, item)| {
                let is_sel: bool = i == selected;
                let bg: Color = if is_sel { BG_ACTIVE } else { BG_POPUP };
                let fg: Color = if is_sel { FG_BRIGHT } else { FG };
                let style: Style = Style::default().fg(fg).bg(bg);
                let marker: &str = if is_sel { "▎" } else { " " };

                // Compose marker + label + right-aligned, color-coded kind badge.
                let badge: &str = item.kind_label.as_deref().unwrap_or("");
                let label: &String = &item.label;

                let (label_text, badge_str): (String, String) = if badge.is_empty() {
                    (format!("{:<width$}", label, width = inner_w.saturating_sub(1)), String::new())
                } else {
                    let badge_str: String = format!("[{}]", badge);
                    let label_w: usize = inner_w.saturating_sub(badge_str.len() + 2);
                    (format!("{:<label_w$}", label, label_w = label_w), badge_str)
                };

                let label_style: Style = if is_sel { style.add_modifier(Modifier::BOLD) } else { style };
                let mut spans: Vec<Span<'_>> = vec![
                    Span::styled(marker, Style::default().fg(ACCENT).bg(bg)),
                    Span::styled(label_text, label_style),
                ];
                if !badge_str.is_empty() {
                    let badge_fg: Color = if is_sel { FG_BRIGHT } else { Self::completion_kind_color(badge) };
                    spans.push(Span::styled(badge_str, Style::default().fg(badge_fg).bg(bg)));
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);

        if has_scrollbar {
            let sb_rect = Rect { x: inner.right(), y: inner.y, width: 1, height: inner.height };
            let mut sb_state = ScrollbarState::new(total).position(selected);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::default().fg(DIVIDER).bg(BG_POPUP))
                .thumb_style(Style::default().fg(ACCENT));
            frame.render_stateful_widget(scrollbar, sb_rect, &mut sb_state);
        }
    }
}
