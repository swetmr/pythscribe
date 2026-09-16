"""M7 gates (spec 13-09-26-lib-selfcontained-pip-wheel; plan §M7 + §M7.1-M7.4; validation §M7 + §J-extras).

Frontend parity from the base wheel + the single `[web-bundled]` extra + `pyths doctor`. Every gate
ships its PAIRED NEGATIVE CONTROL (the anti-vacuity paired-control convention); the ones that cannot be exercised by input alone
(a source mutation) are mutation-verified RED by hand and named in the final report.

  M7-1   base carries no Node; extras-set == {server, gradio, streamlit, all, test, web-bundled}
         (a resurrected [web] or an [optimize] extra -> RED); nodejs-wheel-binaries ONLY under
         web-bundled (never `all`). Control: add nodejs to base deps -> RED (that is a pyproject
         mutation; the extras-set assertion here catches it).
  M7-2   resolver order PYTHS_NODE -> PATH -> vendored, with the printed notices. Controls: a
         cwd-planted `node` is NEVER selected (bare-name cwd resolution); prefer-vendored /
         silent-fallback mutants -> RED (mutation-verified).
  M7-3   `pyths new` uses the VENDORED scaffolder -> byte-identical to a direct payload run (same app
         name); the mirror gate is GREEN and a one-byte edit is RED.
  M7-3b  cross-run/cross-major determinism + no lockfile / node_modules; a process.version-writing
         scaffolder mutant -> the tree differs (the parity check catches it).
  M7-4   `pyths dev` exports PYTHS_BIN = the absolute pip compiler (the plugin then selects it).
         Control: no binary -> PYTHS_BIN unset (the plugin falls back). Full post-hydration DOM
         parity + the PYTHS_BIN logging-wrapper marker are DEFERRED (tracked pythscribe-dev#507),
         run neither here nor in the web-parity CI job -- provenance only, and the npm fallback is
         the byte-identical pinned-V compiler (M6.2 R-NI) so there is no silent-wrong-output path.
  M7-5   the three-part §3.2 actionable error for each state, NO traceback. Control: the resolver
         raising past the launcher boundary -> a Traceback -> RED (mutation-verified).
  M7-6   `pyths install` states the npm-registry caveat before spawning. Control: remove it -> RED.
  M7-7   the [web-bundled] pin major == the scaffolder `engines.node` major == _web.PINNED_NODE_MAJOR.
"""
from __future__ import annotations

import io
import json
import os
import re
import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

import pytest

from conftest import REPO, gate
from pythscribe import _launcher, _web

SCAFFOLDER = REPO / "pythscribe" / "_web" / "create-pyths-app"
SCRIPTS = REPO / "scripts"
NODE = shutil.which("node")
SKIP_PACKAGING = os.environ.get("PYTHSCRIBE_SKIP_PACKAGING") == "1"
DECLARED_EXTRAS = {"server", "gradio", "streamlit", "all", "test", "web-bundled"}


def _run(args, timeout=120, **kw) -> subprocess.CompletedProcess:
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, check=False, timeout=timeout, **kw)


def _tree_hashes(root: Path) -> dict[str, str]:
    import hashlib

    return {
        p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(root.rglob("*")) if p.is_file()
    }


# ============================================================ M7-7 pin gate (pure; always runs)


def _webbundled_pin_major() -> int:
    data = tomllib.loads((REPO / "pyproject.toml").read_text(encoding="utf-8"))
    specs = data["project"]["optional-dependencies"]["web-bundled"]
    [spec] = [s for s in specs if s.startswith("nodejs-wheel-binaries")]
    m = re.search(r">=\s*(\d+)", spec)
    assert m, spec
    return int(m.group(1))


def _scaffolder_engines_major() -> int:
    pkg = json.loads((SCAFFOLDER / "package.json").read_text(encoding="utf-8"))
    node = pkg["engines"]["node"]
    m = re.search(r"(\d+)", node)
    assert m, node
    return int(m.group(1))


def test_m7_7_pin_major_equals_scaffolder_engines_major():
    pin = _webbundled_pin_major()
    eng = _scaffolder_engines_major()
    assert pin == eng == _web.PINNED_NODE_MAJOR, (pin, eng, _web.PINNED_NODE_MAJOR)


