#!/usr/bin/env bash
# pip_suite_gate.sh — THE single authority for the pythscribe pip gate.
#
# Runs the FULL `pytest tests/pythscribe` suite with the exact prerequisites and env the
# gate needs, in an ISOLATED venv. Invoked by BOTH:
#   * scripts/release.sh Phase 6  — on the SCRUBBED public tree, fail-closed, BEFORE any push
#   * .github/workflows/ci.yml     — the `pythscribe pip gate` job
# so the local release gate and CI cannot drift. (v0.2.5 lesson: the pip suite lived ONLY in
# ci.yml — a brand-new job that had never run — so it gated NOTHING until tag time; 50+
# failures surfaced only on the mirror. A new CI gate MUST also be a local release gate.)
#
# Env overrides:
#   PIP_GATE_PYTHON   base interpreter to build the venv from (default: `py -3.14`; CI: `python`)
#   PIP_GATE_VENV     venv dir (default: sibling of the tree, per-tree so `-e .` stays correct)
#   PIP_GATE_WITH_DEPS=1  pass `--with-deps` to `playwright install` (Linux CI; needs sudo/apt)
#   PIP_GATE_FAST=1   INCREMENTAL: reuse the venv/deps/crate-cache from a prior FULL run — skips
#                     cargo fetch + venv create + pip install + playwright install. Still does the
#                     (incremental) compiler build + artifact byte-identity + payload prep + pytest.
#                     Use for tight edit->test loops AFTER one full run has provisioned everything.
#   PIP_GATE_PYTEST_PATHS  pytest target(s) (default: `tests/pythscribe`) — narrow to one file for
#                          incremental testing, e.g. `tests/pythscribe/test_streamlit_callback_e2e.py`
#   PIP_GATE_PYTEST_ARGS  extra args appended to the pytest invocation (e.g. `-k test_m0 -x`)
#   PIP_GATE_BINARYEN_VERSION  binaryen release to provision when no `wasm-opt` is resolvable
#                              (default: version_123; a GitHub release tarball into $VENV/binaryen)
#
# binaryen `wasm-opt` is a REQUIRED oracle of the M3 optimizer gates (F2/F3/F4/F5/F6/F10 FAIL under
# PYTHSCRIBE_REQUIRE_ORACLE=1 without it). The setup path provisions it (apt on Linux when sudo is
# non-interactive, else the pinned release tarball into $VENV/binaryen); the committed-artifact
# rebuild below runs with the optimizer OFF (the `.no-wasm-opt` sentinel) so committed artifacts stay
# UNOPTIMIZED + byte-identical while the M3 tests use the real wasm-opt in their own temp dirs.
#
# Incremental example (after one full run):
#   PIP_GATE_FAST=1 PIP_GATE_PYTEST_PATHS=tests/pythscribe/test_wheel_m2.py \
#     PIP_GATE_PYTEST_ARGS="-k doctor" bash scripts/pip_suite_gate.sh
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"; cd "$ROOT"
ORACLE="${PIP_GATE_PYTHON:-py -3.14}"
# Normalized (no `..`): `test -d` resolves `..` but a PATH entry containing `..` does NOT resolve via
# `command -v`, so a `$ROOT/../` venv silently hid the provisioned $VENV/binaryen/bin in FAST mode.
VENV="${PIP_GATE_VENV:-$(dirname "$ROOT")/.pipgate-venv-$(basename "$ROOT")}"
BINARYEN_VERSION="${PIP_GATE_BINARYEN_VERSION:-version_123}"
banner() { echo; echo "-------------------- pip-gate: $* --------------------"; }

# A binaryen provisioned by a prior full run lives beside the venv; put it on PATH in BOTH modes
# (FAST reuses it exactly like the venv), PREPENDED so the pinned build wins over any ambient wasm-opt.
# NOTE: convert to a unix-style path first -- a Windows `C:/...` PATH entry contains a colon that
# Git Bash's PATH parser treats as the `:` separator, silently corrupting the entry (cygpath is a
# no-op / absent on Linux CI, where paths have no drive-letter colon).
_pathify() { if command -v cygpath >/dev/null 2>&1; then cygpath -u "$1"; else printf '%s' "$1"; fi; }
# PREPEND (not append): the PINNED wasm-opt must WIN over any ambient one so every environment runs
# the SAME binaryen version -- the M2/M3 optimizer tests are version-sensitive (id + behavior), and an
# apt `/usr/bin/wasm-opt` (a) is a DIFFERENT version and (b) sits in a system dir the absent-optimizer
# tests cannot strip from PATH (they only remove $VENV/binaryen), so it breaks BOTH present and absent
# expectations. Keeping wasm-opt exclusively under $VENV/binaryen keeps it version-pinned AND strippable.
[ -d "$VENV/binaryen/bin" ] && export PATH="$(_pathify "$VENV/binaryen/bin"):$PATH"

