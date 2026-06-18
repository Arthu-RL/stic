# Stic – Planned Features & Known Gaps

This file tracks features that are **not yet implemented** or only partially wired.
See the [README](../README.md) for what works today.

---

## LSP

| Feature | Status | Notes |
|---------|--------|-------|
| Diagnostics (`publishDiagnostics`) | **Done** | Shown in diagnostics panel (`Ctrl+D`) |
| Go to Definition | **Done** | `F12` or Command Palette |
| Hover documentation | **Done** | Command Palette → Show Hover Documentation |
| **Autocomplete / completions** | **Not implemented** | Palette command exists; shows “not yet implemented” |
| Find References | **Not implemented** | `Shift+F12` / palette stub |
| Rename Symbol | **Not implemented** | `F2` / palette stub |
| Code Actions | **Not implemented** | `Ctrl+.` / palette stub |
| Workspace Symbol Search | **Not implemented** | `Ctrl+Shift+O` / palette stub |
| Completion popup UI | **Not implemented** | Needs dropdown overlay + `textDocument/completion` |
| Incremental document sync | **Not implemented** | Full-buffer sync on every edit today |

---

## Syntax highlighting

| Feature | Status | Notes |
|---------|--------|-------|
| Bundled languages (Rust, C, C++, Python, …) | **Done** | Via syntect defaults + file extension |
| Theme selection (`ui.theme`) | **Done** | Sublime Text theme name in config |
| **Custom language grammars** (e.g. C3 / `.c3`) | **Not implemented** | No config path to load extra `.sublime-syntax` files yet |
| **Per-language theme overrides** | **Not implemented** | Single global theme only |
| Live theme switch without restart | **Not implemented** | Change config and reopen editor |

### Adding other languages (future)

Today Stic only uses syntect’s **bundled** syntax set. C3 (`.c3`) is not included.
Planned approach:

1. Ship or download a `.sublime-syntax` for C3 (or any language).
2. Add config, e.g. `syntax.extra_paths = ["~/.config/stic/syntaxes"]`.
3. Load extra syntaxes at startup alongside `SyntaxSet::load_defaults_newlines()`.

---

## Editor & UI

| Feature | Status | Notes |
|---------|--------|-------|
| File tree: collapsed by default | **Done** | Directories start collapsed; `Space`/`l`/`→` to expand |
| File tree: lazy async loading | **Done** | `tokio::fs` scan per directory, depth ≤ 3, non-blocking |
| File tree: collapse / parent jump | **Done** | `h`/`←` collapses or jumps to parent |
| File tree: loading indicator `⊙` | **Done** | Shown while scan is in-flight |
| Integrated terminal | **Stub** | Panel toggles; no PTY/shell yet |
| Open File dialog | **Not implemented** | Use `:e path` or CLI `stic file.txt` |
| Configurable keybindings | **Not wired** | `[keybindings]` in TOML is parsed but input uses hardcoded keys |
| Save As path completion | **Not implemented** | Manual path entry only |
| Multiple cursors | **Not planned yet** | — |
| Split panes | **Not planned yet** | — |

---

## Documentation

| Item | Status |
|------|--------|
| User guide in README | **Done** |
| Keybinding reference in `crates/input` docs | **Done** |
| Command reference in `crates/command_palette` docs | **Done** |
| Developer LSP integration guide (`docs/lsp.md`) | **Removed** — LSP client crate is implemented |
