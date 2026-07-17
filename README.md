# Stic – Simple Terminal Integrated Code Editor

A fast, modal terminal code editor built with **Ratatui**, a **rope** buffer for large files, **syntect** syntax highlighting, and optional **LSP** support (diagnostics, go-to-definition, hover).

---

## Quick start

```bash
# Build
cargo build --release

# Open one or more files (each gets its own tab)
./target/release/stic src/main.rs
./target/release/stic a.rs b.py

# Empty buffer
./target/release/stic
```

On first run, configuration lives at **`~/.config/stic/config.toml`**.  
Fastest way to create/edit it: **`Ctrl+P`** → **Open Configuration**.

---

## How to use Stic

Stic is **modal**: the mode pill in the bottom-left status bar tells you how keys behave.

| Mode | Enter | Leave |
|------|-------|-------|
| **Normal** | `Esc` | — (default) |
| **Insert** | `i`, `I`, `o`, `O` | `Esc` |
| **Command** | `:` | `Esc` / `Enter` |
| **Search** | `Ctrl+F` | `Esc` / `Enter` |
| **Go to line** | `Ctrl+G` | `Esc` / `Enter` |
| **Command palette** | `Ctrl+P` | `Esc` / `Enter` |
| **File tree** | `Ctrl+B` | `Esc` / `q` |
| **Save As** | `Ctrl+S` on unsaved file, or palette → Save File As… | `Esc` / `Enter` |

### Saving files

| Action | How |
|--------|-----|
| Save | `Ctrl+S`, `:w`, or palette → Save File |
| Save new file (no path yet) | `Ctrl+S` opens **Save As** prompt — type path, `Enter` |
| Save As | `Ctrl+P` → **Save File As…**, or `:w path/to/file.ext` |
| Save & quit | `Ctrl+Shift+Q`, or `:wq` / `:x` |

Parent directories are created automatically when saving to a new path (e.g. `notes/todo.txt`).

### Command palette (`Ctrl+P`)

Fuzzy-search all commands by name, id, or category. Examples:

- `save` → Save File / Save File As…
- `def` → Go to Definition (LSP)
- `config` → Open Configuration

Full command list: see doc comments in `crates/command_palette/src/lib.rs`.

### Colon commands (`:` in Normal mode)

| Command | Action |
|---------|--------|
| `:w` | Save |
| `:w path/file.ext` | Save As to path |
| `:q` | Quit (blocks if unsaved) |
| `:wq` / `:x` | Save and quit |
| `:q!` | Force quit |
| `:e path` | Open file |
| `:42` | Go to line 42 |

### Essential keys (Normal mode)

| Keys | Action |
|------|--------|
| `h` `j` `k` `l` / arrows | Move cursor |
| `i` / `I` | Insert / insert at line start |
| `o` / `O` | New line below / above + Insert |
| `g` / `G` | Top / bottom of file |
| `q` / `e` | Line start / line end |
| `a` / `d` | Word forward / backward |
| `Ctrl+S` | Save |
| `Ctrl+Z` / `Ctrl+Y` | Undo / redo |
| `Ctrl+F` / `F3` | Find / find next |
| `Ctrl+B` | File tree |
| `Ctrl+T` | Show/hide terminal (shell keeps running while hidden) |
| `Ctrl+Shift+T` | Kill the terminal's shell session |
| `F12` | Go to definition (LSP) |

More detail (Insert mode, mouse, file tree): see `crates/input/src/lib.rs` module docs.

---

## Configuration

Config file: **`~/.config/stic/config.toml`**

Generate defaults: **Command Palette → Open Configuration**, or:

```bash
# From Rust (writes file if missing)
# Config::write_default() is called by the palette command
```

### Editor

```toml
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
```

### UI & syntax colors (themes)