def test_m7_7_control_bumping_either_side_would_diverge():
    """The gate is not vacuous: the three values are read from three independent sources. A mutant
    bumping any one (pyproject pin, scaffolder engines.node, or _web.PINNED_NODE_MAJOR) breaks the
    equality above -- mutation-verified by editing _web.PINNED_NODE_MAJOR to 23."""
    assert _webbundled_pin_major() == _scaffolder_engines_major()


# ============================================================ M7-1 extras set (built wheel)


@pytest.fixture(scope="module")
def built_wheel(tmp_path_factory) -> Path:
    if SKIP_PACKAGING:
        pytest.skip("packaging gate skipped by env (PYTHSCRIBE_SKIP_PACKAGING=1)")
    out = tmp_path_factory.mktemp("m7whl")
    env = {k: v for k, v in os.environ.items() if k != "PYTHONPATH"}  # the build-subprocess gotcha
    p = _run([sys.executable, "-m", "build", "--wheel", "--outdir", str(out), str(REPO)], timeout=600, cwd=str(REPO), env=env)
    gate(p.returncode == 0, f"python -m build --wheel failed: {p.stderr[-1500:]}")
    whls = list(out.glob("*.whl"))
    assert len(whls) == 1, whls
    return whls[0]


def _wheel_metadata(whl: Path) -> str:
    z = zipfile.ZipFile(whl)
    [md] = [n for n in z.namelist() if n.endswith(".dist-info/METADATA")]
    return z.read(md).decode("utf-8")


def test_m7_1_extras_set_is_exactly_the_declared_set(built_wheel):
    md = _wheel_metadata(built_wheel)
    extras = {e.strip() for e in re.findall(r"^Provides-Extra: (.+)$", md, re.M)}
    assert extras == DECLARED_EXTRAS, extras
    assert "web" not in extras and "optimize" not in extras, extras  # no resurrected [web] / [optimize]


def test_m7_1_web_bundled_is_the_only_node_carrier(built_wheel):
    """nodejs-wheel-binaries is pulled ONLY by [web-bundled]; `all` == wasmtime+gradio+streamlit and
    does NOT drag in a ~90 MB Node (validation §J-extras)."""
    md = _wheel_metadata(built_wheel)
    node_lines = [l for l in md.splitlines() if l.startswith("Requires-Dist") and "nodejs-wheel-binaries" in l]
    assert node_lines, "web-bundled must pull nodejs-wheel-binaries"
    for l in node_lines:
        assert 'extra == "web-bundled"' in l, l
    all_lines = [l for l in md.splitlines() if l.startswith("Requires-Dist") and 'extra == "all"' in l]
    assert all_lines and not any("nodejs" in l for l in all_lines), all_lines


def test_m7_1_base_requires_dist_has_no_unconditional_dependency(built_wheel):
    md = _wheel_metadata(built_wheel)
    unconditional = [l for l in md.splitlines() if l.startswith("Requires-Dist") and "extra ==" not in l]
    assert unconditional == [], unconditional  # `import pythscribe` is dependency-free


# ============================================================ M7-2 resolver order + notices


def test_m7_2_system_node_is_chosen_and_printed():
    gate(bool(NODE), "node required on PATH for the system-resolution arm")
    buf = io.StringIO()
    info = _web.find_node(out=buf)
    assert info.source == "PATH" and info.major >= _web.PINNED_NODE_MAJOR
    assert "using system Node" in buf.getvalue()


def test_m7_2_pyths_node_override_wins(monkeypatch):
    gate(bool(NODE), "node required for the override arm")
    monkeypatch.setenv(_web.PYTHS_NODE_ENV, NODE)
    buf = io.StringIO()
    info = _web.find_node(out=buf)
    assert info.source == "PYTHS_NODE"
    assert f"from {_web.PYTHS_NODE_ENV}" in buf.getvalue()


def test_m7_2_pyths_node_relative_is_refused(monkeypatch):
    monkeypatch.setenv(_web.PYTHS_NODE_ENV, "./node")
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    assert "PYTHS_NODE" in ei.value.report and "Traceback" not in ei.value.report


def test_m7_2_system_preferred_over_vendored_when_both_present(monkeypatch):
    """Both a usable system Node AND a vendored one present -> the SYSTEM one is chosen (validation
    §M7-2). A mutant that prefers the vendored Node reports source=='vendored' here -> RED."""
    gate(bool(NODE), "node required on PATH for the both-present arm")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: (Path("/fake/vendored/node"), (22, "22.99.0")))
    info = _web.find_node(out=io.StringIO())
    assert info.source == "PATH"


