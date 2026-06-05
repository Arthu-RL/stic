//! Configuration crate.
//!
//! Reads `~/.config/stic/config.toml`, falling back to compiled defaults.
//! All fields are individually optional in the file so users only override
//! what they care about.


use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;


/// The root structural mapping layer holding distinct configurable subgroups of the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor:      EditorConfig,
    pub ui:          UiConfig,
    pub keybindings: KeybindingsConfig,
    pub lsp:         LspConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// Spaces per tab stop.
    pub tab_size:        usize,
    /// Expand tabs to spaces on insert.
    pub use_spaces:      bool,
    /// Show absolute line numbers.
    pub line_numbers:    bool,
    /// Show numbers relative to cursor line (like Vim's relativenumber).
    pub relative_numbers: bool,
    /// Soft-wrap lines at viewport width.
    pub word_wrap:       bool,
    /// Copy indentation from the previous line on Enter.
    pub auto_indent:     bool,
    /// Minimum lines of context kept above/below cursor when scrolling.
    pub scroll_off:      usize,
    /// Highlight the line the cursor is on.
    pub highlight_line:  bool,
    /// Show a vertical guide at this column (0 = disabled).
    pub ruler_column:    usize,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// syntect theme name (e.g. "base16-ocean.dark", "Solarized (dark)").
    pub theme:            String,
    pub show_status_bar:  bool,
    /// File-tree panel visible by default.
    pub show_file_tree:   bool,
    /// Integrated terminal panel visible by default.
    pub show_terminal:    bool,
    /// Diagnostic (errors/warnings) panel visible by default.
    pub show_diagnostics: bool,
}


/// Global tracking collection mapping specific hardware shortcut keys onto internal operational action handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub save:             String,
    pub quit:             String,
    pub force_quit:       String,
    pub command_palette:  String,
    pub toggle_file_tree: String,
    pub toggle_terminal:  String,
    pub toggle_diag:      String,
    pub find:             String,
    pub find_next:        String,
    pub find_prev:        String,
    pub go_to_line:       String,
    pub undo:             String,
    pub redo:             String,
    pub next_tab:         String,
    pub prev_tab:         String,
    pub close_tab:        String,
    pub new_tab:          String,
}


/// Registry index structure sorting background Language Server Protocol connections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LspConfig {
    /// Per-language-extension server definitions.
    pub servers: std::collections::HashMap<String, LspServer>,
}


/// Execution parameters managing subsystem command hooks targeting specified compiler analyses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServer {
    /// Executable name or absolute path.
    pub command: String,
    /// Extra CLI arguments.
    #[serde(default)]
    pub args: Vec<String>,
}


impl Default for Config {
    /// Generates a standardized fallback parameters root node structure.
    ///
    /// # Returns
    ///
    /// A compiled default configuration instance.
    fn default() -> Self {
        Self {
            editor:      EditorConfig::default(),
            ui:          UiConfig::default(),
            keybindings: KeybindingsConfig::default(),
            lsp:         LspConfig::default(),
        }
    }
}


impl Default for EditorConfig {
    /// Assigns stock editor formatting spacing and safety context boundaries constraints.
    ///
    /// # Returns
    ///
    /// A populated default `EditorConfig` layout collection.
    fn default() -> Self {
        Self {
            tab_size:         4,
            use_spaces:       true,
            line_numbers:     true,
            relative_numbers: false,
            word_wrap:        false,
            auto_indent:      true,
            scroll_off:       5,
            highlight_line:   true,
            ruler_column:     80,
        }
    }
}


impl Default for UiConfig {
    /// Sets initial visual panel tracking flags alongside primary styling formats.
    ///
    /// # Returns
    ///
    /// Standard stock default configuration rules for interface environments.
    fn default() -> Self {
        Self {
            theme:            "base16-ocean.dark".into(),
            show_status_bar: true,
            show_file_tree: false,
            show_terminal: false,
            show_diagnostics: false,
        }
    }
}


impl Default for KeybindingsConfig {
    /// Maps stock layout mappings binding control values onto peripheral interface keys.
    ///
    /// # Returns
    ///
    /// A core populated default `KeybindingsConfig` keyboard key configuration layout wrapper.
    fn default() -> Self {
        Self {
            save:             "ctrl+s".into(),
            quit:             "ctrl+q".into(),
            force_quit:       "ctrl+shift+q".into(),
            command_palette:  "ctrl+p".into(),
            toggle_file_tree: "ctrl+b".into(),
            toggle_terminal:  "ctrl+t".into(),
            toggle_diag:      "ctrl+d".into(),
            find:             "ctrl+f".into(),
            find_next:        "F3".into(),
            find_prev:        "shift+F3".into(),
            go_to_line:       "ctrl+g".into(),
            undo:             "ctrl+z".into(),
            redo:             "ctrl+y".into(),
            next_tab:         "alt+right".into(),
            prev_tab:         "alt+left".into(),
            close_tab:        "ctrl+w".into(),
            new_tab:          "ctrl+n".into(),
        }
    }
}


impl Config {
    /// Evaluates parameters loaded from disk or safely steps back to built-in presets upon error.
    ///
    /// # Returns
    ///
    /// A fully hydrated executable configuration structure.
    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    /// Accesses designated platform configuration pathways parsing content streams.
    ///
    /// # Returns
    ///
    /// A descriptive `Result` containing the decoded config framework properties or parsing errors.
    fn try_load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        let cfg: Config = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Computes explicit system file locations targeting individual execution preferences on disk.
    ///
    /// # Returns
    ///
    /// A descriptive `Result` wrapping operating system directory paths.
    pub fn config_path() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("cannot find config dir"))?;
        Ok(base.join("stic").join("config.toml"))
    }

    /// Writes out initial default format setups to storage locations to aid option identification.
    ///
    /// # Returns
    ///
    /// A descriptive `Result` pointing onto calculated physical location destination targets.
    pub fn write_default() -> Result<PathBuf> {
        let path = Self::config_path()?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let text = DEFAULT_CONFIG_TOML;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}


const DEFAULT_CONFIG_TOML: &str = r#"# Stic Editor – Configuration
# Located at ~/.config/Stic/config.toml
# All values below are the defaults; uncomment and edit to override.

[editor]
tab_size         = 4
use_spaces       = true
line_numbers     = true
relative_numbers = false
word_wrap        = false
auto_indent      = true
scroll_off       = 5
highlight_line   = true
ruler_column     = 80

[ui]
theme            = "base16-ocean.dark"
show_status_bar  = true
show_file_tree   = false
show_terminal    = false
show_diagnostics = false

[keybindings]
save             = "ctrl+s"
quit             = "ctrl+q"
command_palette  = "ctrl+p"
toggle_file_tree = "ctrl+b"
toggle_terminal  = "ctrl+t"
find             = "ctrl+f"
go_to_line       = "ctrl+g"
undo             = "ctrl+z"
redo             = "ctrl+y"
next_tab         = "alt+right"
prev_tab         = "alt+left"
close_tab        = "ctrl+w"
"#;