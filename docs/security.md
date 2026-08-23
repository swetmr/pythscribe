# PythScribe security model

This document is the threat model and security-review packet for the PythScribe compiler, its runtime, and the Vite/Next.js build plugins. It is the working artifact behind any future external security audit.

For coordinated-disclosure procedure, see [`SECURITY.md`](../SECURITY.md) at the repo root.

---

## 1. Trust boundaries

PythScribe has three trust boundaries:

| Boundary | What is trusted | What is not |
|---|---|---|
| **Developer machine** | `.ps`/`.psc` source, `pyths.toml`, project-local `.pyi` stubs, the developer's own JS/TS toolchain | Files written by other users or by an unprivileged process |
| **CI / build server** | Whatever the developer committed to the repo | Pull-request source from external contributors *before* manual review |
| **Browser / Node runtime** | The compiled JS + WASM output | User input that flows into runtime helpers (DOM, fetch, storage) |

**Compiling an untrusted `.ps` file is NOT a supported deployment.** If a downstream system feeds attacker-controlled source into `pyths compile`, it inherits whatever surface area the compiler exposes (parser panics, codegen output of pathological size, etc.). See §3 for the panic-resistance guarantees and §6 for known DoS limits.

---

## 2. Asset inventory

Things a successful attack could target:

1. **Developer machine** — exfiltration via the build process (e.g., a malicious dependency).
2. **Built artifacts** — JS or WASM that ships to users containing injected behavior the developer didn't write.
3. **Browser users** — XSS, prototype pollution, or other web-layer attacks via runtime helpers.
4. **Server users** — injection in compiled handler code (when PythScribe is used for `pyths.web` server handlers or Next.js API routes).

---

## 3. Threat catalog

### T1 — Compiler panic on malformed source

**Vector**: a `.ps` or `.psc` source crafted to trigger an unwrap/index-out-of-bounds in lexer, parser, expander, type checker, or codegen.

**Impact**: the `pyths` process aborts. On a single-developer machine, mildly annoying. On a build server that auto-compiles untrusted contributions (e.g., a SaaS playground), DoS.

**Mitigations today**:
- The parser uses recursive-descent with explicit error returns; no `panic!` calls in the happy path.
- The `pyths_expand` crate is byte-aware UTF-8-safe; the multibyte regression suite asserts panics never fire on emoji, RTL marks, etc.
- A fuzz harness (`crates/pyths_cli/tests/fuzz_inputs.rs` — see §5 of this document) compiles random ASCII and UTF-8 byte sequences; the test fails if any input panics.

**Open**: not yet run under [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) with coverage-guided mutation. Listed in §6 (Known limits).

### T2 — Unsafe codegen output (XSS, prototype pollution)

**Vector**: PythScribe emits JS that itself contains a vulnerability — e.g., uses `innerHTML` on user input without escaping, exposes a prototype-pollution surface, or generates a `Function(...)` constructor.

**Impact**: every site built with PythScribe inherits the flaw.

**Mitigations today**:
- **No string-to-code conversion**. The codegen emits literal JS expressions and statements. There is no `eval`, no `new Function`, no `setTimeout(string, ...)` form. Verified by `grep -rn 'eval\|new Function' crates/pyths_codegen_js/src/` (zero hits).
- **DOM helpers that accept HTML** (`set_html` and its explicitly-named alias `dangerously_set_html`, `query` selectors that build markup) document the XSS risk in their docstrings. The runtime does no implicit sanitization — the user is expected to pass safe HTML or use safe helpers like `set_text`. The `dangerously_set_html` alias (A18) exists to make the sink unmissable at the call site; both map to the same `innerHTML` assignment. See `docs/known-limitations.md` → "HTML sinks".
- **JSX-style codegen** uses `createElement(tag, props, ...children)` — props are object literals, children pass through React's normal escape semantics.
- **Source-map JSON** is generated via deterministic string concat with VLQ-encoded integers; no user-controlled strings are interpolated.