provision_binaryen() {
  # ALWAYS the pinned GitHub release tarball into $VENV/binaryen -- NEVER apt (`/usr/bin`): apt's
  # version drifts (CI's is 108 vs the pinned 123) AND /usr/bin is unstrippable by the absent-optimizer
  # tests. One pinned, strippable, per-venv wasm-opt everywhere (local + CI).
  local os arch tag
  case "$(uname -s)" in
    Linux)  os=linux ;;
    Darwin) os=macos ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) os=windows ;;
    *) echo "pip-gate: cannot provision binaryen for $(uname -s); install wasm-opt on PATH" >&2; return 1 ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) if [ "$os" = macos ]; then arch=arm64; else arch=aarch64; fi ;;
    *) echo "pip-gate: cannot provision binaryen for $(uname -m); install wasm-opt on PATH" >&2; return 1 ;;
  esac
  tag="binaryen-${BINARYEN_VERSION}-${arch}-${os}"
  local url="https://github.com/WebAssembly/binaryen/releases/download/${BINARYEN_VERSION}/${tag}.tar.gz"
  echo "[binaryen] fetching $url"
  rm -rf "$VENV/binaryen" "$VENV/binaryen.tar.gz"
  curl -fsSL --retry 3 -o "$VENV/binaryen.tar.gz" "$url"
  mkdir -p "$VENV/binaryen"
  tar -xzf "$VENV/binaryen.tar.gz" -C "$VENV/binaryen" --strip-components=1
  rm -f "$VENV/binaryen.tar.gz"
  export PATH="$(_pathify "$VENV/binaryen/bin"):$PATH"
}

banner "compiler (cargo build --release --bin pyths)"
cargo build --release --bin pyths
BIN="$ROOT/target/release/pyths"; [ -f "$BIN.exe" ] && BIN="$BIN.exe"
[ -f "$BIN" ] || { echo "pip-gate: compiler not found at $BIN" >&2; exit 1; }

# venv python path — computed regardless of mode (FAST reuses a venv from a prior full run)
VPY="$VENV/bin/python"; [ -f "$VENV/Scripts/python.exe" ] && VPY="$VENV/Scripts/python.exe"

if [ "${PIP_GATE_FAST:-0}" = "1" ]; then
  banner "FAST mode — reusing venv + deps + crate cache from a prior full run ($VENV)"
  [ -x "$VPY" ] || { echo "pip-gate FAST: no venv at $VENV — run a full gate once first (drop PIP_GATE_FAST)" >&2; exit 2; }
else
  banner "cross-target crate cache (M5 third-party closure: cargo metadata --offline per target)"
  # The M5 inventory test runs `cargo metadata --offline --filter-platform <triple>` for EACH of
  # the 5 release targets; the windows/macos-only crates (e.g. anstyle-wincon) are absent from the
  # host build's registry cache, so pre-fetch every target's graph while online.
  cargo fetch --target aarch64-apple-darwin --target aarch64-unknown-linux-gnu \
              --target x86_64-apple-darwin --target x86_64-pc-windows-msvc \
              --target x86_64-unknown-linux-gnu

  banner "isolated venv + deps ($VENV)"
  $ORACLE -m venv "$VENV"
  "$VPY" -m pip install -q --upgrade pip
  # gradio_wasmfunction is VENDORED into the wheel (M1.5 §E) — NEVER install it as a separate
  # distribution (that makes test_p3 RED). `build`+`setuptools`: the M1 sdist gate needs them
  # (3.14 venvs no longer ship setuptools). numba+streamlit ride in the `test` extra.
  "$VPY" -m pip install -e ".[gradio,test]" build setuptools
  if [ "${PIP_GATE_WITH_DEPS:-0}" = "1" ]; then
    "$VPY" -m playwright install --with-deps chromium
  else
    "$VPY" -m playwright install chromium
  fi

  banner "binaryen wasm-opt (M3 optimizer oracle) — pinned $BINARYEN_VERSION, never ambient/apt"
  # Always resolve to the PINNED build under $VENV/binaryen (prepended to PATH above). Reuse it if a
  # prior full run already fetched it into this venv; otherwise fetch. NEVER fall through to an ambient
  # system wasm-opt (e.g. an apt `/usr/bin/wasm-opt`): it is the wrong version AND unstrippable, which
  # is precisely what breaks the version-sensitive M2/M3 tests on CI. Pinning here is the whole point.
  if [ -x "$VENV/binaryen/bin/wasm-opt" ] || [ -x "$VENV/binaryen/bin/wasm-opt.exe" ]; then
    echo "[binaryen] reusing pinned $VENV/binaryen/bin/wasm-opt ($("$VENV/binaryen/bin/wasm-opt" --version 2>/dev/null || echo '?'))"
  else
    provision_binaryen
  fi