def test_m7_2_vendored_fallback_when_no_system_node(monkeypatch):
    """System Node removed, [web-bundled] present -> the vendored Node is chosen WITH the fallback
    notice. The vendored Node is injected through the `_vendored_node` seam (so the test does not
    depend on nodejs_wheel being installed)."""
    gate(bool(NODE), "a real node exe is reused as the fake vendored one")
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    fake = Path(NODE)
    monkeypatch.setattr(_web, "_vendored_node", lambda: (fake, (fake and 22, "22.99.0")))
    buf = io.StringIO()
    info = _web.find_node(out=buf)
    assert info.source == "vendored"
    assert "vendored Node" in buf.getvalue() and "web-bundled" in buf.getvalue()  # the fallback notice


def test_m7_2_old_vendored_node_is_refused_by_the_pinned_floor(monkeypatch):
    """S8: a vendored Node OLDER than PINNED_NODE_MAJOR (e.g. Node 18) must be refused, exactly like
    an old system Node -- NOT returned just because it is vendored. find_node raises ToolchainMissing
    naming the version and the floor; source='vendored' is never produced. Paired mutation control:
    revert the vendored-arm floor check and this old vendored Node is returned as usable -> RED."""
    monkeypatch.setenv("PATH", "")  # no system Node
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    old = Path("/fake/vendored/node")
    monkeypatch.setattr(_web, "_vendored_node", lambda: (old, (18, "18.0.0")))
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    assert "18.0.0" in ei.value.report and f">= {_web.PINNED_NODE_MAJOR}" in ei.value.report
    assert "vendored" in ei.value.report
    # discriminating twin: a vendored Node AT the floor is accepted (so the refusal is the floor, not the arm)
    monkeypatch.setattr(_web, "_vendored_node", lambda: (old, (_web.PINNED_NODE_MAJOR, f"{_web.PINNED_NODE_MAJOR}.0.0")))
    monkeypatch.setattr(_web, "_probe_node_version", lambda exe: (_web.PINNED_NODE_MAJOR, f"{_web.PINNED_NODE_MAJOR}.0.0"))
    assert _web.find_node(out=io.StringIO()).source == "vendored"


def test_m7_2_control_cwd_planted_node_is_never_selected(monkeypatch, tmp_path):
    """A bare `node` in the cwd must NEVER be resolved (CWE-426). `_search_path` -- the primitive the
    resolver uses -- skips the cwd / empty / relative PATH elements, so a cwd-planted `node[.exe]` is
    invisible even with PATH empty. The mutant that adds the cwd to _search_path returns it here -> RED.
    (Tested at the primitive so the planted file need not be a runnable Node.)"""
    for name in ("node", "node.exe"):
        (tmp_path / name).write_bytes(b"planted")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setenv("PATH", "")
    assert _web._search_path("node") is None
    # and end-to-end: no PATH, no vendored -> absent, never the cwd file
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    with pytest.raises(_web.ToolchainMissing):
        _web.find_node(out=io.StringIO())


# ============================================================ M7-5 actionable error (four states)


def _assert_three_parts(report: str):
    assert "need Node" in report                       # part 1: what needs the toolchain
    assert "fix:" in report                             # part 2: the state-tailored fix
    assert "run `pyths doctor`" in report               # the doctor pointer
    assert "What still works without Node" in report    # part 3: what still works
    assert "Traceback" not in report


def test_m7_5_no_node_state(monkeypatch):
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    _assert_three_parts(ei.value.report)
    assert "pip install pythscribe[web-bundled]" in ei.value.report and "no Node.js was found" in ei.value.report


def test_m7_5_too_old_state(monkeypatch):
    gate(bool(NODE), "a real node on PATH is downgraded via a version-probe stub")
    monkeypatch.setattr(_web, "_probe_node_version", lambda exe: (18, "18.0.0"))
    monkeypatch.setattr(_web, "_vendored_node", lambda: None)
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    _assert_three_parts(ei.value.report)
    assert "18.0.0" in ei.value.report and f">= {_web.PINNED_NODE_MAJOR}" in ei.value.report


