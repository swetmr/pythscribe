// PythScribe VS Code language-client.
//
// Spawns the `pyths-lsp` binary and connects it via the LSP protocol over
// stdio. Resolves the binary path in this order:
//   1. The `pyths.serverPath` setting (if set)
//   2. `pyths-lsp` on PATH
//   3. `<workspace>/target/release/pyths-lsp` (or `.exe` on Windows)
//   4. `<workspace>/target/debug/pyths-lsp` (or `.exe`)
//
// If none of those resolves to a real file, surfaces an actionable error
// with the exact `cargo build` command the user needs to run.

const vscode = require("vscode");
const path = require("node:path");
const fs = require("node:fs");
const cp = require("node:child_process");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

/** @type {LanguageClient | undefined} */
let client;

function which(cmd) {
  // Cross-platform `which` — returns absolute path of `cmd` on PATH, or null.
  const isWin = process.platform === "win32";
  const exts = isWin ? (process.env.PATHEXT || ".EXE;.CMD;.BAT").split(";") : [""];
  const dirs = (process.env.PATH || "").split(path.delimiter);
  for (const d of dirs) {
    for (const ext of exts) {
      const candidate = path.join(d, cmd + ext);
      try {
        if (fs.statSync(candidate).isFile()) {
          return candidate;
        }
      } catch (_) {}
    }
  }
  return null;
}

function resolveServerPath() {
  const config = vscode.workspace.getConfiguration("pyths");
  const configured = config.get("serverPath", "").trim();
  if (configured) {
    if (fs.existsSync(configured)) return configured;
    throw new Error(
      `pyths.serverPath was set to "${configured}" but no file exists there.`,
    );
  }

  // Try PATH (covers `cargo install --path crates/pyths_lsp` users).
  const onPath = which("pyths-lsp");
  if (onPath) return onPath;

  // Fall back to workspace-local `target/release` then `target/debug`.
  const folders = vscode.workspace.workspaceFolders;
  if (folders && folders.length) {
    const root = folders[0].uri.fsPath;
    const exe = process.platform === "win32" ? ".exe" : "";
    const candidates = [
      path.join(root, "target", "release", "pyths-lsp" + exe),
      path.join(root, "target", "debug", "pyths-lsp" + exe),
    ];
    for (const c of candidates) {
      if (fs.existsSync(c)) return c;
    }
  }

  throw new Error(
    "pyths-lsp binary not found. Build it with `cargo build --release -p pyths_lsp` " +
      "(from the workspace root), or set `pyths.serverPath` to its absolute path.",
  );
}

async function startClient() {
  let serverPath;
  try {
    serverPath = resolveServerPath();
  } catch (e) {
    vscode.window.showErrorMessage(`PythScribe: ${e.message}`);
    return;
  }

  /** @type {import("vscode-languageclient/node").ServerOptions} */
  const serverOptions = {
    command: serverPath,
    args: [],
    transport: TransportKind.stdio,
    options: {
      env: { ...process.env, RUST_BACKTRACE: "1" },
    },
  };

  /** @type {import("vscode-languageclient/node").LanguageClientOptions} */
  const clientOptions = {
    documentSelector: [
      { scheme: "file", language: "pyths" },
      { scheme: "untitled", language: "pyths" },
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.ps"),
    },
    outputChannelName: "PythScribe LSP",
  };

  client = new LanguageClient(
    "pythsLsp",
    "PythScribe Language Server",
    serverOptions,
    clientOptions,
  );

  try {
    await client.start();
    vscode.window.setStatusBarMessage("PythScribe: language server connected", 4000);
  } catch (e) {
    vscode.window.showErrorMessage(
      `PythScribe: failed to start language server: ${e && e.message ? e.message : e}`,
    );
  }
}

function activate(context) {
  startClient();

  // Restart command (handy after rebuilding the binary).
  context.subscriptions.push(
    vscode.commands.registerCommand("pyths.restartServer", async () => {
      if (client) {
        await client.stop();
        client = undefined;
      }
      await startClient();
    }),
  );
}

async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
