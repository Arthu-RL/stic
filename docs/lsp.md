# Adding LSP Support to STIC – Step-by-Step

Adding LSP integration is intentionally left as a second phase so the first version compiles
cleanly.

## Phase 1 – Add the `lsp` crate

```
crates/lsp/
├── Cargo.toml
└── src/
    ├── lib.rs          ← public API used by app/input
    ├── client.rs       ← JSON-RPC subprocess management
    ├── protocol.rs     ← request/response builders
    └── capabilities.rs ← what we advertise to the server
```

**Cargo.toml additions**

```toml
# workspace Cargo.toml
[workspace.dependencies]
lsp-types  = "0.95"
serde_json = "1"
tokio      = { version = "1", features = ["rt-multi-thread", "process", "io-util", "sync"] }

# crates/lsp/Cargo.toml
[dependencies]
lsp-types.workspace  = true
serde_json.workspace = true
tokio.workspace      = true
anyhow.workspace     = true
config               = { path = "../config" }
```

---

## Phase 2 – Spawn the language server process

```rust
// crates/lsp/src/client.rs
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct LspProcess {
    child:  Child,
    stdin:  tokio::process::ChildStdin,
    // receiver is polled on a background task
}

impl LspProcess {
    pub async fn spawn(cmd: &str, args: &[&str]) -> anyhow::Result<Self> {
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().unwrap();
        Ok(Self { child, stdin })
    }
}
```

---

## Phase 3 – JSON-RPC framing

LSP uses HTTP-style Content-Length headers:

```rust
// crates/lsp/src/protocol.rs
pub fn encode(msg: &serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_string(msg).unwrap();
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

pub fn decode(raw: &[u8]) -> Option<serde_json::Value> {
    let text   = std::str::from_utf8(raw).ok()?;
    let body   = text.split("\r\n\r\n").nth(1)?;
    serde_json::from_str(body).ok()
}
```

---

## Phase 4 – Initialize handshake

```rust
// After spawning, send initialize:
use lsp_types::*;
use serde_json::json;

let init = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": InitializeParams {
        process_id: Some(std::process::id()),
        root_uri: Some(Url::from_directory_path("/your/project").unwrap()),
        capabilities: ClientCapabilities::default(),
        ..Default::default()
    }
});
process.send(encode(&init)).await?;
```

Wait for `InitializeResult`, then send `initialized` notification.

---

## Phase 5 – Document sync

Every time a buffer changes, notify the server:

```rust
// textDocument/didOpen  (once, on open)
// textDocument/didChange (on every edit – send full text for simplicity;
//                         incremental diffs are an optimisation)
// textDocument/didSave   (on Ctrl+S)
// textDocument/didClose  (on close_tab)
```

---

## Phase 6 – Wire features into the App

| Feature            | LSP Method                          | App action                  |
|--------------------|-------------------------------------|-----------------------------|
| Diagnostics        | `textDocument/publishDiagnostics`   | `buf.diagnostics = …`       |
| Go to Definition   | `textDocument/definition`           | move cursor to result       |
| Find References    | `textDocument/references`           | populate diag panel list    |
| Hover Docs         | `textDocument/hover`                | show popup overlay          |
| Completions        | `textDocument/completion`           | show autocomplete dropdown  |
| Rename             | `textDocument/rename`               | apply workspace edits       |
| Code Actions       | `textDocument/codeAction`           | show action picker          |
| Symbols            | `workspace/symbol`                  | populate command palette    |

---

## Phase 7 – Recommended crate: `tower-lsp`

If you want a higher-level abstraction instead of raw JSON-RPC, add:

```toml
tower-lsp = "0.20"
```

It handles framing, threading, and the initialize handshake automatically.
Your job is just to implement the `LanguageServer` trait's callbacks.

---

## Phase 8 – Multi-server support

The `config.lsp.servers` map already handles this:

```toml
[lsp.servers.rs]
command = "rust-analyzer"

[lsp.servers.py]
command = "pylsp"

[lsp.servers.ts]
command = "typescript-language-server"
args    = ["--stdio"]
```

In `crates/lsp/src/lib.rs`, keep a `HashMap<String, LspProcess>` keyed by
extension, spawning on first use and reusing on subsequent opens.
