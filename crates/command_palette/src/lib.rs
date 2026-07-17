//! Command palette – opened with Ctrl+P.
//!
//! Supports fuzzy filtering across the full command list. Each command has a
//! stable string `id` that the `input` handler uses to dispatch actions.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

/// A single palette entry with a stable string `id` used for dispatch.
///
/// Owned `String` fields (rather than `&'static str`) allow commands to be
/// registered at runtime — e.g. by Lua plugins via `stic.register_command`.
#[derive(Debug, Clone)]
pub struct PaletteCmd {
    pub id: String,
    pub label: String,
    pub shortcut: String,
    pub category: String,
}

impl PaletteCmd {
    /// Builds a palette entry from its four display/dispatch fields.
    ///
    /// # Arguments
    ///
    /// * `id`       - Stable identifier matched by the command dispatcher.
    /// * `label`    - Human-readable name shown in the list.
    /// * `shortcut` - Display-only keybinding hint (may be empty).
    /// * `category` - Grouping tag rendered before the label.
    pub fn new(
        id:       impl Into<String>,
        label:    impl Into<String>,
        shortcut: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id:       id.into(),
            label:    label.into(),
            shortcut: shortcut.into(),
            category: category.into(),
        }
    }
}

/// Returns the built-in command set every palette starts with.
///
/// Runtime commands (e.g. from Lua plugins) are appended afterwards via
/// [`CommandPalette::add_command`].
fn builtin_commands() -> Vec<PaletteCmd> {
    let cmd = PaletteCmd::new;
    vec![
        // File
        cmd("new_file",         "New File",                "Ctrl+N",        "File"),
        cmd("open_file",        "Open File…",              "Ctrl+O",        "File"),
        cmd("save_file",        "Save File",               "Ctrl+S",        "File"),
        cmd("save_as",          "Save File As…",           "",              "File"),
        cmd("close_tab",        "Close Tab",               "Ctrl+W",        "File"),
        cmd("open_config",      "Open Configuration",      "",              "File"),
        // Edit
        cmd("undo",             "Undo",                    "Ctrl+Z",        "Edit"),
        cmd("redo",             "Redo",                    "Ctrl+Y",        "Edit"),
        cmd("find",             "Find in File",            "Ctrl+F",        "Edit"),
        cmd("find_next",        "Find Next",               "F3",            "Edit"),
        cmd("find_prev",        "Find Previous",           "Shift+F3",      "Edit"),
        cmd("go_to_line",       "Go to Line…",             "Ctrl+G",        "Edit"),
        cmd("duplicate_line",   "Duplicate Line",          "Ctrl+Shift+D",  "Edit"),
        cmd("delete_line",      "Delete Line",             "Ctrl+Shift+K",  "Edit"),
        // View
        cmd("toggle_file_tree", "Toggle File Tree",        "Ctrl+B",        "View"),
        cmd("toggle_terminal",  "Toggle Terminal",         "Ctrl+T",        "View"),
        cmd("close_terminal",   "Close Terminal Session",  "Ctrl+Shift+T",  "View"),
        cmd("next_tab",         "Next Tab",                "Alt+Right",     "View"),
        cmd("prev_tab",         "Previous Tab",            "Alt+Left",      "View"),
        // LSP
        cmd("go_to_def",        "Go to Definition",        "F12",           "LSP"),
        cmd("find_refs",        "Find References",         "Shift+F12",     "LSP"),
        cmd("hover_doc",        "Show Hover Documentation","K (Normal)",    "LSP"),
        cmd("rename_symbol",    "Rename Symbol",           "F2",            "LSP"),
        cmd("code_action",      "Code Actions",            "Ctrl+.",        "LSP"),
        cmd("symbol_search",    "Search Symbols…",         "Ctrl+Shift+O",  "LSP"),
        // App
        cmd("quit",             "Quit",                    "Ctrl+Q",        "App"),
        cmd("force_quit",       "Save & Quit Application", "Ctrl+Shift+Q",  "App"),
    ]
}

/// A UI component managing text input, selection state, and fuzzy string filtering
/// over the global command list.
pub struct CommandPalette {
    pub input:    String,
    pub selected: usize,
    /// First visible row in the list (vertical scroll offset).
    pub scroll_top: usize,
    /// Built-in commands plus any registered at runtime (e.g. by plugins).
    commands: Vec<PaletteCmd>,
    filtered: Vec<usize>, // indices into `commands`
}

impl CommandPalette {
    /// Creates a new instance of `CommandPalette`.
    ///
    /// # Returns
    ///
    /// An empty, initialized `CommandPalette` with all commands visible by default.
    pub fn new() -> Self {
        let commands: Vec<PaletteCmd> = builtin_commands();
        let filtered: Vec<usize> = (0..commands.len()).collect();
        Self { input: String::new(), selected: 0, scroll_top: 0, commands, filtered }
    }

    /// Registers a runtime command, replacing any existing entry with the
    /// same `id` so plugin reloads don't accumulate duplicates.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The palette entry to add or replace.
    pub fn add_command(&mut self, cmd: PaletteCmd) {
        match self.commands.iter_mut().find(|c| c.id == cmd.id) {
            Some(existing) => *existing = cmd,
            None           => self.commands.push(cmd),
        }
        self.refilter();
    }

