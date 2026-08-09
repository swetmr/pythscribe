# PythScribe for Visual Studio Code

Full language support for PythScribe (`.ps`) files: syntax highlighting **plus** real LSP-driven features (diagnostics, hover, completion, document symbols, goto-definition).

## Features

### Syntax (always on)
- Syntax highlighting for all PythScribe/Python keywords
- F-string interpolation highlighting
- Decorator support (`@dataclass`, `@component`, etc.)
- React hook recognition (`use_state`, `use_effect`, etc.)
- Type annotation highlighting
- PSX (Pythonic JSX) tag highlighting
- Number literal support (hex, binary, octal, float, underscore separators)
- Comment highlighting
- Indentation-based folding

### Language server (requires `pyths-lsp` binary)
- **Diagnostics** — parse errors surfaced inline as you type
- **Hover** — function signatures, class info, built-in keyword docs
- **Completion** — Python keywords, PythScribe builtins, decorators
- **Document symbols** — outline view of top-level functions and classes (Ctrl+Shift+O)
- **Goto definition** — F12 jumps from a call site to its definition

## File-tree icons (opt-in)

The extension ships an icon theme that gives `.ps` and `.psc` files a distinctive PS-knot icon in the file tree. Off by default to avoid overriding your existing icon theme.

To enable:

1. `Ctrl+Shift+P` → `Preferences: File Icon Theme`
2. Pick **PythScribe Icons** from the list.

The theme only contributes icons for PythScribe files; every other file falls back to your previous theme's default.

## Setup

The language server is a separate Rust binary. Build it once, then load the extension.

### 1. Build the language server

```bash
cd <workspace-root>
cargo build --release -p pyths_lsp
```

This produces `target/release/pyths-lsp` (or `pyths-lsp.exe` on Windows). The extension auto-discovers it in this location, so you don't need to set anything if you build from the workspace root.

Alternative: install globally so it's on PATH:

```bash
cargo install --path crates/pyths_lsp
```

### 2. Install the npm dependencies (one-time)

```bash
cd editors/vscode
npm install
```

This pulls in `vscode-languageclient`. It's a one-time step.

### 3. Install the extension

#### Option A — Development Host (no packaging)

1. Open `editors/vscode/` in VS Code (`code editors/vscode`).
2. Press **F5**. A second VS Code window opens with the extension loaded.
3. In that window, open a `.ps` file. The status bar should briefly show *"PythScribe: language server connected"*.

#### Option B — Package and install

```bash
cd editors/vscode
npx vsce package          # produces pyths-lang-0.2.0.vsix
code --install-extension pyths-lang-0.2.0.vsix
```

The extension is now permanently installed; restart VS Code and any `.ps` file will activate it.

#### Option C — Symlink into the user-extensions folder

```bash
ln -s <abs-path>/editors/vscode ~/.vscode/extensions/pyths-lang
```

Restart VS Code.

## Verifying it's working

Open `examples/cloudflare-bench/large-samples/pythscribe/dashboard_500.ps`. You should see:

| Feature | How to check |
|---|---|
| Diagnostics | Introduce a syntax error (e.g. delete a `:`) — a red squiggle appears within ~200ms |
| Hover | Mouse over `MetricCard`, `Dashboard`, or any user-defined function name — a tooltip shows the signature |
| Document symbols | Ctrl+Shift+O lists every `@component`, function, and class with their line numbers |
| Goto definition | F12 on a call site like `MetricCard(...)` jumps to the function's `def` line |
| Completion | Type `def ` and trigger completion (Ctrl+Space) — keywords + builtins + decorators show up |

Output Channel ▸ "PythScribe LSP" shows the JSON-RPC traffic if you set `pyths.trace.server` to `messages` or `verbose` in settings.

## Configuration

| Setting | Default | Description |
|---|---|---|
| `pyths.serverPath` | `""` | Absolute path to `pyths-lsp`. If empty: looks on PATH, then `<workspace>/target/release/pyths-lsp(.exe)`, then `target/debug/`. |
| `pyths.trace.server` | `"off"` | `"messages"` or `"verbose"` to log LSP traffic to the output channel. |

## Commands

| Command | When to use |
|---|---|
| **PythScribe: Restart Language Server** | After rebuilding `pyths-lsp` (the running server still has the old binary loaded). |

## Troubleshooting

**"pyths-lsp binary not found"** — you haven't built the server yet, or the workspace root isn't where the extension is looking. Run `cargo build --release -p pyths_lsp` from the workspace root, or set `pyths.serverPath` in your settings.

**Features don't show up** — open the Output panel, switch to "PythScribe LSP" channel. If empty, the extension didn't activate (reload VS Code with the `.ps` file open). If you see *"failed to start language server"*, the binary path is wrong.

**Want the bleeding-edge build** — after `cargo build --release -p pyths_lsp`, run **PythScribe: Restart Language Server** from the command palette. The new binary is picked up without reloading VS Code.
