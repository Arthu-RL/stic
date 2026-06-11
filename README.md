# Stic – Simple Terminal Integrated Code Editor

A blazingly fast, modern, and cross-platform terminal editor built with **Ratatui** and powered by a **rope data structure** for seamless, *O(log N)* text manipulation—even on multi-megabyte files.

Whether you are making quick edits over SSH or settling in for a long coding session, Stic combines the speed of terminal applications with the modern features of heavy GUI editors.

---

## Technologies & Dependencies Included

The Stic ecosystem leverages the following open-source libraries and runtimes to deliver high-performance modal editing:

* **[Ratatui](https://github.com/ratatui/ratatui)** — Core Terminal User Interface (TUI) framing engine handling components, layouts, and display buffer cells.
* **[ropey](https://github.com/cwalton/ropey)** — Heavy-duty, immutable rope data structure managing steady *O(log N)* text mutation performance on multi-megabyte files.
* **[Crossterm](https://github.com/crossterm-rs/crossterm)** — Cross-platform terminal manipulation backend handling raw modes, terminal commands, and system key/mouse input processing.
* **[syntect](https://github.com/trishume/syntect)** — High-fidelity syntax highlighting pipeline leveraging Sublime Text `.sublime-syntax` definitions and text themes.
* **[Tokio](https://github.com/tokio-rs/tokio)** — Asynchronous engine runtime orchestrating non-blocking system tasks, file operations, and multi-threaded processing.
* **[Serde](https://github.com/serde-rs/serde)** — Generic data serialization and deserialization framework used to map settings records into memory structures.
* **[toml-rs](https://github.com/toml-rs/toml-rs)** — Zero-allocation TOML file decoding parser explicitly matching local configuration profiles.
* **[lsp-types](https://github.com/gluon-lang/lsp-types)** — Full implementation types matching the structural communication contracts outlined by the Microsoft Language Server Protocol spec.
* **[Anyhow](https://github.com/dtolnay/anyhow)** — Idiomatic, dynamic error reporting utility facilitating error propagation safely across workspace dependencies.
* **[dirs-rs](https://github.com/dirs-dev/dirs-rs)** — Platform-agnostic directory utility locating base configuration layouts across Linux, macOS, and Windows environments.
* **[unicode-width](https://github.com/unicode-rs/unicode-width)** — Explicit character metric calculator computing exact UI terminal cell grids for multi-byte or variable width Unicode representations.
---

## Why Stic?

* **Vim-Flavored, Not Vim-Restricted:** Enjoy the speed of modal editing (Normal/Insert/Command modes) with intuitive, modern global shortcuts (like `Ctrl+S` to save).
* **Fuzzy Command Palette:** Hit `Ctrl+P` to execute commands, toggle UI panels, or open configs without memorizing arbitrary keystrokes. Includes visual selection tracking markers (`▶`).
* **Multi-Line Text Selection:** Native selection anchor logic supporting drag-to-highlight text blocks with custom layered styling blocks (`SEL_BG`).
* **Full Mouse & Scroll Support:** Position your cursor with a left click, drag to select text blocks, or use the mouse inside the File Tree sidebar to select, scroll, and open files.
* **Heavy-Duty Buffer:** Backed by a rope data structure, meaning no lag when pasting huge blocks of text.
* **Polymorphic Crate Architecture:** Decoupled, single-responsibility modules powered by explicit trait-driven input state machines.

---

## Quick Start

Get Stic up and running locally in seconds:

```bash
# Clone and build the release version for maximum performance
cargo build --release

# Open a specific file
./target/release/stic src/main.rs

# Or launch the editor empty
./target/release/stic
```

---

## Keybindings & Mouse Interaction Master Sheet

Stic uses a modal editing system. **Global** keys work everywhere. **Normal** mode is for navigating and manipulating text. **Insert** mode is for typing.

### Global (Works in any mode)

| Key | Action |
| --- | --- |
| `Ctrl+P` | Open fuzzy command palette |
| `Ctrl+S` | Save current file |
| `Ctrl+Q` | Quit (warns if unsaved) |
| `Ctrl+Shift+Q` | **Safe Save & Quit** (commits current buffer text safely, then exits cleanly) |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+F` / `Ctrl+G` | Live Find / Go to line |
| `Ctrl+B` | Toggle File Tree sidebar |
| `Ctrl+T` | Toggle Terminal panel |
| `Ctrl+D` | Toggle Diagnostics panel |
| `Ctrl+N` / `Ctrl+W` | New tab / Close tab |
| `Alt+←` / `Alt+→` | Switch to previous / next tab |

### Normal Mode (Navigation & Manipulation)

*Press `Esc` from any mode to return here.*

| Key / Mouse Action | Action |
| --- | --- |
| `i` / `I` | Insert before cursor / Insert at line start |
| `a` / `A` | Insert after cursor / Insert at line end |
| `o` / `O` | Open new line below / Open new line above |
| `h` `j` `k` `l` | Move Left / Down / Up / Right (Vim style) |
| `w` / `b` | Jump forward one word / backward one word |
| `0` / `$` | Jump to start of line / end of line |
| `gg` / `G` | Jump to start of file / end of file |
| `x` | Delete character under cursor |
| `:` | Open Command Bar (e.g., `:w`, `:q`) |
| `/` | Quick search |
| `F3` | Find next match |
| **Left Click** | Clear active selection and relocate text cursor instantly |
| **Left Click + Drag** | Start selection anchor and dynamically scale multi-line selection highlights |
| **Scroll Up / Down** | Scroll text view vertical panes upwards or downwards by 3 text rows |

### Insert Mode (Typing)

| Key | Action |
| --- | --- |
| `Esc` | Return to Normal mode |
| `Tab` | Smart indent (respects your spaces/tabs config) |
| `Ctrl+W` | Delete whole word backward |
| `Arrows` | Move cursor (stays in Insert mode) |

### File Tree Panel (`Ctrl+B`)

| Key / Mouse Action | Action |
| --- | --- |
| `j` / `k` | Move selection down / up |
| **Scroll Up / Down** | Scroll sidebar entries directory listing list view up / down |
| **Left Click** | Automatically highlight clicked directory path entry row |
| `Enter` / **Left Click File** | Open selected file and return viewport focus to Normal editing mode |
| `r` | Refresh directory tree mappings |
| `Esc` | Unfocus and return to Normal mode |

---

## Configuration

Stic is highly customizable via TOML.

**Pro-tip:** You don't need to hunt for the folder! Just press `Ctrl+P`, type `Open Configuration`, and Stic will generate and open `~/.config/stic/config.toml` for you.

```toml
[editor]
tab_size = 4
use_spaces = true
line_numbers = true
scroll_off = 5
ruler_column = 80

[ui]
# Powered by syntect - swap out your vibe in seconds
theme = "base16-ocean.dark"  # Options: "Solarized (dark)", "InspiredGitHub", etc.

[lsp.servers.rs]
command = "rust-analyzer"
```

---

## Project Architecture for Developers

Want to contribute? Stic is designed to be easily readable. The workspace is split into specialized crates so you never have to guess where a feature lives:

```text
stic/
├── src/main.rs             # Application entry point
├── docs/
│   └── lsp.md              # Step-by-step LSP integration guide (Coming Soon)
└── crates/
    ├── app/                # Application state, mode machine, and command dispatch
    ├── buffer/             # Rope-backed text buffer with undo/redo stacks, fuzzy search, diagnostic logs, and selection anchor boundaries tracking
    ├── command_palette/    # Fuzzy-filtered command UI overlay utilizing index-matching tracks and active row markers (`▶`)
    ├── config/             # TOML parsing with sane user defaults
    ├── editor/             # Buffer manager + syntect syntax highlighter
    ├── input/              # Decoupled polymorphic `InputHandler` trait system mapping hardware terminal key/mouse events onto distinct mode structs (Normal, Insert, etc.)
    └── ui/                 # The complete Ratatui rendering pipeline managing vertical split layouts, gutter relative numbering, and multi-line selection overlay highlights (`SEL_BG`)
```