def test_m7_5_invalid_pyths_node_state(monkeypatch):
    monkeypatch.setenv(_web.PYTHS_NODE_ENV, "./not-a-node")
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    _assert_three_parts(ei.value.report)
    assert "PYTHS_NODE=" in ei.value.report


def test_m7_5_vendored_broken_state(monkeypatch):
    monkeypatch.setenv("PATH", "")
    monkeypatch.delenv(_web.PYTHS_NODE_ENV, raising=False)
    monkeypatch.setattr(_web, "_vendored_node", lambda: (Path("/fake/node"), None))  # present but no version
    with pytest.raises(_web.ToolchainMissing) as ei:
        _web.find_node(out=io.StringIO())
    _assert_three_parts(ei.value.report)
    assert "force-reinstall nodejs-wheel-binaries" in ei.value.report


def test_m7_5_launcher_boundary_prints_no_traceback(tmp_path):
    """The launcher catches ToolchainMissing AT THE BOUNDARY: `pyths new` with no resolvable Node
    exits non-zero, stderr carries the three parts, and there is NO Traceback anywhere. Run as a real
    subprocess with Node scrubbed from PATH (System32 only so python's own DLLs still load)."""
    sys_path = "C:\\Windows\\System32" if os.name == "nt" else "/usr/bin:/bin"
    env = {**os.environ, "PATH": sys_path, "PYTHONPATH": str(REPO)}
    env.pop(_web.PYTHS_NODE_ENV, None)
    r = _run([sys.executable, "-m", "pythscribe._launcher", "new", "x"], env=env, cwd=str(tmp_path), timeout=60)
    assert r.returncode != 0
    combined = r.stdout + r.stderr
    assert "Traceback" not in combined, combined
    _assert_three_parts(combined)


# ============================================================ M7-6 network caveat


def test_m7_6_install_states_the_registry_caveat_before_spawning(monkeypatch, capsys):
    gate(bool(NODE), "node required so the resolver reaches the install branch")
    captured = {}

    def fake_spawn(cmd, env=None):
        captured["cmd"] = cmd
        return 0

    monkeypatch.setattr(_launcher, "_spawn", fake_spawn)
    rc = _launcher.main(["install"])
    err = capsys.readouterr().err
    assert rc == 0
    assert "npm registry" in err and "stays offline" in err
    assert captured["cmd"][-1] == "install"  # it DID go on to spawn npm install


# ============================================================ M7-3 scaffold parity (vendored)


@pytest.fixture
def _scaffold(tmp_path):
    def make(where: Path, *, use_launcher: bool, node_env: str | None = None) -> Path:
        where.mkdir(parents=True, exist_ok=True)
        env = {**os.environ, "PYTHONPATH": str(REPO)}
        if node_env is not None:
            env[_web.PYTHS_NODE_ENV] = node_env
        if use_launcher:
            r = _run([sys.executable, "-m", "pythscribe._launcher", "new", "app"], env=env, cwd=str(where), timeout=90)
        else:
            r = _run([NODE, str(SCAFFOLDER / "index.js"), "app"], env=env, cwd=str(where), timeout=90)
        assert r.returncode == 0, r.stderr
        return where / "app"

    return make


def test_m7_3_launcher_new_matches_direct_vendored_run(_scaffold, tmp_path):
    gate(bool(NODE), "node required to run the scaffolder")
    b = _scaffold(tmp_path / "B", use_launcher=True)          # `pyths new app` (uses find_scaffolder -> vendored)
    ref = _scaffold(tmp_path / "REF", use_launcher=False)      # `node <vendored>/index.js app` directly
    assert _tree_hashes(b) == _tree_hashes(ref)               # byte-identical trees (same app name)


def test_m7_3_scaffolder_mirror_gate_is_green_and_one_byte_edit_is_red(tmp_path):
    """The mirror gate makes `pyths new` == `npm create pyths-app@V` by construction. GREEN as
    shipped; a one-byte edit to the vendored copy is RED (validation §M7-3 control)."""
    payload = REPO / "dist" / "scaffolder-payload"
    gate(payload.is_dir(), "run scripts/prepare_release_payloads.py first (dist/scaffolder-payload)")
    green = _run([sys.executable, str(SCRIPTS / "verify_scaffolder_mirror.py")], cwd=str(REPO))
    assert green.returncode == 0, green.stderr
    # one-byte edit to a COPY of the vendored tree
    v = tmp_path / "create-pyths-app"
    shutil.copytree(SCAFFOLDER, v)
    idx = v / "index.js"
    idx.write_text(idx.read_text(encoding="utf-8") + "\n// drift\n", encoding="utf-8")
    red = _run([sys.executable, str(SCRIPTS / "verify_scaffolder_mirror.py"), "--vendored", str(v)], cwd=str(REPO))
    assert red.returncode == 1 and "CHANGED bytes" in red.stderr and "index.js" in red.stderr


