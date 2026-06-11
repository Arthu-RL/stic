//! Command palette – opened with Ctrl+P.
//!
//! Supports fuzzy filtering across the full command list. Each command has a
//! stable string `id` that the `input` handler uses to dispatch actions.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

#[derive(Debug, Clone)]
pub struct PaletteCmd {
    pub id: &'static str,
    pub label: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
}

macro_rules! cmd {
    ($id:expr, $lbl:expr, $key:expr, $cat:expr) => {
        PaletteCmd { id: $id, label: $lbl, shortcut: $key, category: $cat }
    };
}

pub static COMMANDS: &[PaletteCmd] = &[
    // File
    cmd!("new_file",         "New File",               "Ctrl+N",           "File"),
    cmd!("open_file",        "Open File…",             "Ctrl+O",           "File"),
    cmd!("save_file",        "Save File",              "Ctrl+S",           "File"),
    cmd!("save_as",          "Save File As…",          "",                 "File"),
    cmd!("close_tab",        "Close Tab",              "Ctrl+W",           "File"),
    cmd!("open_config",      "Open Configuration",     "",                 "File"),
    // Edit
    cmd!("undo",             "Undo",                   "Ctrl+Z",           "Edit"),
    cmd!("redo",             "Redo",                   "Ctrl+Y",           "Edit"),
    cmd!("find",             "Find in File",           "Ctrl+F",           "Edit"),
    cmd!("find_next",        "Find Next",              "F3",               "Edit"),
    cmd!("find_prev",        "Find Previous",          "Shift+F3",         "Edit"),
    cmd!("go_to_line",       "Go to Line…",            "Ctrl+G",           "Edit"),
    cmd!("duplicate_line",   "Duplicate Line",         "Ctrl+Shift+D",     "Edit"),
    cmd!("delete_line",      "Delete Line",            "Ctrl+Shift+K",     "Edit"),
    // View
    cmd!("toggle_file_tree", "Toggle File Tree",       "Ctrl+B",           "View"),
    cmd!("toggle_terminal",  "Toggle Terminal",        "Ctrl+T",           "View"),
    cmd!("toggle_diag",      "Toggle Diagnostics",     "Ctrl+D",           "View"),
    cmd!("next_tab",         "Next Tab",               "Alt+Right",        "View"),
    cmd!("prev_tab",         "Previous Tab",           "Alt+Left",         "View"),
    // LSP
    cmd!("go_to_def",        "Go to Definition",       "F12",              "LSP"),
    cmd!("find_refs",        "Find References",        "Shift+F12",        "LSP"),
    cmd!("hover_doc",        "Show Hover Documentation","K (Normal)",      "LSP"),
    cmd!("rename_symbol",    "Rename Symbol",          "F2",               "LSP"),
    cmd!("code_action",      "Code Actions",           "Ctrl+.",           "LSP"),
    cmd!("symbol_search",    "Search Symbols…",        "Ctrl+Shift+O",     "LSP"),
    // App
    cmd!("quit",             "Quit",                   "Ctrl+Q",           "App"),
    cmd!("force_quit",       "Save & Quit Application", "Ctrl+Shift+Q",     "App"),
];

/// A UI component managing text input, selection state, and fuzzy string filtering
/// over the global command list.
pub struct CommandPalette {
    pub input: String,
    pub selected: usize,
    filtered: Vec<usize>, // indices into COMMANDS
}

impl CommandPalette {
    /// Creates a new instance of `CommandPalette`.
    ///
    /// # Returns
    ///
    /// An empty, initialized `CommandPalette` with all commands visible by default.
    pub fn new() -> Self {
        let filtered = (0..COMMANDS.len()).collect();
        Self { input: String::new(), selected: 0, filtered }
    }

    /// Resets the input search text and moves the item selection back to the top.
    pub fn reset(&mut self) {
        self.input.clear();
        self.selected = 0;
        self.refilter();
    }

    /// Appends a character to the query string and updates the filtered selection list.
    ///
    /// # Arguments
    ///
    /// * `ch` - The character character typed by the user.
    pub fn push_char(&mut self, ch: char) {
        self.input.push(ch);
        self.selected = 0;
        self.refilter();
    }

    /// Removes the trailing character from the query string and updates the filtered selection list.
    pub fn pop_char(&mut self) {
        self.input.pop();
        self.selected = 0;
        self.refilter();
    }

    /// Navigates up by one item within the filtered list bounds.
    pub fn move_up(&mut self) {
        if self.selected > 0 { self.selected -= 1; }
    }