**Recommendations to user code**:
- Prefer `set_text` over `set_html` for any string that originates from user input.
- When using `set_html` deliberately, run input through DOMPurify or an equivalent sanitizer before assignment.

### T3 — Malicious `pyths.toml`

**Vector**: a project's `pyths.toml` points `[stubs.paths]` or `[expand.dictionary]` at attacker-controlled content; the developer runs `pyths compile` and gets unexpected behavior.

**Impact**: low — `pyths.toml` is in the developer's own checkout. If the developer trusts the repo, they trust `pyths.toml`. The only realistic vector is a compromised pull request that modifies `pyths.toml` to change emit paths.

**Mitigations today**:
- `pyths.toml` discovery is Cargo-style walk-up from CWD — only files inside the project tree are honored.
- Malformed `pyths.toml` falls back to defaults (the loader never errors).
- `[stubs.paths]` are resolved relative to the `pyths.toml` directory; absolute paths are allowed but obvious in diff review.
- `[npm.imports]` values are emitted verbatim as JS module specifiers — they can re-route imports, but to do harm they'd need to point at a malicious package the dev installed locally.

**Recommendation**: treat `pyths.toml` changes the same as `package.json` or `Cargo.toml` changes in code review.

### T4 — Project-local `.pyi` stub injection

**Vector**: an attacker-controlled stub redefines `react.use_state` so the type checker accepts a malformed argument that later crashes at runtime.

**Impact**: low — the type checker is advisory, not a runtime gate. The compiled JS is the same regardless of stub content. A bad stub causes incorrect type errors (false positives or false negatives), not insecure output.

**Mitigations today**: stubs are parsed with the same recursive-descent parser; a malformed stub falls back to `Any` for unresolved names — never to executable code.

### T5 — Supply-chain compromise of build dependencies

**Vector**: a `cargo` or `npm` dependency is compromised; PythScribe's binaries or runtime pull in malicious code.

