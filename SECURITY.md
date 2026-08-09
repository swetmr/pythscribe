# Security policy

## Supported versions

PythScribe is pre-1.0. Security fixes are issued against `main` only; there is no LTS branch.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Email security reports to **mrigank.swet@gmail.com** with subject line `[pythscribe security]`. Include:

- A description of the issue and its impact.
- A minimal reproduction (`.ps`/`.psc` source, command line, and observed vs. expected behavior).
- The PythScribe version (`pyths --version`) and host OS.

We aim to acknowledge reports within **3 business days** and to issue a patch or written triage decision within **14 days**. If the issue is severe and exploitable in the wild, a coordinated-disclosure window of 30–90 days from acknowledgement is the working baseline; the disclosure date is agreed in writing with the reporter.

PGP keys, bug-bounty programs, and a dedicated security mailbox are **not** offered today.

## Scope

### In scope

- **PythScribe compiler** (`crates/pyths_*`): panics, infinite loops, OOB reads, unsoundness leading to incorrect codegen, malformed source maps that crash debuggers.
- **JS/WASM codegen output**: PythScribe producing JS or WASM that is itself unsafe (XSS in DOM helpers, ReDoS in runtime regex helpers, prototype pollution, etc.).
- **Runtime helpers** (`crates/pyths_runtime/js`, `runtime/`): vulnerabilities triggered through documented APIs.
- **Vite/Next.js plugins** (`packages/vite-plugin-pyths`, `packages/next-plugin-pyths`): command-injection, path-traversal, or arbitrary file write triggered through `.ps`/`.psc` source content.
- **`pyths_expand`** (`.psc` compression layer): aliasing logic that produces code the original `.psc` author did not intend.

### Out of scope

- Vulnerabilities in compiled output JS that originate from the user's own `.ps` code (e.g., the user writes `innerHTML = user_input` — the compiler emits what the source says).
- Build-toolchain vulnerabilities in upstream dependencies (`clap`, `serde`, `wasm-encoder`, etc.) — report those to the upstream project; we'll consume the fix.
- Issues that require local-machine code execution to exploit (we already trust the developer's machine).
- Denial-of-service via giant source **files** (raw byte size): the compiler is single-pass O(n) but not hardened against a multi-GB input (it will OOM). Pathological *nesting* depth is now guarded (a clean diagnostic past ~1000 levels — no stack-overflow crash). See [`docs/security.md`](docs/security.md) §6/§8.

## Disclosure

Once a fix lands and is released, the issue is described in the release notes and in [`REVIEWERS.md`](REVIEWERS.md) under a "Security" heading. Reporters are credited by name and link unless they request anonymity.

## Threat model

A condensed threat model lives in [`docs/security.md`](docs/security.md). Highlights:

- **Trust boundary** is the developer's machine: PythScribe trusts the `.ps`/`.psc` source files it compiles. Untrusted `.ps` execution (running an LLM's output without review) is **not** a supported deployment.
- **Codegen safety**: the compiler does not generate `eval()`, dynamic `new Function(...)`, or any string-to-code conversion. All emitted JS uses static syntax.
- **DOM runtime helpers** that accept HTML strings (`set_html`, etc.) document the XSS risk in their docstrings; the runtime does no automatic sanitization.

See [`docs/security.md`](docs/security.md) for the full document.
