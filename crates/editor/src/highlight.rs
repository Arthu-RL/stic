//! Syntax highlighting via syntect.
//!
//! We keep a [`HighlightLines`] iterator alive per-buffer-render so that
//! incremental state is handled correctly by syntect.  The visible window is
//! fed in as a slice of line strings.


use ratatui::style::{Color, Modifier, Style};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Theme, ThemeSet},
    parsing::SyntaxSet,
};


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
    /// # Arguments
    ///
    /// * `theme_name` - The targeted theme identification string key.
    ///
    /// # Returns
    ///
    /// An initialized and prepared `Highlighter` instance.
    pub fn new(theme_name: &str) -> Self {
        Self {
            ss: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
            theme_name: theme_name.to_string(),
        }
    }

    /// Evaluates structural setups to reference active theme configurations.
    ///
    /// # Returns
    ///
    /// A core reference token pattern pointer matching the active `Theme`.
    fn theme(&self) -> &Theme {
        self.ts.themes.get(&self.theme_name)
            .or_else(|| self.ts.themes.get("base16-ocean.dark"))
            .expect("base16-ocean.dark always bundled")
    }

    /// Transforms an ordered block array layout of strings into styled visual span groups.
    ///
    /// # Arguments
    ///
    /// * `lines` - A sequence containing text rows to evaluate.
    /// * `extension` - The type extension identifier matching the file track format.
    ///
    /// # Returns
    ///
    /// A collection mapping structured arrays of `HighlightedSpan` rows matching source strings.
    ///
    /// # Notes
    ///
    /// Parser engines depend heavily on historical transitions; string structures must pass
    /// in consecutive order to map state patterns accurately.
    pub fn highlight(&self, lines: &[String], extension: &str) -> Vec<Vec<HighlightedSpan>> {
        let syntax: &syntect::parsing::SyntaxReference = self.ss.find_syntax_by_extension(extension)
            .unwrap_or_else(|| self.ss.find_syntax_plain_text());
        let mut h: HighlightLines<'_> = HighlightLines::new(syntax, self.theme());
        let mut out: Vec<Vec<HighlightedSpan>> = Vec::with_capacity(lines.len());

        for line in lines {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = h.highlight_line(line, &self.ss).unwrap_or_default();
            let spans: Vec<HighlightedSpan>  = ranges.iter()
                .map(|(sty, txt)| HighlightedSpan {
                    text:  txt.to_string(),
                    style: convert_style(sty),
                })
                .collect();
            out.push(spans);
        }
        out
    }

    /// Gathers all bundled color style design variants kept inside memory setups.
    ///
    /// # Returns
    ///
    /// A vector listing all valid and text-loadable identifier names.
    pub fn available_themes(&self) -> Vec<&str> {
        self.ts.themes.keys().map(|s: &String| s.as_str()).collect()
    }
}

/// Normalizes syntax structural states into compatible standard interface traits.
///
/// # Arguments
///
/// * `s` - Source style layout parameters tracking text properties.
///
/// # Returns
///
/// A stylized output layout structure containing mapping updates.
fn convert_style(s: &syntect::highlighting::Style) -> Style {
    let mut r: Style = Style::default().fg(sc_to_ratatui(s.foreground));
    if s.font_style.contains(FontStyle::BOLD)      { r = r.add_modifier(Modifier::BOLD);      }
    if s.font_style.contains(FontStyle::ITALIC)    { r = r.add_modifier(Modifier::ITALIC);    }
    if s.font_style.contains(FontStyle::UNDERLINE) { r = r.add_modifier(Modifier::UNDERLINED);}
    r
}


/// Projects raw system values straight down into color structures.
///
/// # Arguments
///
/// * `c` - The initial source color properties structure wrapper to translate.
///
/// # Returns
///
/// A corresponding output structure defining exact RGB values.
fn sc_to_ratatui(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}