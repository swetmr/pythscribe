---
name: compressing-ps-to-psc
description: Use when authoring `.psc` (compressed PythScribe) files by hand from canonical `.ps` source, or when porting a `.ps` file to `.psc` to validate the compression pipeline. Covers Tier A/B/C/D + dictionary aliases, the mandatory expand-and-diff verification, and the layered authoring workflow. Triggers on terms like ".psc", "compressed pythscribe", "psc compression", "Tier A/B/C/D", "pyths expand", "alias".
---

# Compressing PythScribe to `.psc`

## Overview

`.psc` is a **compressed superset** of canonical PythScribe (`.ps`) designed for LLM token efficiency. A `.psc` file expands deterministically into canonical `.ps` before the compiler pipeline runs. Every alias is optional and individually disable-able.

**Core principle — the Iron Rule of `.psc` authoring:**

> Every `.psc` file MUST round-trip back to its canonical `.ps` source byte-identically. If `pyths expand foo.psc` does not match `foo.ps` exactly, the `.psc` is broken — regardless of whether it "looks right".

Without this check, alias collisions and string-literal substitutions can silently change program meaning. This is the only test that matters.

## When to Use

- Authoring `.psc` from scratch as an LLM-emitted file (token-constrained tool boundary)
- Porting a working `.ps` file to `.psc` to validate the compression pipeline at app scale (Phase 3 matrix cell)
- Adding project-local dictionary aliases (`pyths.toml [expand.dictionary]`)
- Debugging an unexpected diff between `.psc` and its expanded `.ps`

**When NOT to use:**
- Authoring code that humans will read primarily — stick with `.ps`. The aliases save tokens, not human time.
- Performance-critical hot paths where the ~50µs expander pass matters (rare).

## The Mandatory Verification Loop

Run this for **every** edit. There is no "I'll check at the end" — alias bugs compound.

```powershell
& 'C:\Users\DELL\pythscribe-src\target\release\pyths.exe' expand foo.psc -o foo.expanded.ps
# Now diff foo.expanded.ps against the canonical foo.ps:
$a = Get-Content foo.ps -Raw
$b = Get-Content foo.expanded.ps -Raw
if ($a -eq $b) { 'OK' } else { 'DIFF — fix the .psc' }
```

If the diff is non-empty, **read it before doing anything else**. Common causes are documented in "Common mistakes" below.

## Tier Reference

Apply tiers in this order (A → B → C → Dict). Tier D (PySON JSON-AST) is for schema-constrained tool-use only, not hand authoring.

### Tier A — Import presets + decorator aliases

Whole-line markers. Replace exactly one canonical import line each.

| Marker | Expands to |
|---|---|
| `R*` | `from pyths.react import component, use_state, use_effect, use_callback, use_memo` |
| `R+` | adds `forwardRef, memo, Suspense, lazy` |
| `T*` | `from dataclasses import dataclass` |
| `T+` | adds `Field` |
| `A*` | `from pyths.asyncio import gather, sleep` |
| `D*` | DOM helpers from `pyths.dom` |
| `W*` | `from pyths.web import handler, Response` |

Decorator slots: `@c`=`@component`, `@d`=`@dataclass`, `@v`=`@validator`, `@h`=`@handler`, `@k`=`@check`. Call form works: `@d(coerce=True)`.

**Gotcha:** `R*` includes `use_state, use_effect, use_callback, use_memo`. If your canonical `.ps` imports a strict subset (e.g. only `component, use_state`), the expander will produce a *different* import line and **the round-trip diff will fail**. In that case, write the import out canonically instead of using `R*`.

### Tier B — Kwarg-position aliases + hook-call shorthand

Substitutes ONLY inside function-call argument position (after `(` or `,`, before `=`). The expander state-machine refuses to substitute inside string literals, comments, f-strings, or top-level statements.

| Kwarg | Canonical |
|---|---|
| `st=` | `style=` |
| `cn=` | `class_name=` |
| `cl=` | `className=` |
| `oc=` | `on_click=` |
| `oh=` | `on_change=` |
| `os=` | `on_submit=` |
| `oa=` | `on_blur=` |
| `ph=` | `placeholder=` |
| `dis=` | `disabled=` |

Hook calls (requires trailing `(`):

| Hook | Canonical |
|---|---|
| `us(` | `use_state(` |
| `ue(` | `use_effect(` |
| `um(` | `use_memo(` |
| `uc(` | `use_callback(` |
| `ur(` | `use_ref(` |
| `ux(` | `use_context(` |

### Tier C — PSX tag-DSL

JSX-like angle brackets expand to canonical PSX call form:

```python
<div .foo #main st={"padding": "4px"}>{label}</div>
# →
div(className="foo", id="main", style={"padding": "4px"})(label)
```

- `.foo` → `className="foo"`. Multiple: `.foo .bar` → `className="foo bar"`
- `#main` → `id="main"`
- Self-closing: `<input oh=set/>` → `input(on_change=set)`

**Gotcha:** PSX inside comparisons (`x < 7`), strings, and comments is left alone — but if your canonical `.ps` uses the flat call form `div(prop=v, child)`, converting it to `<div prop=v>{child}</div>` produces a *different* canonical form on expand. The expander emits the curried form `div(prop=v)(child)`, not the flat form. **Only use Tier C if your canonical `.ps` is already in curried form.**

### Tier Dictionary — `$NAME` string-literal aliases

`$NAME` tokens expand to canonical string literals. Bundled aliases (partial):