    /// Navigates down by one item within the filtered list bounds.
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() { self.selected += 1; }
    }

    /// Retrieves the identity key of the currently selected command.
    ///
    /// # Returns
    ///
    /// An `Option` reference holding the string slice id if a command is highlighted.
    pub fn selected_id(&self) -> Option<&'static str> {
        self.filtered.get(self.selected).map(|&i| COMMANDS[i].id)
    }

    /// Evaluates whether characters in a query appear in order inside a specific label.
    ///
    /// # Arguments
    ///
    /// * `label` - The full phrase or identifier string text being evaluated.
    /// * `query` - The target sequence string being typed.
    ///
    /// # Returns
    ///
    /// `true` if every character of the query string matches sequentially; otherwise, `false`.
    fn fuzzy_match(label: &str, query: &str) -> bool {
        if query.is_empty() { return true; }
        let mut chars = label.chars().flat_map(|c| c.to_lowercase());
        for qc in query.chars().flat_map(|c| c.to_lowercase()) {
            if !chars.by_ref().any(|c| c == qc) { return false; }
        }
        true
    }

    /// Updates internal command indices matching the user query across label, ID, and category fields.
    fn refilter(&mut self) {
        let q = self.input.as_str();
        self.filtered = (0..COMMANDS.len())
            .filter(|&i| {
                let cmd = &COMMANDS[i];
                Self::fuzzy_match(cmd.label, q)
                    || Self::fuzzy_match(cmd.id, q)
                    || Self::fuzzy_match(cmd.category, q)
            })
            .collect();
    }

    /// Renders the complete interactive popup panel layout over the current terminal screen frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - The application UI view buffer drawing handle.
    pub fn render(&self, frame: &mut Frame) {
        let area: Rect   = frame.area();
        let popup: Rect  = centered_rect(55, 60, area);

        frame.render_widget(Clear, popup);

        let outer: Block<'_> = Block::default()
            .title(" Command Palette ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Rgb(25, 29, 38)));

        let inner: Rect = outer.inner(popup);
        frame.render_widget(outer, popup);

        let layout: std::rc::Rc<[Rect]> = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        let input_block: Block<'_> = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(80, 100, 130)));
        let prompt: String = format!("> {}", self.input);
        let input_widget: Paragraph<'_> = Paragraph::new(prompt)
            .style(Style::default().fg(Color::White))
            .block(input_block);
        frame.render_widget(input_widget, layout[0]);

        let items: Vec<ListItem> = self.filtered.iter().enumerate().map(|(pos, &idx)| {
            let cmd: &PaletteCmd = &COMMANDS[idx];
            let is_sel: bool = pos == self.selected;
            
            let label_style: Style = if is_sel {
                Style::default().fg(Color::White).bg(Color::Rgb(50, 80, 120)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(200, 210, 220))
            };
            let hint_style: Style = Style::default().fg(Color::Rgb(100, 120, 150));
            let cat_style: Style  = Style::default().fg(Color::Rgb(80, 100, 130));

            let prefix: &str = if is_sel { "▶ " } else { "  " };

            let line: Line<'_> = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!("[{}]  ", cmd.category), cat_style),
                Span::styled(format!("{:<33}", cmd.label),    label_style),
                Span::styled(cmd.shortcut.to_string(),        hint_style),
            ]);

            ListItem::new(line).style(if is_sel {
                Style::default().bg(Color::Rgb(35, 50, 70))
            } else {
                Style::default()
            })
        }).collect();

        let list: List<'_> = List::new(items)
            .style(Style::default().bg(Color::Rgb(25, 29, 38)));
        frame.render_widget(list, layout[1]);

        frame.set_cursor_position((
            layout[0].x + 1 + 2 + self.input.len() as u16, // 1=border, 2="> "
            layout[0].y + 1,
        ));
    }
}

impl Default for CommandPalette {
    fn default() -> Self { Self::new() }
}

/// Generates a symmetrically centered sub-rectangle boundary space.
///
/// # Arguments
///
/// * `pct_x` - Target layout width as a percentage of overall workspace container.
/// * `pct_y` - Target layout height as a percentage of overall workspace container.
/// * `r` - Base workspace boundaries rectangle layout wrapper.
///
/// # Returns
///
/// A calculated centered nested `Rect` target coordinates layout structure.
fn centered_rect(pct_x: u16, pct_y: u16, r: Rect) -> Rect {
    let v: std::rc::Rc<[Rect]> = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(v[1])[1]
}