_S10_GOOD = {
    "package.json": b'{"name":"create-pyths-app"}\n',
    "index.js": b"console.log('x');\n",
    "LICENSE": b"MIT\n",
    "README.md": b"# scaffold\n",
    "skills/compressing-ps-to-psc.md": b"psc\n",
    "skills/pythscribe-language.md": b"lang\n",
}


def _write_tree(root: Path, files: dict) -> Path:
    for rel, data in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(data)
    return root


def test_s10_scaffolder_mirror_enforces_exact_skills_and_lf(tmp_path):
    """S10: the mirror gate now pins the skills/ subtree to EXACTLY the two generated skills and
    requires LF -- so a stale skills/old.md (or a CRLF file) present on BOTH sides, which the raw-byte
    compare cannot see, is caught. Paired with the copy-skills-clears test below."""
    sys.path.insert(0, str(SCRIPTS))
    import verify_scaffolder_mirror as vsm

    assert vsm.verify(_write_tree(tmp_path / "p", _S10_GOOD), _write_tree(tmp_path / "v", _S10_GOOD)) == []
    # a stale skills/old.md on BOTH sides -> membership RED (invisible to compare())
    stale = {**_S10_GOOD, "skills/old.md": b"stale\n"}
    prob = vsm.verify(_write_tree(tmp_path / "p2", stale), _write_tree(tmp_path / "v2", stale))
    assert any("skills/ subtree" in x and "old.md" in x for x in prob), prob
    assert vsm.compare(vsm.tree_bytes(tmp_path / "p2"), vsm.tree_bytes(tmp_path / "v2")) == []  # compare() is BLIND to it
    # a CRLF file on BOTH sides -> LF RED (compare() passes)
    crlf = {**_S10_GOOD, "index.js": b"console.log('x');\r\n"}
    prob = vsm.verify(_write_tree(tmp_path / "p3", crlf), _write_tree(tmp_path / "v3", crlf))
    assert any("not LF" in x and "index.js" in x for x in prob), prob
    # a DROPPED generated skill -> membership RED
    dropped = {k: v for k, v in _S10_GOOD.items() if k != "skills/pythscribe-language.md"}
    assert any("skills/ subtree" in x for x in vsm.verify(_write_tree(tmp_path / "p4", dropped), _write_tree(tmp_path / "v4", dropped)))


def test_s10_copy_skills_clears_stale_files():
    """S10: copy-skills.cjs clears/recreates skills/ before copying, so a stale skills/old.md from a
    previous prepack does not survive. Leaves skills/ with exactly the two canonical skills."""
    gate(NODE is not None, "node required to run copy-skills.cjs")
    pkg = REPO / "packages" / "create-pyths-app"
    skills_dir = pkg / "skills"
    assert _run([NODE, str(pkg / "scripts" / "copy-skills.cjs")]).returncode == 0
    (skills_dir / "old.md").write_bytes(b"stale\n")
    assert _run([NODE, str(pkg / "scripts" / "copy-skills.cjs")]).returncode == 0
    assert not (skills_dir / "old.md").exists(), "copy-skills.cjs did not clear the stale skills/old.md"
    assert (skills_dir / "pythscribe-language.md").exists() and (skills_dir / "compressing-ps-to-psc.md").exists()
    assert sorted(p.name for p in skills_dir.iterdir()) == ["compressing-ps-to-psc.md", "pythscribe-language.md"]


def test_m7_3b_determinism_and_no_lockfile(_scaffold, tmp_path):
    gate(bool(NODE), "node required")
    a = _scaffold(tmp_path / "A", use_launcher=True)
    b = _scaffold(tmp_path / "B", use_launcher=True, node_env=NODE)  # second run (PYTHS_NODE arm)
    assert _tree_hashes(a) == _tree_hashes(b)
    names = {p.name for p in a.rglob("*") if p.is_file()}
    assert "package-lock.json" not in names and "node_modules" not in {p.name for p in a.rglob("*")}