Syntax highlighting uses **[syntect](https://github.com/trishume/syntect)** (Sublime Text grammars + color themes).  
You do **not** configure colors per language in TOML — you pick a **theme**; each language’s tokens (keywords, strings, comments) are colored by that theme.

```toml
[ui]
theme            = "base16-ocean.dark"
show_status_bar  = true
show_file_tree   = false
show_terminal    = false
```

**Popular bundled theme names** (exact spelling matters):

| Theme | Vibe |
|-------|------|
| `base16-ocean.dark` | Default; cool blue-gray |
| `Solarized (dark)` | Solarized dark |
| `Solarized (light)` | Solarized light |
| `InspiredGitHub` | GitHub-like |
| `Monokai Extended` | Classic Monokai |
| `TwoDark` | Atom Two Dark |

Change `theme`, save config, restart Stic to apply.

### Which languages get colors?

Stic maps **file extension → syntect grammar**. Examples that work **out of the box** with bundled syntaxes:

| Language | Extensions | Highlighting |
|----------|------------|--------------|
| Rust | `.rs` | Yes |
| Python | `.py` | Yes |
| C | `.c`, `.h` | Yes |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh` | Yes |
| JavaScript / TypeScript | `.js`, `.ts`, `.jsx`, `.tsx` | Yes |
| Go, Java, Ruby, … | common extensions | Usually yes (bundled set) |
| **C3** | `.c3` | **No** — not in default bundle; see [docs/TODO.md](docs/TODO.md) |

If a file has no known extension, text is shown **without** syntax colors (plain foreground).

Custom grammars (C3, niche languages) are **not implemented yet** — tracked in [docs/TODO.md](docs/TODO.md).

---

## LSP (Language Server Protocol)

LSP is **optional**. With no `[lsp.servers.*]` entries, Stic runs as a standalone editor with syntax highlighting only.

### Setup

1. Install a language server on your `PATH` (examples below).
2. Add server blocks to `~/.config/stic/config.toml` keyed by **file extension** (no dot):

```toml
[lsp.servers.rs]
command = "rust-analyzer"

[lsp.servers.py]
command = "pylsp"

[lsp.servers.cpp]
command = "clangd"

[lsp.servers.c]
command = "clangd"

[lsp.servers.ts]
command = "typescript-language-server"
args    = ["--stdio"]
```

Optional workspace root override (defaults to current directory when Stic starts):

```toml
[lsp.servers.rs]
command = "rust-analyzer"
root    = "/path/to/rust/project"
```

3. Restart Stic from the project root (or the directory you want as workspace).
4. Open a file with a matching extension — the server starts lazily on first use.

### What works today

| Feature | Trigger |
|---------|---------|
| Diagnostics (errors/warnings) | Automatic; underlined inline in the editor |
| Go to definition | `F12` or palette → Go to Definition |
| Hover docs | Palette → Show Hover Documentation |
| Document sync | On open, edit, save, close tab |

LSP lifecycle events (e.g. a server becoming ready) are logged to `/tmp/stic.log` rather than
the status bar; only errors that need your attention surface there briefly.

### What does **not** work yet

- **Autocomplete / IntelliSense popup**
- Find references, rename, code actions, symbol search

See [docs/TODO.md](docs/TODO.md) for the full list and planned work.

### Example: Rust + Python + C++

```toml
[lsp.servers.rs]
command = "rust-analyzer"

[lsp.servers.py]
command = "pylsp"

[lsp.servers.cpp]
command = "clangd"

[lsp.servers.c]
command = "clangd"
```

Install tools: `rust-analyzer`, `python-lsp-server` (or `pylsp`), `clangd`.

---

## Project layout

```text
stic/
├── src/main.rs              # Entry point, event loop
├── docs/
│   └── TODO.md              # Unimplemented features & roadmap
└── crates/
    ├── app/                 # App state, modes, LSP
    ├── buffer/              # Rope buffer, undo, diagnostics storage
    ├── command_palette/     # Ctrl+P UI + command list
    ├── config/              # TOML config load/save
    ├── editor/              # Tabs + syntect highlighter
    ├── input/               # Key/mouse → mode handlers
    ├── lsp/                 # LSP client (async-lsp-client)
    ├── terminal/            # Integrated PTY shell
    └── ui/                  # Ratatui rendering
```

---

## Dependencies (high level)

| Crate | Role |
|-------|------|
| [Ratatui](https://github.com/ratatui/ratatui) | Terminal UI |
| [ropey](https://github.com/cesarb/ropey) | Text buffer |
| [Crossterm](https://github.com/crossterm-rs/crossterm) | Input & terminal |
| [syntect](https://github.com/trishume/syntect) | Syntax highlighting & themes |
| [async-lsp-client](https://crates.io/crates/async-lsp-client) | LSP subprocess client |
| [Tokio](https://tokio.rs/) | Async runtime for LSP |
| [Serde](https://serde.rs/) + [toml](https://github.com/toml-rs/toml) | Configuration |

---

## Contributing

Pick an item from [docs/TODO.md](docs/TODO.md) or open an issue.  
User-facing keybindings and commands are documented in:

- `crates/input/src/lib.rs` — keybinding reference
- `crates/command_palette/src/lib.rs` — command palette reference

License: MIT OR Apache-2.0
