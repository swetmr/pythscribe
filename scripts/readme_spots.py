#!/usr/bin/env python3
"""M6.4 -- README SPOTs: the README's install + usage claims RUN against the shipped wheel (spec 13-09-26; validation §K).

    python scripts/readme_spots.py --static [--readme README.md] [--pyproject pyproject.toml | --wheel W]
    python scripts/readme_spots.py --run [--python PY] [--wheel W] [--node] [--pip-arg ...]

STATIC (runs anywhere, incl. the pre-tag gate -- §K-platform):
  * every `pythscribe[<extra>]` the README mentions is a DECLARED extra (pyproject `[project.optional-dependencies]`,
    or the wheel's `Provides-Extra` with --wheel): `pythscribe[nope]` / a resurrected `[web]` / `[optimize]` -> RED;
  * the environment table (requirements §3.1): the "Without Node" row names only shipped capabilities (JS+WASM
    compile, `@wasm`, Gradio, Streamlit) and never a With-Node one; `MCP` appears there ONLY suffixed "(v0.3)";
    the "With Node" row names the frontend capabilities (`pyths new`, dev server, HMR, npm imports);
  * the "Prebuilt wheels: ..." platform list == EXPECTED_WHEEL_SET (one committed word map; an OS/arch the
    matrix does not build -> RED; a matrix target the README omits -> RED) and the source-install note is present.
RUN (the acceptance job; needs the wheel installed in --python's environment, or --wheel):
  * every fenced block under "## Installation" / "## Quick Start" preceded by `<!-- spot -->` is executed in a
    fresh workspace OUTSIDE the checkout, node-free PATH: `pip install pythscribe[...]` lines are bound to the
    SHIPPED wheel (`"<wheel>[...]"` -- the wheel path from --wheel or the installed distribution's direct_url.json;
    no wheel => RED, a SPOT must bind to shipped bytes), `uv pip install pythscribe[...]` lines are bound the SAME
    way and run with the REAL `uv` (`--python <target>`) when one is on the host's PATH, else deferred (never
    passed via pip), `pyths ...` runs the installed console script, `python ...` the target interpreter; `# → line`
    comments assert stdout; a python block needs a `# <name>.py` header and is run as that file. Blocks marked
    `<!-- spot: node -->` need Node and run ONLY with --node (M7's web-parity job); the node-free run reports
    them as deferred, never as passed.
Exit 0 GREEN / 1 RED (every failing claim printed).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.parse
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
from pythscribe.build._native import EXPECTED_WHEEL_SET  # noqa: E402

SPOT_SECTIONS = re.compile(r"install|quick start", re.I)
MARKER = re.compile(r"^\s*<!--\s*spot(?::\s*(?P<kind>[\w-]+))?\s*-->\s*$")
FENCE = re.compile(r"^(\s*)```(\S*)\s*$")
EXTRA_MENTION = re.compile(r"pythscribe\[([A-Za-z0-9_,\- ]+)\]")
FORBIDDEN_MENTIONS = ("pythscribe[web]", "[optimize]")
# tag -> (OS word, arch word) as the README's "Prebuilt wheels:" sentence spells them (ONE committed map, §K-platform)
PLATFORM_WORDS: dict[str, tuple[str, str]] = {
    "manylinux_2_28_x86_64": ("Linux", "x86_64"),
    "manylinux_2_28_aarch64": ("Linux", "aarch64"),
    "macosx_11_0_x86_64": ("macOS", "x86_64"),
    "macosx_11_0_arm64": ("macOS", "arm64"),
    "win_amd64": ("Windows", "x64"),
}
ARCH_WORDS = ("x86_64", "aarch64", "arm64", "x64", "i686", "armv7", "ia32", "riscv64", "s390x", "ppc64le")
GLIBC_FLOOR = "glibc 2.28"
SOURCE_NOTE = ("builds from source", "without", "the compiler")
WITHOUT_NODE_REQUIRED = ("JS and WASM", "@wasm", "Gradio", "Streamlit")
WITHOUT_NODE_FORBIDDEN = ("pyths new", "dev server", "HMR", "npm")
WITH_NODE_REQUIRED = ("pyths new", "dev server", "HMR", "npm")
SYSTEM_PATH = "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"


@dataclass
class Block:
    line: int
    lang: str
    code: str
    kind: str = "spot"  # "spot" (node-free) | "node"
    section: str = ""


@dataclass
class Report:
    problems: list[str] = field(default_factory=list)
    passed: list[str] = field(default_factory=list)
    deferred: list[str] = field(default_factory=list)

    def ok(self) -> bool:
        return not self.problems


# ----------------------------------------------------------------------------- extraction


def extract_spots(text: str) -> tuple[list[Block], list[str]]:
    """Spot blocks + authoring problems (a marker outside an Installation/Quick Start H2, an unterminated fence)."""
    lines = text.split("\n")
    blocks: list[Block] = []
    problems: list[str] = []
    section = ""
    pending: tuple[int, str] | None = None  # (marker line, kind)
    open_fence: tuple[int, str, int, list[str], str | None] | None = None
    for i, raw in enumerate(lines, 1):
        m = FENCE.match(raw)
        if open_fence is None and raw.startswith("## "):
            section = raw[3:].strip()
            if pending:
                problems.append(f"README:{pending[0]}: `<!-- spot -->` marker is not followed by a fenced block")
                pending = None
        mk = MARKER.match(raw)
        if mk and open_fence is None:
            if not SPOT_SECTIONS.search(section):
                problems.append(f"README:{i}: `<!-- spot -->` under `## {section}` -- spots live under Installation / Quick Start only")
            pending = (i, (mk.group("kind") or "spot").lower())
            continue
        if m and open_fence is None:
            open_fence = (i, m.group(2).lower(), len(m.group(1)), [], pending[1] if pending else None)
            if pending and (i - pending[0]) > 3:
                problems.append(f"README:{pending[0]}: `<!-- spot -->` marker too far from its fence")
            pending = None
            continue
        if m and open_fence is not None:
            start, lang, indent, body, kind = open_fence
            if kind is not None:
                if kind not in ("spot", "node"):
                    problems.append(f"README:{start}: unknown spot kind {kind!r} (spot | node)")
                blocks.append(Block(line=start, lang=lang, code="\n".join(body), kind=kind, section=section))
            open_fence = None
            continue
        if open_fence is not None:
            open_fence[3].append(raw[open_fence[2]:])
        elif pending and raw.strip():
            problems.append(f"README:{pending[0]}: `<!-- spot -->` marker must directly precede a fenced block")
            pending = None
    if open_fence is not None:
        problems.append(f"README:{open_fence[0]}: unterminated fence")
    return blocks, problems


# ----------------------------------------------------------------------------- static checks


def declared_extras_pyproject(pyproject: Path) -> set[str]:
    return set((tomllib.loads(pyproject.read_text(encoding="utf-8"))["project"].get("optional-dependencies") or {}).keys())


def declared_extras_wheel(wheel: Path) -> set[str]:
    with zipfile.ZipFile(wheel) as z:
        meta = next(n for n in z.namelist() if n.endswith(".dist-info/METADATA"))
        text = z.read(meta).decode("utf-8")
    return {m.group(1).strip() for m in re.finditer(r"^Provides-Extra:\s*(.+)$", text, re.M)}


def check_extras(text: str, declared: set[str]) -> list[str]:
    p: list[str] = []
    for m in EXTRA_MENTION.finditer(text):
        for extra in (x.strip() for x in m.group(1).split(",")):
            if extra and extra not in declared:
                p.append(f"README names the extra `pythscribe[{extra}]` but the wheel declares only {sorted(declared)} -- a claim that cannot hold")
    for bad in FORBIDDEN_MENTIONS:
        if bad in text:
            p.append(f"README mentions `{bad}` -- that extra does not exist in this release")
    return p


def _table_row(text: str, label: str) -> str | None:
    for line in text.split("\n"):
        if line.startswith("|") and label in line.split("|")[1]:
            return line
    return None


def check_env_table(text: str) -> list[str]:
    p: list[str] = []
    without, with_ = _table_row(text, "Without Node"), _table_row(text, "With Node")
    if without is None or with_ is None:
        return ["README lacks the environment table (rows `Without Node` / `With Node`, requirements §3.1)"]
    cell = without.split("|")[2]
    for need in WITHOUT_NODE_REQUIRED:
        if need not in cell:
            p.append(f"Without-Node row does not name `{need}`")
    for bad in WITHOUT_NODE_FORBIDDEN:
        if re.search(r"(?<![\w-])" + re.escape(bad) + r"(?![\w-])", cell):
            p.append(f"Without-Node row claims `{bad}` -- a With-Node capability (a claim the node-free wheel cannot hold)")
    if re.search(r"\bMCP\b", cell) and not re.search(r"\bMCP[^|]{0,40}?\(v0\.3\)", cell):
        p.append("Without-Node row mentions MCP without the \"(v0.3)\" label -- the MCP server is not in this release")
    wcell = with_.split("|")[2]
    for need in WITH_NODE_REQUIRED:
        if need not in wcell:
            p.append(f"With-Node row does not name `{need}`")
    if "pyths doctor" not in text:
        p.append("README lacks the `pyths doctor` pointer")
    return p


def check_platforms(text: str, expected: frozenset[str] = EXPECTED_WHEEL_SET) -> list[str]:
    p: list[str] = []
    m = re.search(r"Prebuilt wheels:\s*(?P<list>.+?)\.(?:\s|$)", text)
    if not m:
        return ["README lacks the \"Prebuilt wheels: ...\" platform list"]
    segments: dict[str, str] = {}
    for seg in (s.strip() for s in m.group("list").split(",")):
        os_word = seg.split()[0] if seg else ""
        segments[os_word] = seg
    want: dict[str, set[str]] = {}
    for tag in expected:
        if tag not in PLATFORM_WORDS:
            p.append(f"expected wheel tag {tag!r} has no README spelling in PLATFORM_WORDS (add the matrix target to the README + this map)")
            continue
        os_word, arch = PLATFORM_WORDS[tag]
        want.setdefault(os_word, set()).add(arch)
    for os_word, archs in sorted(want.items()):
        seg = segments.get(os_word)
        if seg is None:
            p.append(f"README platform list omits {os_word} ({sorted(archs)})")
            continue
        listed = {w for w in re.split(r"[\s/()]+", seg) if w in ARCH_WORDS}
        for a in sorted(archs - listed):
            p.append(f"README platform list omits {os_word} {a} (the matrix builds it)")
        for a in sorted(listed - archs):
            p.append(f"README platform list claims {os_word} {a}, which the release matrix does not build")
        if os_word == "Linux" and GLIBC_FLOOR not in seg:
            p.append(f"README Linux entry does not state the `{GLIBC_FLOOR}+` floor")
    for os_word in sorted(set(segments) - set(want)):
        p.append(f"README platform list claims {segments[os_word]!r}, which the release matrix does not build")
    if not all(s in text for s in SOURCE_NOTE):
        p.append("README lacks the source-install note (\"On any other platform `pip install` builds from source **without** the compiler ...\")")
    return p


def static_checks(text: str, declared: set[str], expected: frozenset[str] = EXPECTED_WHEEL_SET) -> list[str]:
    blocks, problems = extract_spots(text)
    if not blocks:
        problems.append("README has no `<!-- spot -->` blocks under Installation / Quick Start (nothing binds the claims to the wheel)")
    return problems + check_extras(text, declared) + check_env_table(text) + check_platforms(text, expected)


# ----------------------------------------------------------------------------- run mode


def installed_wheel_path(python: Path) -> Path | None:
    """The wheel file the target interpreter's `pythscribe` was installed from (pip's direct_url.json)."""
    code = ("import importlib.metadata as m, json\n"
            "d = m.distribution('pythscribe')\n"
            "t = d.read_text('direct_url.json')\n"
            "print(json.loads(t)['url'] if t else '')")
    # -I (isolated): no cwd / PYTHONPATH on sys.path, so a checkout's egg-info can never shadow the venv's dist-info
    r = subprocess.run([str(python), "-I", "-c", code], capture_output=True, text=True, check=False, cwd=str(Path(python).resolve().parent))
    url = r.stdout.strip()
    if r.returncode != 0 or not url.startswith("file:"):
        return None
    p = Path(urllib.parse.unquote(urllib.parse.urlparse(url).path))
    if os.name == "nt" and re.match(r"^/[A-Za-z]:", str(p).replace("\\", "/")):
        p = Path(str(p).lstrip("/\\"))
    return p if p.is_file() and p.suffix == ".whl" else None


def _steps(code: str) -> list[tuple[str, list[str]]]:
    steps: list[tuple[str, list[str]]] = []
    for raw in code.split("\n"):
        line = raw.strip()
        if not line:
            continue
        arrow = re.match(r"^#\s*(?:→|->)\s?(.*)$", line)
        if arrow:
            if steps:
                steps[-1][1].append(arrow.group(1).rstrip())
            continue
        if line.startswith("#"):
            continue
        cmd = re.split(r"\s+#", line)[0].strip()
        if cmd:
            steps.append((cmd, []))
    return steps


def _bind_pip(tokens: list[str], wheel: Path) -> tuple[list[str], set[str]]:
    """Bind `pythscribe[...]` tokens to the shipped wheel; also return the extras named (pip only WARNS on an
    undeclared extra and exits 0, so the caller must refuse them itself -- the E-K control)."""
    out, extras = [], set()
    for t in tokens:
        m = re.fullmatch(r'"?pythscribe(\[[^\]]*\])?"?', t)
        if m:
            out.append(f"{wheel}{m.group(1) or ''}")
            extras |= {x.strip() for x in (m.group(1) or "[]")[1:-1].split(",") if x.strip()}
        else:
            out.append(t.strip('"'))
    return out, extras


def run_spots(blocks: list[Block], *, python: Path, wheel: Path | None, node: bool, pip_args: list[str]) -> Report:
    rep = Report()
    scripts = Path(python).resolve().parent
    declared = declared_extras_wheel(wheel) if wheel is not None else set()
    ws = Path(tempfile.mkdtemp(prefix="readme-spots-"))
    env = {k: v for k, v in os.environ.items() if k not in ("PYTHONPATH", "PYTHSCRIBE_PYTHS", "PYTHSCRIBE_NO_JIT", "PYTHSCRIBE_MODE")}
    env["PATH"] = os.pathsep.join([str(scripts), SYSTEM_PATH] + ([os.environ.get("PATH", "")] if node else []))
    env["PYTHSCRIBE_CACHE"] = str(ws / "jit-cache")

    def run(argv: list[str], where: str, expected: list[str]) -> None:
        try:
            r = subprocess.run(argv, capture_output=True, text=True, cwd=str(ws), env=env, timeout=900, check=False)
        except (OSError, subprocess.TimeoutExpired) as e:
            rep.problems.append(f"{where}: {' '.join(argv)!r} could not run: {e}")
            return
        if r.returncode != 0:
            rep.problems.append(f"{where}: `{' '.join(argv[-min(len(argv), 6):])}` exited {r.returncode}: {(r.stderr or r.stdout).strip()[-600:]}")
            return
        if expected:
            got = [l.rstrip() for l in r.stdout.splitlines() if l.strip()]
            if got != expected:
                rep.problems.append(f"{where}: expected stdout {expected!r}, got {got!r}")
                return
        rep.passed.append(f"{where}: {' '.join(argv[-min(len(argv), 4):])}")

    for b in blocks:
        where = f"README:{b.line} [{b.lang}]"
        if b.kind == "node" and not node:
            rep.deferred.append(f"{where}: node block -- deferred to the web-parity job (M7); not counted as passed")
            continue
        if b.lang in ("python", "py"):
            header = re.search(r"^#\s*([\w.-]+\.py)\s*$", b.code, re.M)
            if not header:
                rep.problems.append(f"{where}: a python spot needs a `# <name>.py` header line")
                continue
            expected = [m.group(1).rstrip() for m in re.finditer(r"^#\s*(?:→|->)\s?(.*)$", b.code, re.M)]
            (ws / header.group(1)).write_text(b.code + "\n", encoding="utf-8")
            run([str(python), header.group(1)], where, expected)
            continue
        if b.lang not in ("bash", "sh", "shell", "console"):
            rep.problems.append(f"{where}: spot blocks must be bash or python, not {b.lang!r}")
            continue
        for cmd, expected in _steps(b.code):
            for part in [c.strip() for c in cmd.split("&&")]:
                words = part.split()
                w0 = words[0]
                is_pip = w0 == "pip" and words[1:2] == ["install"]
                is_uv = w0 == "uv" and words[1:3] == ["pip", "install"]  # the README's `uv pip install` drop-in line
                if is_pip or is_uv:
                    if wheel is None:
                        rep.problems.append(f"{where}: `{part}` cannot be bound to the shipped wheel (no --wheel and no direct_url.json for the installed pythscribe)")
                        continue
                    # ONE binding + extras refusal for both installers (the E-K control holds on the uv arm too)
                    bound, extras = _bind_pip(words[3:] if is_uv else words[2:], wheel)
                    undeclared = sorted(extras - declared)
                    if undeclared:
                        rep.problems.append(f"{where}: `{part}` names extra(s) {undeclared} the shipped wheel does not declare ({sorted(declared)}); pip would only warn -- refused")
                        continue
                    if is_uv:
                        # Run the REAL uv (a Python-ecosystem tool, resolved from the caller's PATH -- the
                        # node-free PATH only excludes Node) into the SAME target interpreter. Never silently
                        # substitute pip: a host without uv DEFERS the line (like node blocks), never passes it.
                        uv = shutil.which("uv")
                        if not uv:
                            rep.deferred.append(f"{where}: `{part}` -- uv is not installed on this host; deferred, not counted as passed")
                            continue
                        run([uv, "pip", "install", "--python", str(python), *bound, *pip_args], where, expected)
                    else:
                        run([str(python), "-m", "pip", "install", *bound, *pip_args], where, expected)
                elif w0 == "pyths":
                    exe = shutil.which("pyths", path=str(scripts))
                    if not exe:
                        rep.problems.append(f"{where}: no `pyths` console script under {scripts}")
                        continue
                    run([exe, *words[1:]], where, expected)
                elif w0 == "python":
                    run([str(python), *words[1:]], where, expected)
                elif w0 == "cd" and len(words) == 2:
                    ws = ws / words[1]
                    ws.mkdir(parents=True, exist_ok=True)
                elif w0 in ("npm", "npx", "node") and node:
                    run([shutil.which(w0) or w0, *words[1:]], where, expected)
                else:
                    rep.problems.append(f"{where}: `{part}` is not a runnable spot command (pip install / uv pip install / pyths / python; npm/node only with --node)")
    shutil.rmtree(ws, ignore_errors=True)
    return rep


# ----------------------------------------------------------------------------- CLI


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="readme_spots.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--readme", default=str(REPO / "README.md"))
    ap.add_argument("--static", action="store_true", help="the static claim checks only (no execution)")
    ap.add_argument("--run", action="store_true", help="execute the spot blocks against the shipped wheel")
    ap.add_argument("--python", default=sys.executable, help="the interpreter whose environment holds the installed wheel")
    ap.add_argument("--wheel", default=None, help="the shipped wheel (default: the installed distribution's direct_url.json)")
    ap.add_argument("--pyproject", default=str(REPO / "pyproject.toml"))
    ap.add_argument("--node", action="store_true", help="also run `<!-- spot: node -->` blocks (web-parity)")
    ap.add_argument("--pip-arg", action="append", default=[], help="extra args for every `pip install` (e.g. --find-links wheelhouse)")
    ns = ap.parse_args(argv)
    text = Path(ns.readme).read_text(encoding="utf-8")
    wheel = Path(ns.wheel) if ns.wheel else (installed_wheel_path(Path(ns.python)) if ns.run else None)
    declared = declared_extras_wheel(wheel) if wheel else declared_extras_pyproject(Path(ns.pyproject))
    problems = static_checks(text, declared)
    blocks, _ = extract_spots(text)
    if ns.run:
        rep = run_spots(blocks, python=Path(ns.python), wheel=wheel, node=ns.node, pip_args=ns.pip_arg)
        problems += rep.problems
        for x in rep.passed:
            print(f"  ok   {x}")
        for x in rep.deferred:
            print(f"  DEFER {x}")
    if problems:
        print(f"readme_spots: RED ({len(problems)}):", file=sys.stderr)
        for x in problems:
            print(f"  {x}", file=sys.stderr)
        return 1
    print(f"readme_spots: GREEN -- {len(blocks)} spot block(s), extras {sorted(declared)}, platform list == {sorted(EXPECTED_WHEEL_SET)}" + (" (run)" if ns.run else " (static)"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