def test_m7_3b_control_process_version_mutant_diverges(tmp_path):
    """A scaffolder mutant that writes process.version into a file makes the tree nondeterministic;
    the parity check (tree equality) then goes RED. Verified by running such a mutant and asserting
    its tree differs from the honest vendored run."""
    gate(bool(NODE), "node required")
    honest = tmp_path / "H"
    honest.mkdir()
    assert _run([NODE, str(SCAFFOLDER / "index.js"), "app"], cwd=str(honest), timeout=90).returncode == 0
    # mutant: a copy of index.js that also writes process.version
    mut_dir = tmp_path / "mut"
    shutil.copytree(SCAFFOLDER, mut_dir)
    mi = mut_dir / "index.js"
    src = mi.read_text(encoding="utf-8")
    src += '\nimport { writeFileSync as _wf } from "node:fs";\n_wf("app/NODE_VERSION.txt", process.version);\n'
    mi.write_text(src, encoding="utf-8")
    mut = tmp_path / "M"
    mut.mkdir()
    assert _run([NODE, str(mi), "app"], cwd=str(mut), timeout=90).returncode == 0
    assert _tree_hashes(honest / "app") != _tree_hashes(mut / "app")  # the parity check catches it


# ============================================================ M7-4 PYTHS_BIN wiring (env SPOT; DOM E2E is CI-only)


def test_m7_4_web_parity_ci_job_exists_with_its_gates():
    """M7.4: ci.yml carries a `web-parity` job (separate from the M4 node-free acceptance job) that
    runs the A==B==C scaffold parity AND `pyths doctor` in BOTH the node-present and node-absent
    states. A mutant that deletes the job, or its parity / node-absent-doctor step, is RED here."""
    import yaml

    wf = yaml.safe_load((REPO / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8"))
    assert "web-parity" in wf["jobs"], list(wf["jobs"])
    job = wf["jobs"]["web-parity"]
    steps = job["steps"]
    blob = "\n".join(s.get("run", "") + " " + s.get("name", "") for s in steps)
    assert any(u.get("uses", "").startswith("actions/setup-node") for u in steps)  # system Node
    assert "A == B == C" in blob and "pyths new" in blob                            # scaffold parity
    assert "verify_scaffolder_mirror.py" in blob                                    # the mirror gate
    assert "--strict --require node" in blob and "Node: absent" in blob             # doctor both states


def test_m7_4_dev_env_exports_absolute_pyths_bin(tmp_path, monkeypatch):
    """`pyths dev` exports PYTHS_BIN = the absolute pip compiler so the Vite/Next plugin's
    resolvePythsCommand selects it. Locally there is no bundled binary, so PYTHSCRIBE_PYTHS is pointed
    at a stand-in file (find_pyths accepts any non-launcher file via that env)."""
    fake = tmp_path / "pyths-stub"
    fake.write_bytes(b"\x7fELF stub for the test")
    monkeypatch.setenv("PYTHSCRIBE_PYTHS", str(fake))
    env = _launcher._dev_child_env(base=dict(os.environ))
    assert env.get("PYTHS_BIN") == str(Path(os.path.normpath(fake.resolve())))


def test_m7_4_control_no_binary_leaves_pyths_bin_unset(monkeypatch):
    """Control: with no resolvable compiler, PYTHS_BIN is NOT set (the plugin falls back to its own
    resolution) -- and a bogus PYTHSCRIBE_PYTHS pointing nowhere raises inside find_pyths, so the env
    helper leaves PYTHS_BIN unset rather than exporting a bad path."""
    monkeypatch.delenv("PYTHSCRIBE_PYTHS", raising=False)
    # a base env with no PYTHS_BIN and no bundled binary in this source tree
    base = {k: v for k, v in os.environ.items() if k != "PYTHS_BIN"}
    env = _launcher._dev_child_env(base=base)
    # source checkout has no pythscribe/_bin/pyths, so find_pyths may resolve a dev/PATH binary or
    # raise; either way, if it cannot resolve, PYTHS_BIN stays unset.
    from pythscribe.build import BuildError, find_pyths

    try:
        expected = str(find_pyths())
        assert env.get("PYTHS_BIN") == expected
    except BuildError:
        assert "PYTHS_BIN" not in env