    /// Resets the input search text and moves the item selection back to the top.
    pub fn reset(&mut self) {
        self.input.clear();
        self.selected   = 0;
        self.scroll_top = 0;
        self.refilter();
    }

    /// Appends a character to the query string and updates the filtered selection list.
    ///
    /// # Arguments
    ///
    /// * `ch` - The character character typed by the user.
    pub fn push_char(&mut self, ch: char) {
        self.input.push(ch);
        self.selected   = 0;
        self.scroll_top = 0;
        self.refilter();
    }

    /// Removes the trailing character from the query string and updates the filtered selection list.
    pub fn pop_char(&mut self) {
        self.input.pop();
        self.selected   = 0;
        self.scroll_top = 0;
        self.refilter();
    }

    /// Navigates up by one item within the filtered list bounds.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll_top {
                self.scroll_top = self.selected;
            }
        }
    }

    /// Navigates down by one item within the filtered list bounds.
    ///
    /// # Arguments
    ///
    /// * `visible_h` - Number of rows visible in the list area; used to advance
    ///   the scroll offset so the selected row is always on screen.
    pub fn move_down(&mut self, visible_h: usize) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
            let bottom: usize = self.scroll_top + visible_h.saturating_sub(1);
            if self.selected > bottom {
                self.scroll_top += 1;
            }
        }
    }

    /// Retrieves the identity key of the currently selected command.
    ///
    /// # Returns
    ///
    /// An `Option` reference holding the string slice id if a command is highlighted.
    pub fn selected_id(&self) -> Option<&str> {
        self.filtered.get(self.selected).map(|&i| self.commands[i].id.as_str())
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
        let q: &str = self.input.as_str();
        self.filtered = (0..self.commands.len())
            .filter(|&i| {
                let cmd: &PaletteCmd = &self.commands[i];
                Self::fuzzy_match(&cmd.label, q)
                    || Self::fuzzy_match(&cmd.id, q)
                    || Self::fuzzy_match(&cmd.category, q)
            })
            .collect();
    }

    /// Renders the complete interactive popup panel layout over the current terminal screen frame.
    ///
    /// Takes `&mut self` so it can keep `scroll_top` in sync with `selected`
    /// whenever the visible height changes (e.g. on terminal resize).
    ///
    /// # Arguments
    ///
    /// * `frame` - The application UI view buffer drawing handle.
    pub fn render(&mut self, frame: &mut Frame) {
        let area: Rect   = frame.area();
        let popup: Rect  = centered_rect(55, 60, area);

        frame.render_widget(Clear, popup);

        let outer: Block<'_> = Block::default()
            .title(" Command Palette ")
            .title_style(Style::default().fg(Color::Rgb(122, 162, 247)).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(122, 162, 247)))
            .style(Style::default().bg(Color::Rgb(27, 31, 42)));

        let inner: Rect = outer.inner(popup);
        frame.render_widget(outer, popup);

        let layout: std::rc::Rc<[Rect]> = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);

        let input_block: Block<'_> = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(76, 96, 132)));
        let prompt: String = format!("> {}", self.input);
        let input_widget: Paragraph<'_> = Paragraph::new(prompt)
            .style(Style::default().fg(Color::Rgb(228, 233, 250)))
            .block(input_block);
        frame.render_widget(input_widget, layout[0]);

        // Clamp scroll_top so the selected row is always visible.
        let visible_h = layout[1].height as usize;
        if self.selected < self.scroll_top {
            self.scroll_top = self.selected;
        } else if visible_h > 0 && self.selected >= self.scroll_top + visible_h {
            self.scroll_top = self.selected - visible_h + 1;
        }

        let items: Vec<ListItem> = self.filtered
            .iter()
            .enumerate()
            .skip(self.scroll_top)
            .take(visible_h)
            .map(|(pos, &idx)| {
                let cmd: &PaletteCmd = &self.commands[idx];
                let is_sel: bool = pos == self.selected;

                let label_style: Style = if is_sel {
                    Style::default().fg(Color::Rgb(228, 233, 250)).bg(Color::Rgb(53, 82, 126)).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(202, 211, 245))
                };
                let hint_style: Style = Style::default().fg(Color::Rgb(97, 107, 130));
                let cat_style: Style  = Style::default().fg(Color::Rgb(76, 96, 132));

                let prefix: &str = if is_sel { "▶ " } else { "  " };

                let line: Line<'_> = Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Rgb(122, 162, 247)).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("[{}]  ", cmd.category), cat_style),
                    Span::styled(format!("{:<33}", cmd.label),    label_style),
                    Span::styled(cmd.shortcut.to_string(),        hint_style),
                ]);

                ListItem::new(line).style(if is_sel {
                    Style::default().bg(Color::Rgb(30, 35, 47))
                } else {
                    Style::default()
                })
            })
            .collect();

        let list: List<'_> = List::new(items)
            .style(Style::default().bg(Color::Rgb(27, 31, 42)));
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