//! Syntax highlighting via syntect.
//!
//! Built-in syntect themes are augmented at startup with two embedded custom
//! themes compiled directly into the binary:
//!
//! | Config name          | Description                                  |
//! |----------------------|----------------------------------------------|
//! | `Monokai`            | Classic Monokai colour scheme                |
//! | `High Contrast Dark` | High-contrast dark theme (Qt Creator style)  |
//!
//! Any theme name that is not found falls back to `base16-ocean.dark`.


use std::io::Cursor;

use ratatui::style::{Color, Modifier, Style};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
};


// Embedded custom theme files (compiled into the binary at build time).
const MONOKAI_THEME_XML:       &[u8] = include_bytes!("themes/monokai.tmTheme");
const HIGH_CONTRAST_DARK_XML:  &[u8] = include_bytes!("themes/high-contrast-dark.tmTheme");


/// Represents a text segment paired with its resolved UI styling attributes.
#[derive(Debug, Clone)]
pub struct HighlightedSpan {
    pub text:  String,
    pub style: Style,
}


/// Manages syntax token parsing sets and active theme configurations.
pub struct Highlighter {
    pub ss:         SyntaxSet,
    pub ts:         ThemeSet,
    pub theme_name: String,
}


impl Highlighter {
    /// Instantiates a syntax highlighting engine matching a preferred theme specification.
    ///
    /// Custom embedded themes (`Monokai`, `High Contrast Dark`) are registered
    /// into the `ThemeSet` alongside syntect's bundled defaults before the
    /// active theme is resolved.
    ///
    /// # Arguments
    ///
    /// * `theme_name` - The targeted theme identification string key.
    ///
    /// # Returns
    ///
    /// An initialized and prepared `Highlighter` instance.
    pub fn new(theme_name: &str) -> Self {
        let ss: SyntaxSet = SyntaxSet::load_defaults_newlines();
        let mut ts: ThemeSet = ThemeSet::load_defaults();

        Self::register_embedded_theme(&mut ts, "Monokai", MONOKAI_THEME_XML);
        Self::register_embedded_theme(&mut ts, "High Contrast Dark", HIGH_CONTRAST_DARK_XML);

        Self { ss, ts, theme_name: theme_name.to_string() }
    }

    /// Returns a list of all available theme names (built-in + custom).
    pub fn available_themes(&self) -> Vec<&str> {
        self.ts.themes.keys().map(|s: &String| s.as_str()).collect()
    }

    /// Transforms an ordered block of line strings into styled visual span groups.
    ///
    /// Lines must be supplied in consecutive order so that syntect's incremental
    /// state machine tracks multi-line tokens (e.g. block comments) correctly.
    ///
    /// # Arguments
    ///
    /// * `lines`     - A slice of text rows to highlight, starting at `scroll_top`.
    /// * `extension` - File extension used to select the syntax grammar.
    ///
    /// # Returns
    ///
    /// One `Vec<HighlightedSpan>` per input line.
    pub fn highlight(&self, lines: &[String], extension: &str) -> Vec<Vec<HighlightedSpan>> {
        let syntax: &syntect::parsing::SyntaxReference = self.ss.find_syntax_by_extension(extension)
            .unwrap_or_else(|| self.ss.find_syntax_plain_text());
        let mut h: HighlightLines<'_> = HighlightLines::new(syntax, self.active_theme());
        let mut out: Vec<Vec<HighlightedSpan>> = Vec::with_capacity(lines.len());

        for line in lines {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = h.highlight_line(line, &self.ss).unwrap_or_default();
            let spans: Vec<HighlightedSpan> = ranges.iter()
                .map(|(sty, txt)| HighlightedSpan {
                    text:  txt.to_string(),
                    style: convert_style(sty),
                })
                .collect();
            out.push(spans);
        }
        out
    }

    /// Returns the active [`Theme`], falling back to `base16-ocean.dark` if the
    /// configured name is not found.
    fn active_theme(&self) -> &Theme {
        self.ts.themes.get(&self.theme_name)
            .or_else(|| self.ts.themes.get("base16-ocean.dark"))
            .expect("base16-ocean.dark always bundled with syntect")
    }

    /// Parses `xml_bytes` as a `.tmTheme` plist and inserts the resulting
    /// [`Theme`] into `ts` under `name`.  Parse errors are logged and ignored
    /// so a malformed embedded asset never crashes the editor at startup.
    fn register_embedded_theme(ts: &mut ThemeSet, name: &str, xml_bytes: &[u8]) {
        let mut cursor: Cursor<&[u8]> = Cursor::new(xml_bytes);
        match ThemeSet::load_from_reader(&mut cursor) {
            Ok(theme) => { ts.themes.insert(name.to_string(), theme); }
            Err(e)    => { eprintln!("failed to load embedded theme '{name}': {e}"); }
        }
    }
}


/// Converts a syntect `Style` into a ratatui `Style`.
fn convert_style(s: &syntect::highlighting::Style) -> Style {
    let mut r: Style = Style::default().fg(sc_to_ratatui(s.foreground));
    if s.font_style.contains(FontStyle::BOLD)      { r = r.add_modifier(Modifier::BOLD);      }
    if s.font_style.contains(FontStyle::ITALIC)    { r = r.add_modifier(Modifier::ITALIC);    }
    if s.font_style.contains(FontStyle::UNDERLINE) { r = r.add_modifier(Modifier::UNDERLINED);}
    r
}

/// Maps a syntect RGBA `Color` to ratatui's `Color::Rgb`.
fn sc_to_ratatui(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}