fi

# The M3 gates REQUIRE the real optimizer (PYTHSCRIBE_REQUIRE_ORACLE=1 turns its absence into a
# FAILURE, never a skip) -- fail here, loudly, rather than 6 tests deep.
command -v wasm-opt >/dev/null 2>&1 \
  || { echo "pip-gate: binaryen \`wasm-opt\` is not resolvable (FAST mode reuses \$VENV/binaryen from a full run -- run one, or install wasm-opt on PATH)" >&2; exit 2; }
echo "[binaryen] wasm-opt = $(command -v wasm-opt) ($(wasm-opt --version))"

export PYTHSCRIBE_PYTHS="$BIN"

banner "committed-artifact byte-identical rebuild (forced, optimizer OFF)"
# The compiler stamp / any codegen change invalidates committed example __pythscribe__/ artifacts;
# rebuild with the just-built compiler and assert byte-identity (L1 layout gate + E2E use these).
# Committed artifacts are UNOPTIMIZED by contract: the rebuild runs with the optimizer OFF via the
# sanctioned sentinel (`PYTHS_WASM_OPT=<abs>/.no-wasm-opt` -> a SILENT `none`, manifest identical to
# an optimizer-free machine's; pythscribe/build/optimizer.py::NO_WASM_OPT_NAME, gate F10) so the
# wasm-opt provisioned above for the M3 tests cannot turn this rebuild into an optimized (drifted) one.
NO_OPT_SENTINEL="$ROOT/.no-wasm-opt"
[ -e "$NO_OPT_SENTINEL" ] && { echo "pip-gate: $NO_OPT_SENTINEL exists; the OFF sentinel must be a MISSING path" >&2; exit 1; }
DIRS="$(git ls-files '**/__pythscribe__/**/manifest.json' | sed -E 's#/__pythscribe__/.*##' | sort -u)"
for d in $DIRS; do
  [ -f "$d/kernels.py" ] || continue
  echo "[artifacts] rebuild $d/kernels.py"
  PYTHS_WASM_OPT="$NO_OPT_SENTINEL" "$VPY" -m pythscribe.build --force "$d/kernels.py"
done
git diff --exit-code -- $(printf '%s/__pythscribe__ ' $DIRS) \
  || { echo "pip-gate: committed artifacts drifted from the compiler output — rebuild + commit" >&2; exit 1; }

banner "scaffolder payload (M7 gate needs dist/scaffolder-payload)"
"$VPY" scripts/prepare_release_payloads.py

PYTEST_PATHS="${PIP_GATE_PYTEST_PATHS:-tests/pythscribe}"
# Parse PIP_GATE_PYTEST_ARGS respecting shell quoting, so a `-k 'a or b'` expression survives as ONE
# arg (a bare ${VAR} word-splits it into `-k`, `a`, `or`, `b`). PYTEST_PATHS stays word-split on
# purpose (multiple space-separated paths/node-ids -> multiple args).
extra_args=(); [ -n "${PIP_GATE_PYTEST_ARGS:-}" ] && eval "extra_args=($PIP_GATE_PYTEST_ARGS)"
banner "pip suite: pytest $PYTEST_PATHS"
# tmp_path_retention_policy=failed: delete each PASSING test's tmp dir immediately (keep FAILED ones
# for debugging). ~50 tests each build a wheel + an isolated venv (~200-400MB); pytest's default keeps
# ALL of them for the whole session, accumulating to 15-20GB and exhausting a modest local disk. This
# is hygiene, not a skip -- every test still runs; CI (ample disk) is unaffected.
PYTHSCRIBE_REQUIRE_ORACLE=1 \
PYTHSCRIBE_EMBED_BINARY="$BIN" PYTHSCRIBE_REQUIRE_EMBED=1 \
PYTHSCRIBE_PYTHS="$BIN" \
  "$VPY" -m pytest $PYTEST_PATHS -v -ra -p no:cacheprovider -o tmp_path_retention_policy=failed "${extra_args[@]}"