**Impact**: critical (full compromise of any user's build chain).

**Mitigations today**:
- **Rust deps** (`Cargo.toml`): pinned via `Cargo.lock`. Top-level deps are vetted (`clap`, `serde`, `toml`, `wasm-encoder`, `logos`, `ariadne`, `owo-colors`, `insta`, `criterion`).
- **npm runtime deps** (`runtime/package.json`): we ship zero runtime npm deps in `pyths-runtime`. The Vite/Next plugins depend on Node built-ins only; the published packages have a trivial dep graph.
- **No build-script execution**: the workspace does not use `build.rs` for any first-party crate.
- Regular `cargo audit` / `npm audit` runs in CI (TODO: not yet wired — see §6).

**Open**: `cargo audit` and `npm audit` not yet wired into CI. Tracked in §6.

### T6 — `.psc` expander semantic drift

**Vector**: an expander tier introduces a substitution that changes program meaning silently — e.g., kwarg alias collides with a user identifier, or a dictionary alias shadows a string the user wrote literally.

**Impact**: the user's program does something different from what the source reads. This is more a correctness threat than security, but listed here because it's worth tracking under "intentional code obfuscation by hostile code-emitter".

**Mitigations today**:
- The `$` sentinel for dictionary aliases is **not a valid Python token character**; collisions with user identifiers are structurally impossible.
- Kwarg aliases substitute only in function-call argument position via a state machine that excludes string literals, comments, and f-string interpolations (see `crates/pyths_expand/src/kwargs.rs`).
- Hook-call shorthand requires a following `(`; bare-identifier uses like `us` (as a variable) pass through (see `crates/pyths_expand/src/hooks.rs`).
- 136 unit tests + 24 integration tests in `pyths_expand` cover these edge cases.
- `pyths expand foo.psc -o foo.ps` is the recommended workflow — humans review the canonical form before trusting AI-emitted `.psc`.

### T7 — Source-map information disclosure

**Vector**: emitted `.js.map` files contain original source. If deployed to production unintentionally, attacker can recover the `.ps` source.

**Impact**: information disclosure (no execution, no code injection). Standard for any modern web toolchain.

**Mitigations today**:
- Source maps are emitted only when `--sourcemap` is passed. Default is no source map.
- **`--no-sources-content` (A17)** omits the inlined `sourcesContent` from the emitted `.js.map`. Positions still resolve to `.ps` line:col, but the original source text (comments, server-side logic, secrets) is **not** shipped. Recommended for production/edge builds where a `.map` may be served. The default still inlines `sourcesContent` (unchanged — best DX for dev).
- The Vite/Next plugins emit source maps for dev mode; production builds in Vite/Next have their own source-map policy. (Follow-up: the plugins pass `--sourcemap` unconditionally including in build mode; wiring `--no-sources-content` into their production path is a tracked enhancement.)

**Recommendation**: standard web-deploy hygiene — exclude `.map` files from production, or build production maps with `--no-sources-content`.

### T8 — Path traversal in CLI input

**Vector**: `pyths compile ../../etc/passwd` — does the CLI read arbitrary files?

**Impact**: low — the CLI is a local tool. Reading files the user has access to is intentional.

**Mitigations**: not applicable; this is a feature, not a bug. CLI inputs are passed to `std::fs::read_to_string` directly with no escaping. Writing files via `-o` similarly trusts the user's path argument.

**Recommendation if used in a sandboxed context** (e.g., a SaaS playground compiling user submissions): wrap the CLI in a process sandbox with restricted filesystem and CPU/memory limits.

---

## 4. Runtime-helper review checklist

For each new runtime helper added to `crates/pyths_runtime/js/` or `runtime/`:

- [ ] Does it accept any string that may originate from user input?
  - If yes: does it interpolate into HTML, JS, or SQL? If yes: document the escaping responsibility.
- [ ] Does it accept a regex pattern?
  - If yes: is the pattern user-controlled? ReDoS check.
- [ ] Does it accept a URL?
  - If yes: validate scheme (`http`/`https`) before fetch; reject `javascript:`, `data:` unless explicitly opted in.
- [ ] Does it write to `window`, `document`, or any global?
  - If yes: namespace under `pyths_*` to avoid collisions; document.
- [ ] Does it use `eval`, `new Function`, `Function`, or any string-to-code path?
  - **MUST NOT** unless reviewed and justified in the docstring.
- [ ] Is there a unit test that covers a malicious-input case?
  - If applicable, yes.

Existing helpers in `runtime/dom.js` have been reviewed against this checklist (see commit history; documented inline).

---

## 5. Fuzz harness

A simple panic-resistance fuzz suite lives in `crates/pyths_cli/tests/fuzz_inputs.rs`. It exercises the four components most likely to panic on adversarial input:

- `pyths_lexer::lex_recovering`
- `pyths_expand::expand`
- `pyths_parser::parse`
- `pyths_types::check`

Input corpus:

1. **Seeded random ASCII**: 200 inputs from a deterministic RNG, lengths 0–4096. Confirms the pipeline never panics on garbage ASCII.
2. **Seeded random UTF-8**: 200 inputs including multi-byte sequences, emoji, RTL marks, zero-width joiners. Confirms UTF-8 handling never panics.
3. **Mutated valid sources**: starting from fixtures in `tests/fixtures/`, apply byte-level mutations (flip, insert, delete) and re-feed through the pipeline.

The harness is a regression test, not a coverage-guided fuzzer. For production-grade fuzzing, a `cargo-fuzz` setup ships in [`fuzz/`](../fuzz/README.md) at the repo root with four targets (lexer, expander, parser, checker). It requires nightly + `cargo install cargo-fuzz` to run — see the fuzz README for invocation.

Run locally with:

```bash
cargo test -p pyths_cli --test fuzz_inputs --release
```

The `--release` flag matters: debug builds are ~10× slower and the harness generates a few thousand inputs.

---

## 6. Known limits / open items

These are areas where the current security posture is **acknowledged** rather than mitigated. Closing them is the path from "audit-ready" to "audited".

| Item | Status | Risk |
|---|---|---|
| `cargo-fuzz` with coverage-guided mutation | **Scaffolded** in `fuzz/` (nightly + `cargo install cargo-fuzz` to run) | Medium — needs CPU-hours of nightly fuzzing budget; not yet wired to CI |
| `cargo audit` / `npm audit` in CI | Not wired (lockfiles now present for all JS packages → auditable) | Medium — supply-chain vulns can land silently |
| External security audit | Not done | This document is the prep packet; see §8 for the internal review |
| Published-package contents | Curated via each package's `files` allow-list; `SHASUMS256.txt` emitted at build | Low |
| DoS limits on input size | **Nesting depth guarded** (parser/codegen error cleanly past ~1000 levels; WASM over-deep subscripts route to JS — no more stack-overflow crash). Raw file *size* still unbounded | Low — `pyths compile` of a multi-GB `.ps` file will OOM |
| Sandboxed runtime helper review (DOM helpers re: trusted-types CSP) | Not done | Low — adding trusted-types support is on the roadmap |
| **Generated-output ownership markers are forgeable by design** | **Documented trust model** (see §9): the `@generated` text header and the `pythscribe.generated` WASM custom section are **accident prevention** — they stop the compiler from clobbering a hand-written or foreign file. They are **not** a security boundary against a malicious process that can already write the build directory (such a process can forge the marker — or write the destination directly). That adversary belongs to filesystem permissions / the OS trust boundary, not to any marker scheme | Accepted — out of scope by trust model (§1) |
| Windows: no `O_NOFOLLOW` | The fd-verified writer opens no-follow on unix only; Windows relies on the pre-open `symlink_metadata` + the on-fd ownership/identity proof. Symlink creation on Windows is privilege-gated (admin / developer mode) | Low — documented platform residual |
| Case-insensitive destination aliasing outside Windows/macOS defaults | Alias preflight case-folds on Windows and macOS; a case-insensitive mount on Linux (or case-sensitive APFS) is not modeled — the per-destination exclusive-create/fd checks still fail closed on the real collision, only later in the write sequence | Low — documented platform residual |

---

## 7. Acknowledgements

Reporters are credited in [`REVIEWERS.md`](../REVIEWERS.md) under each release's "Security" subsection. If you would prefer anonymous credit or no credit at all, say so in your report.

---

## 8. Security + code-review campaign (2026-07-31)

Before the public release, the compiler + distribution underwent two independent reviews — a **security review** (framed with OWASP Top 10, MITRE ATT&CK, and CWE) and a **comprehensive code review** — followed by triage, ensemble, and a fix campaign. Every confirmed critical/high finding was fixed and merged, each reproduced with a concrete trigger before and after and gated on the full `cargo test --workspace` suite.

### Fixed (all merged; PRs in parentheses)

| Area | Finding | OWASP / CWE | PR |
|---|---|---|---|
| Codegen | Arbitrary-JS injection via ad-hoc `format!("\"{}\"")` sites (import side-effect, `[npm.imports]` config, dataclass `choices`, PSX `style` key) bypassing the escapers | A03 / CWE-94, CWE-116 | #414 |
| CLI | Compile-cache substitution + arbitrary file read (in-tree `.pyths/cache`) | A08 / CWE-345, CWE-22 | #416 |
| Codegen | `__pyparams__` reserved-word + Next.js-rename miscompile (`def f(default=…)`, `generateMetadata`) | — | #415 |
| Runtime | Inline (`pyths run`) vs package drift — 6+ behavioral divergences | — | #417 |
| CLI | `fmt` destroyed non-4-space-indented files | CWE-73 | #413 |
| Runtime | Diamond-MRO / `list(d)` / `str.format` correctness | — | #421 |
| Parser/codegen | Deep-nesting stack-overflow → clean diagnostic | CWE-674, CWE-400 | #419 |
| Plugins | cwd binary-probe RCE (`./target/…/pyths` executed before PATH) | CWE-426 / ATT&CK T1204 | #418 |
| CLI | Sibling-write clobber/symlink-follow, predictable temp files, PATH-relative `node`/`wasm-opt` | CWE-59, CWE-377, CWE-427 | #422 |
| Supply chain | npm `@pythscribe` scope; SHA-pinned CI actions; lockfiles; SHASUMS; publish-from-CI + `--provenance` | A08 / ATT&CK T1195, CWE-494 | #412, #423, #426 |
| Compiler | Duplicate-decl miscompile, UTF-8 BOM, over-`i128` diagnostic, invalid-Python checks, printer safety | — | #424 |
| Plugin/scaffold | Line-anchored import regex, `create-pyths-app` deps + path validation, `serde_json` deno.json, `--no-sources-content`, `dangerously_set_html` alias | A03 / CWE-116 | #425 |

### New verified-core result (#420)
**Naming-conversion soundness** (Lean): `sanitize_no_reserved_collision` proves no valid Python identifier ever emits as a JS reserved word (against the full ECMA-262 set, kernel-checked); `sanitize_injective` proves the escape is collision-free (no silent shadowing). See `TRUST.md`.

### Clean bill (verified during the review)
- The core string escapers (`escape_js_string`, `escape_template_literal`, f-strings) are correct — the injection findings were all ad-hoc sites that bypassed them (now routed through `js_string_literal`).
- **Zero `unsafe` Rust** in the entire compiler.
- **No ReDoS** — hand-written lexer, no regex crate; the one quadratic JS-side scan (`rewritePsImports`) was anchored (#425).
- **No install-time execution** — no `postinstall`/`preinstall`/`prepare` in any published package.
- The `.psc` expander has no billion-laughs (fixed finite tier sequence, no fixpoint rescan).

### Deferred (documented, non-blocking; `pythscribe-v3.x-target`)
`nonlocal`-no-binding check (needs resolver wired into `check`); BigInt-lexing of `>i128` literals (bounded diagnostic shipped instead); inline set-op canonicalization on interop `Set`s (common ops identical); plugins passing `--sourcemap` unconditionally in build mode; compiler-side import-specifier rewrite; deno scaffold `--allow-read` tightening; `except Exception` unconditional-catch masking (documented in `known-limitations.md`); early-bound loop-var closure capture (deviation D4).

---

## 9. Generated-output write safety — plugins/CLI hardening campaign (2026-08-12 → 2026-08-14, ships in v0.2.2)

Six adversarial review rounds (codex) against the build plugins + CLI file-writing surface. Rounds 1–3 patched per call site; rounds 4–6 closed the **classes** by consolidating into single abstractions (round 6: file-identity binding + drive-relative resolution). This section doubles as the v0.2.2 changelog entry for this work.

**What ships (the consolidated design):**

- **One safe-write API** (`pyths_cli::commands::safewrite`): every generated output — js, d.ts, map, wasm, glue, certs, edge entry, scaffolds, bundle — is declared to `OutputPlan::preflight`, which checks the COMPLETE output graph before any write (existence, ownership, symlink, dir-vs-file, hard-link count, destination aliasing via canonicalization + Windows/macOS case-folding). Writes go through one fd-verified path: open no-follow (`libc::O_NOFOLLOW`, per-arch), fstat the fd (regular file, link count exactly 1 — enforced on unix via fstat and on Windows via `GetFileInformationByHandle` through the safe `winapi-util` wrapper), prove ownership/identity **on that same fd**, then truncate + write on the same handle. Fresh creations are `O_EXCL`. There is **no proof-free overwrite path**: marked text proves the `@generated` header on the fd; `.wasm` proves its `pythscribe.generated` custom section (name + contents) on the fd; marker-less sidecars (`.map`, `.cert.json`) and `--force` overwrites prove **byte-identity** with what preflight authorized. Round 6: every overwrite proof additionally carries the **file identity** (dev+inode on unix, volume-serial+file-index on Windows) captured at preflight from the same handle that read the evidence, re-verified on the writing fd — so only the *exact file* preflight inspected can be overwritten, never a different-but-also-marked (or byte-identical) file renamed into place afterwards. `wasm-opt`'s optimized module is placed through the same writer (`OutputPlan::rewrite`), never a raw rename, and that placement is bound to the identity of the file the same invocation wrote.
- **One private-temp-dir mechanism** (`tempfile`, both crates): OS-RNG names, explicit mode 0700 handed to `mkdir` itself (immune to a permissive umask), RAII cleanup on every exit path including failed spawns. Precisely: the destructor removes the directory **at its recorded pathname** — it is not identity-bound to the directory it created, so an adversary who renames the dir away can leak the original and point the removal at a successor; that adversary is the same-directory writer, i.e. the OS-permission trust boundary, and the RNG name + 0700 keep the contents private regardless.
- **One exe resolver per language** (`procutil::resolve_program` / `pyths-safe.js::searchPath`): absolute-only — relative env-var overrides are refused, relative `PATH` entries are skipped, the cwd is never searched, and every spawned program path is absolute. Round 6: "not a bare name" is decided by path-component parsing, not separator sniffing, so Windows **drive-relative** values (`PYTHS_NODE=C:evil.exe` — no separator, resolves against drive C's cwd), rooted-driveless values (`\evil.exe`), and ADS-style names (`prog:ads`) are refused; only truly absolute paths (`C:\…`, `\\?\C:\…`, UNC) are honored as overrides. Final round: the JS side (`PYTHS_BIN` / the `pythsBin` plugin option / `PYTHS_DEV_BIN`) enforces the same rule — a relative or drive-relative value throws instead of resolving against the project cwd, and `searchPath` refuses non-bare names and skips non-truly-absolute `PATH` entries, mirroring `procutil.rs`.
- **JS plugin safe API** (`pyths-safe.js`, byte-identical in both plugin packages): same fd-verified ordering, link count exactly 1, short-write loop.

**Trust model (the honest part):** the ownership markers are **accident prevention, not an adversary boundary** — see the §6 row. A local process that can write your build directory is outside what any marker can defend against; the writer's guarantees are the narrower, real ones, stated precisely: no-follow opens on POSIX (`O_NOFOLLOW`; **Windows has no `O_NOFOLLOW`** — the writer there relies on the pre-open `symlink_metadata` check plus the fact that symlink creation on Windows is privilege-gated, so a symlink swapped in between inspection and open is a documented residual, not a defended case); never truncate before the identity + ownership proof passes on the writing fd; never write an undeclared destination. Preflight-all-then-write **reduces but does not eliminate** partial-artifact risk: writes are sequential, so a *mid-sequence* failure (ENOSPC, a destination appearing between writes) leaves the earlier outputs of that build in place — what is guaranteed is that no *refusal the preflight can detect* surfaces after a partial write.

**Platform residuals (documented, accepted):** Windows lacks `O_NOFOLLOW` (symlink creation there is privilege-gated); alias case-folding covers Windows/macOS defaults only; the file-identity check assumes the filesystem reports stable file IDs (true of NTFS/ext4/APFS — on an exotic filesystem without them the check degrades to the evidence proofs). Behavior note: a `.wasm` built by ≤ v0.2.1 predates the ownership section, so its first rebuild under v0.2.2 needs `--force` once (the error says exactly this); all later rebuilds are free.
