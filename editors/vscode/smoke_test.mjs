// Smoke test for pyths-lsp: spawn the binary, send `initialize` + a
// `textDocument/didOpen` for a buggy doc, expect `publishDiagnostics`.
//
// Run after `cargo build --release -p pyths_lsp`:
//   node editors/vscode/smoke_test.mjs

import { spawn } from "node:child_process";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const exe = process.platform === "win32" ? "pyths-lsp.exe" : "pyths-lsp";
const binPath = resolve(root, "target", "release", exe);

console.log(`[smoke] launching ${binPath}`);
const proc = spawn(binPath, [], { stdio: ["pipe", "pipe", "inherit"] });

function frame(msg) {
  const body = JSON.stringify(msg);
  return `Content-Length: ${Buffer.byteLength(body, "utf8")}\r\n\r\n${body}`;
}

let buffer = Buffer.alloc(0);
const pending = [];
proc.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd === -1) return;
    const header = buffer.slice(0, headerEnd).toString("utf8");
    const m = /Content-Length:\s*(\d+)/.exec(header);
    if (!m) return;
    const bodyLen = parseInt(m[1], 10);
    const bodyStart = headerEnd + 4;
    if (buffer.length < bodyStart + bodyLen) return;
    const body = buffer.slice(bodyStart, bodyStart + bodyLen).toString("utf8");
    buffer = buffer.slice(bodyStart + bodyLen);
    const msg = JSON.parse(body);
    pending.push(msg);
  }
});

async function recv(predicate, timeoutMs = 3000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    while (pending.length) {
      const m = pending.shift();
      if (predicate(m)) return m;
    }
    await new Promise((r) => setTimeout(r, 20));
  }
  throw new Error("recv timeout");
}

function send(msg) {
  proc.stdin.write(frame(msg));
}

(async () => {
  let pass = 0, fail = 0;
  function check(name, ok) {
    if (ok) { pass++; console.log(`  ok  - ${name}`); }
    else { fail++; console.log(`  FAIL - ${name}`); }
  }

  // initialize
  send({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} });
  const initResp = await recv((m) => m.id === 1);
  check("initialize responds", !!initResp.result);
  check("advertises hoverProvider", initResp.result.capabilities.hoverProvider === true);
  check("advertises documentSymbolProvider", initResp.result.capabilities.documentSymbolProvider === true);
  check("advertises completionProvider", !!initResp.result.capabilities.completionProvider);
  check("advertises definitionProvider", initResp.result.capabilities.definitionProvider === true);

  send({ jsonrpc: "2.0", method: "initialized", params: {} });

  // didOpen with bad source → expect publishDiagnostics with at least 1 error.
  send({
    jsonrpc: "2.0",
    method: "textDocument/didOpen",
    params: {
      textDocument: {
        uri: "file:///smoke.ps",
        languageId: "pyths",
        version: 1,
        text: "def add(a: int, b: int -> int:\n    return a + b\n",
      },
    },
  });
  const diag = await recv((m) => m.method === "textDocument/publishDiagnostics");
  check("publishDiagnostics fires on bad source", Array.isArray(diag.params.diagnostics) && diag.params.diagnostics.length >= 1);

  // documentSymbol on a clean source.
  send({
    jsonrpc: "2.0",
    method: "textDocument/didOpen",
    params: {
      textDocument: {
        uri: "file:///clean.ps",
        languageId: "pyths",
        version: 1,
        text: "def alpha(x: int) -> int:\n    return x\n\nclass Beta(Exception):\n    pass\n",
      },
    },
  });
  await recv((m) => m.method === "textDocument/publishDiagnostics" && m.params.uri === "file:///clean.ps");
  send({
    jsonrpc: "2.0", id: 2,
    method: "textDocument/documentSymbol",
    params: { textDocument: { uri: "file:///clean.ps" } },
  });
  const symResp = await recv((m) => m.id === 2);
  const syms = symResp.result || [];
  check("documentSymbol returns entries", syms.length === 2);
  check("first symbol is alpha (Function)", syms[0]?.name === "alpha" && syms[0]?.kind === 12);
  check("second symbol is Beta (Class)", syms[1]?.name === "Beta" && syms[1]?.kind === 5);

  // hover on `alpha` at line 0, col 5
  send({
    jsonrpc: "2.0", id: 3,
    method: "textDocument/hover",
    params: {
      textDocument: { uri: "file:///clean.ps" },
      position: { line: 0, character: 5 },
    },
  });
  const hover = await recv((m) => m.id === 3);
  check("hover returns markdown", typeof hover.result?.contents?.value === "string" && hover.result.contents.value.includes("alpha"));

  // shutdown / exit
  send({ jsonrpc: "2.0", id: 999, method: "shutdown", params: {} });
  await recv((m) => m.id === 999);
  send({ jsonrpc: "2.0", method: "exit", params: {} });

  console.log(`\n[smoke] ${pass} passed, ${fail} failed`);
  process.exit(fail === 0 ? 0 : 1);
})().catch((e) => {
  console.error("[smoke] error:", e);
  process.exit(1);
});