| Alias | Canonical |
|---|---|
| `$c1` | `"#9ca3af"` (gray-400) |
| `$c2` | `"#ffffff"` |
| `$c4` | `"#3b82f6"` (blue-500) |
| `$p1` | `"12px"` |
| `$p4` | `"16px"` |
| `$pad` | `"padding"` |
| `$bg` | `"background"` |
| `$ff` | `"system-ui, sans-serif"` |
| `$gtc` | `"grid_template_columns"` |

Full bundled table: `crates/pyths_expand/src/strings.rs` in the pythscribe-src repo.

Project-local additions go in `pyths.toml` at the project root:

```toml
[expand.dictionary]
API_BASE = "http://localhost:8000"
BRAND_GRAY = "#9ca3af"
```

Used in `.psc`: `url = $API_BASE`. User entries override bundled aliases of the same name.

**CWD gotcha — this is the silent failure mode:** the expander walks **upward from the current working directory** looking for `pyths.toml` (Cargo-style discovery). If you run `pyths expand src/components/Foo.psc` from a directory that is NOT under the dir containing `pyths.toml`, project-local aliases will not be found, every `$NAME` will pass through verbatim, and the round-trip diff will report `$API` ≠ `"http://localhost:8000"`. The expander does NOT walk from the *input file's* directory — it walks from your shell's CWD. Always `cd` to the PythScribe project root (or below it) before invoking `pyths expand` or `pyths compile`. Build plugins (Vite, Next.js) get this right automatically; manual invocations and verification scripts are where it bites.

**Because `$` is not a valid Python token character, there is zero risk of colliding with user identifiers.** Unknown `$NAME` aliases pass through verbatim.

## Authoring Workflow (Layered, Verify-Each-Pass)

Start from a working `.ps` file. Apply ONE tier per pass, verify before moving on.

1. **Copy** `foo.ps` → `foo.psc`. Verify the no-op baseline: `pyths expand foo.psc` matches `foo.ps`.
2. **Tier A pass** — replace import lines with presets *only if the full preset matches the canonical imports*. Replace decorators (`@component` → `@c`, etc.). Verify round-trip.
3. **Tier B pass** — substitute kwargs and hook calls in *function-call positions only*. Verify round-trip.
4. **Tier C pass** *(optional)* — only if canonical form is already curried `tag(...)(...)`. Verify round-trip.
5. **Dictionary pass** — replace duplicated string literals with `$NAME` aliases (bundled or project-local). Verify round-trip.
6. **Measure** — `(Get-Item foo.psc).Length` vs `(Get-Item foo.ps).Length` for bytes saved. For tokens, use `tiktoken` cl100k.

If a pass breaks the round-trip, the previous pass was fine — revert just that pass.

## Common Mistakes

| Symptom in diff | Cause | Fix |
|---|---|---|
| Import line has extra symbols (`use_callback`, `use_memo`) | `R*` used when canonical imports only `component, use_state` | Write the import canonically; don't use `R*` for subsets |
| `div(prop=v, child)` vs `div(prop=v)(child)` | Used Tier C `<div prop=v>{child}</div>` but canonical was flat-form | Either rewrite canonical to curried form, or skip Tier C |
| `$NAME` appears verbatim in expanded output (e.g. `url = $API` not `url = "http://..."`) | `pyths.toml` not found — CWD is not under its directory | `cd` to the PythScribe project root (or any subdirectory) before running `pyths expand` / `pyths compile`. Discovery walks from CWD, not from the input file's path |
| String literal changed inside a comment | Dictionary alias matched inside a non-substitution zone — shouldn't happen, but verify if it does | File issue against `pyths_expand`; revert that alias |
| `oc=` replaced with `on_click=` inside an f-string | The state machine should prevent this — if it doesn't, it's a bug | Revert the alias for that line; file issue |
| `us(` not expanded | Missing trailing `(` — `us` as bare identifier is intentionally untouched | Add the `(`, or write `use_state` |
| Whole file unchanged | Forgot `.psc` extension on the input | Rename or use `--expand=always` |

## Quick Reference: Verify Round-Trip

```powershell
# CD to the PythScribe project root FIRST — the expander discovers
# `pyths.toml` by walking upward from CWD, not from the input file's path.
Set-Location 'C:\Users\DELL\reference-app\frontend'

$pyths = 'C:\Users\DELL\pythscribe-src\target\release\pyths.exe'
& $pyths expand src/components/Foo.psc -o /tmp/Foo.expanded.ps
$ok = (Get-Content src/components/Foo.ps -Raw) -eq (Get-Content /tmp/Foo.expanded.ps -Raw)
if (-not $ok) {
    # Show first diff
    Compare-Object (Get-Content src/components/Foo.ps) (Get-Content /tmp/Foo.expanded.ps) | Select-Object -First 10
}
```

## Red Flags — STOP and Re-Verify

- Edited `.psc` and skipped the expand-diff check ("looks fine to me")
- Applied multiple tiers in one pass without verifying each
- Used `R*` to "match closely enough" — the diff is the only judge of "closely enough"
- Saw a diff in `Compare-Object` output and continued editing without understanding it
- Ran `pyths expand` from the repo root when `pyths.toml` is in a subdirectory like `frontend/` — verify CWD before debugging "broken" aliases

All of these mean: revert the most recent change. Re-verify before going on.

## Reference

- Authoritative docs: `C:\Users\DELL\pythscribe-src\docs\compression.md`
- Bundled dictionary source: `crates/pyths_expand/src/strings.rs`
- 154+ library tests: `crates/pyths_expand/src/lib.rs`
- CLI integration tests: `crates/pyths_cli/tests/cli_test.rs` (search `psc_`)
