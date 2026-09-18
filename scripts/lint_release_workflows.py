#!/usr/bin/env python3
"""Workflow-lint for the release evidence graph (spec 13-09-26, plan M6.1 job graph + M4 native window; validation §E0/§A).

    python scripts/lint_release_workflows.py [--release .github/workflows/release.yml] [--publish .github/workflows/publish-pypi.yml]

Invariants (each RED names the violation; tests/pythscribe/test_wheel_m4_acceptance.py runs the same `lint()` on
mutated copies to prove every rule fires):
  G1  `manifest.needs` excludes `node-free-acceptance`; `node-free-acceptance.needs` includes `manifest` (no cycle,
      and A0b never runs without a manifest); `manifest.needs` covers `prepare` and every distribution producer
      (`wheel-set` -> `wheel` -> `build`) transitively; the whole `needs` graph is acyclic; `manifest` has no `if:`.
  G2  REQUIRED_PREREQ_JOBS (require_evidence.py) == the display names of {prepare, Build x <matrix targets>, manifest}
      in release.yml -- the constant can never be stale against a rename.
  R1  every release.yml step invoking require_evidence.py passes the literal `--role release-run`; every
      publish-pypi.yml step passes `--role promotion` AND has no `if:` (no input can bypass it) AND is reached
      before any build/publish step of its job; no workflow exposes a `role` / `mode` input.
  W1  node-free-acceptance native window: between the first step whose id starts with `scrub` and the last step
      whose id starts with `restore`, NO step has `uses:` (every step is a `run:` shell step).
  W2  a `restore` step exists, and at least one `uses:` step FOLLOWS it (the leg upload -- the fail-safe that
      breaks loudly on a forgotten restore); the first post-scrub `uses:` step comes AFTER restore.
  M6 (plan M6.1-M6.5):
  M1  `manifest` runs scripts/assemble_manifest.py (the real producer; a fail-closed `exit 1` placeholder is RED)
      and uploads the `release-manifest` artifact; `prepare` runs `publish.mjs --check` (stamp idempotence).
  M2  `npm-identity` exists, needs `npm-publish` + `manifest`, runs `require_evidence.py --role release-run`
      BEFORE scripts/verify_npm_identity.py; `npm-publish` runs the evidence step BEFORE `publish.mjs`.
  M3  `release` (the GitHub Release of binaries) needs `manifest` (binaries never reach the Release without one).
  M4  the wheel legs' CIBW_TEST_COMMAND runs scripts/readme_spots.py --run (validation §K).
  P1  publish-pypi.yml: `testpypi` gates with `--need R-BA R-NI`, `pypi` with `--need R-BA R-NI R-TP R-TV`;
      NO `python -m build` anywhere (no rebuild); both publication jobs run verify_dist_manifest.py before the
      publish action (pypi with `--rtp`); a `testpypi-validate` matrix covering every TRIPLE_TO_TAG target needs
      `testpypi`; a job runs assemble_rtv.py after it; `gh run download` / `pip install` / publish never precede
      the evidence gate.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from require_evidence import RECORD_PRODUCER_JOB, RECORD_PRODUCER_WORKFLOW, REQUIRED_PREREQ_JOBS  # noqa: E402

ACCEPTANCE, MANIFEST, PREPARE = "node-free-acceptance", "manifest", "prepare"
DIST_PRODUCERS = ("wheel-set", "wheel", "build")
ROLE_RE = re.compile(r"require_evidence\.py(?P<args>[^\n|&;]*)")
NEED_RE = re.compile(r"--need((?:\s+R-[A-Za-z]+)+)")
# S9/S11 (codex pass-3): shell control operators that let a SECOND (unchecked) invocation mask the
# first's failure (`gate || gate2`, `gate; gate2`, `gate && x`, `gate | x`). A gate command must be a
# single simple invocation with none of these and no comment hiding a flag.
SHELL_OPS = ("||", "&&", ";", "|", "&", "`", "$(")


def _mask_quoted(s: str) -> str:
    """Replace the CONTENTS of single/double-quoted spans with 'X' (quotes kept) so a later scan for
    shell operators / comment markers never trips on characters INSIDE a quoted string (e.g. a value
    like "${GITHUB_REF_NAME}" or a message). Unbalanced quotes mask to end-of-string."""
    out: list[str] = []
    q: str | None = None
    for ch in s:
        if q is not None:
            out.append(ch if ch == q else "X")
            if ch == q:
                q = None
        elif ch in ("'", '"'):
            q = ch
            out.append(ch)
        else:
            out.append(ch)
    return "".join(out)


def _strip_comments(run_text: str) -> tuple[str, bool]:
    """Remove an unquoted `#...` comment from each line (a comment could hide a flag from a substring
    lint while the shell ignores it). Returns (comment-free text, had_comment)."""
    lines, had = [], False
    for line in run_text.splitlines():
        masked = _mask_quoted(line)
        idx = masked.find("#")
        if idx != -1:
            had = True
            line = line[:idx]
        lines.append(line)
    return "\n".join(lines), had


def _gate_command_problems(run_text: str, invocation: str, label: str, key: str) -> list[str]:
    """S9/S11: a gate STEP must be EXACTLY ONE simple `invocation` (e.g. require_evidence.py /
    verify_npm_identity.py) with NO shell control operator and NO comment. This is what stops a
    same-step bypass (`gate --need ... || gate` -- the second, no-record call masks the first's
    failure) and a comment hiding/altering a flag -- neither of which a substring test catches."""
    p: list[str] = []
    stripped, had_comment = _strip_comments(run_text)
    masked = _mask_quoted(stripped)
    n = stripped.count(invocation)
    if n != 1:
        p.append(f"{label}: `{key}` gate step has {n} {invocation} invocations -- EXACTLY ONE is required "
                 f"(a second, no-record invocation via a shell operator masks the first's failure)")
    ops = [op for op in SHELL_OPS if op in masked]
    if ops:
        p.append(f"{label}: `{key}` gate step uses shell control/substitution {ops} -- a gate command must be a "
                 f"single simple invocation (a `|| {invocation}` bypass runs a second, unchecked gate)")
    if had_comment:
        p.append(f"{label}: `{key}` gate step contains a `#` comment -- a comment can hide/alter a flag "
                 f"(e.g. `# --checkout .`) while the shell ignores it; remove it")
    return p

# S11: the EXACT `--need` record set every promotion gate must carry, per job. Compared as SETS
# (not substrings) so adding R-TV to `testpypi` -- a future deadlock, since R-TV's producer is
# `testpypi-validate` which `needs: testpypi` -- is caught. The two intermediate jobs run in the SAME
# run that produces R-TP/R-TV, so requiring either self-references an in-progress run (opus M6 B1).
# ── node-free ACCEPTANCE MODE: the single source of truth (flip THIS one line to switch) ────────────────
# "advisory" (default from v0.2.5): node-free-acceptance RUNS but does NOT gate npm/PyPI -- it is
#   continue-on-error and R-BA is advisory (attached to the Release, not required). Use this whenever the
#   M4 acceptance harness is broken/flaky (GitHub runner-image drift is unbounded) OR you need to push a
#   release quickly: a red acceptance run must NEVER block shipping. This is intentionally the sticky
#   default so future releases are not blocked by acceptance without a deliberate decision.
# "blocking": the full R-BA gate -- npm-publish needs node-free-evidence + consumes/validates R-BA, and the
#   promotion gates require R-BA. Flip here ONLY when M4 acceptance is reliable enough to gate an
#   irreversible publish.
# EITHER WAY the lint below ENFORCES the workflow matches this mode (acceptance_mode_problems + the
# EXPECTED_NEED sets), so the chosen mode -- and the continue-on-error/always() that make "advisory" safe
# -- cannot silently drift (invariants-as-failing-gates). Changing the mode is one line here, never surgery.
ACCEPTANCE_MODE = "advisory"
assert ACCEPTANCE_MODE in ("advisory", "blocking")
ADVISORY = ACCEPTANCE_MODE == "advisory"
_RBA: list[str] = [] if ADVISORY else ["R-BA"]

# S11: the EXACT `--need` record set every promotion gate must carry, per job -- R-BA included ONLY in
# blocking mode. Compared as SETS so an extra/missing record is caught.
EXPECTED_NEED: dict[str, set[str]] = {
    "testpypi": {*_RBA, "R-NI"},
    "pypi": {*_RBA, "R-NI", "R-TP", "R-TV"},
    "testpypi-validate": {*_RBA, "R-NI"},
    "testpypi-evidence": {*_RBA, "R-NI"},
}

# S9 + S11 ROOT FIX (codex pass-4): an operator/comment BLACKLIST never converges -- codex kept finding a
# new shell trick that left a gate step lint-green while masking failure (`if python ...; then :; fi`, a
# newline + `true`, `--checkout ""`, multiline, reserved words, negation). So each promotion/identity gate
# step is PINNED to its EXACT normalized single-line command: ANY deviation (any of the above, plus ||/&&/
# ;/|, a comment, an extra invocation) fails by construction, because it cannot equal the pinned string.
# The ONLY normalization is the python interpreter token (`python` / `python3` both fine); everything else
# -- flags, order, quoting, the `--tag "${GITHUB_REF_NAME}"` literal -- is matched byte-for-byte.
# The exact `--need` ORDER each promotion gate must carry (order matters for the exact-string pin).
EXPECTED_NEED_ORDER: dict[str, list[str]] = {
    "testpypi": [*_RBA, "R-NI"],
    "testpypi-validate": [*_RBA, "R-NI"],
    "testpypi-evidence": [*_RBA, "R-NI"],
    "pypi": [*_RBA, "R-NI", "R-TP", "R-TV"],
}
_PY = re.compile(r"^python3?(?=\s)")


def _pinned_promotion_command(key: str) -> str:
    need = " ".join(EXPECTED_NEED_ORDER[key])
    return ('python scripts/require_evidence.py --role promotion --manifest release_manifest.json '
            f'--need {need} --evidence-dir evidence --tag "${{GITHUB_REF_NAME}}"')


NPM_IDENTITY_PIN = ('python scripts/verify_npm_identity.py --manifest release_manifest.json '
                    '--dist dist --out evidence/R-NI.json --checkout .')

# The EXACT post-publish PyPI set-proof command (codex astra 2026-09-18). The `pypi` publish is resumable
# (skip-existing:true), so this proof -- assemble_rtp against PRODUCTION PyPI, asserting the registry serves
# exactly the manifest's distribution set + digests -- is the only catch for a partial/wrong production
# upload. Pinning it EXACTLY (like the promotion gates) is what stops the vacuous-proof bypasses a substring
# check misses: `echo`-prefix, `|| true`, a mispointed --repository-url, or a hidden comment all fail here.
PYPI_PROOF_PIN = ('python scripts/assemble_rtp.py --manifest release_manifest.json --dist dist '
                  '--repository-url https://upload.pypi.org/legacy/ --out evidence/PyPI-registry-proof.json')

# The EXACT R-TP command in the `testpypi` job (fable pass C1): the SAME assembler is pinned on the pypi
# path but was unlinted on the testpypi path, so `|| true` on it or its removal linted GREEN. Pin it too --
# both fail safe downstream, but a lying assembler on one path deserves the same exact-command guard.
TESTPYPI_PROOF_PIN = ('python scripts/assemble_rtp.py --manifest release_manifest.json --dist dist '
                      '--repository-url https://test.pypi.org/legacy/ --out evidence/R-TP.json')


def _normalize_gate_run(run_text: str) -> str:
    """Normalize ONLY the leading python interpreter token; everything else is compared exactly."""
    return _PY.sub("python", str(run_text).strip())


# Fix 1 (codex pass-5) -- STEP-METADATA failure-masking bypass. The exact-command pin above validates
# only the `run:` STRING; it says nothing about the STEP's other keys. GitHub lets a step carry
# `continue-on-error: true` (the job proceeds after the step FAILS), an `if:` (the gate is skipped, which
# does not fail the job), `timeout-minutes`, a non-standard `background`, or a CUSTOM `shell:` such as
# `bash -c '... || true'` -- each leaves the exact-`run:` pin GREEN while defeating the gate. ROOT FIX
# (close the CLASS, not the three keys codex named): pin the COMPLETE step execution contract with an
# ALLOWLIST of permitted step keys. ANY key not on the allowlist -- continue-on-error, if, background,
# timeout-minutes, or any future metadata trick -- is RED by construction, so there is no 4th key to find;
# and a `shell:`, if present, must be the project's standard shell (a custom wrapper can `|| true` a
# failed gate). Applied to every promotion require_evidence.py gate step AND the npm-identity
# verify_npm_identity.py step -- the exact-pinned gate steps.
GATE_STEP_ALLOWED_KEYS: frozenset[str] = frozenset({"name", "id", "run", "shell", "env", "working-directory"})
GATE_STEP_APPROVED_SHELLS: frozenset[str] = frozenset({"bash"})  # the standard shell of the Linux gate jobs


def _gate_step_metadata_problems(step: dict, label: str, key: str) -> list[str]:
    """Fix 1: a gate STEP may carry ONLY execution-neutral keys (the allowlist). Any other key can mask a
    failed gate or skip it (`continue-on-error`/`if`/`background`/`timeout-minutes`) -> RED. A `shell:`
    must be the project's standard shell -- a custom `shell:` (`bash -c '... || true'`) masks failure."""
    p: list[str] = []
    extra = sorted(k for k in step if k not in GATE_STEP_ALLOWED_KEYS)
    if extra:
        p.append(f"{label}: `{key}` gate step carries disallowed step key(s) {extra} -- a gate step may use "
                 f"ONLY {sorted(GATE_STEP_ALLOWED_KEYS)}. `continue-on-error` lets the job proceed after the "
                 f"gate FAILS, `if` SKIPS the gate, `background` detaches it, `timeout-minutes`/any other "
                 f"metadata can mask a failed gate -- failure-masking step metadata is refused by construction")
    if "shell" in step and str(step.get("shell")) not in GATE_STEP_APPROVED_SHELLS:
        p.append(f"{label}: `{key}` gate step declares `shell: {step.get('shell')!r}` -- only the project's "
                 f"standard shell {sorted(GATE_STEP_APPROVED_SHELLS)} is permitted on a gate step (a custom "
                 f"`shell:` such as `bash -c '... || true'` masks a failed gate command)")
    return p


def _exact_pin_problems(run_text: str, expected: str, label: str, key: str) -> list[str]:
    """S9/S11 root fix: the gate step's `run:` must EQUAL the pinned command after interpreter
    normalization. Any shell composition -- if/then/fi, ||, &&, ;, |, a comment, an empty-string arg
    (`--checkout ""`), a multiline command, a shell reserved word, negation, or an extra invocation --
    makes the string differ, so it fails HERE by construction (no operator blacklist to out-run)."""
    got = _normalize_gate_run(run_text)
    if got != expected:
        return [f"{label}: `{key}` gate step is not the EXACT pinned command (root fix vs the shell-composition "
                f"bypass class). ANY deviation -- if/then/fi, ||/&&/;/|, a comment, an empty-string arg "
                f'(--checkout ""), a multiline command, a shell reserved word, negation, or an extra invocation '
                f"-- fails by construction.\n      expected: {expected}\n      got:      {got!r}"]
    return []


def parse_need(text: str) -> set[str] | None:
    """The SET of records after a `--need` flag in a require_evidence.py invocation (tokens are
    consumed until the next non-record token), or None when there is no `--need`."""
    m = NEED_RE.search(text)
    if not m:
        return None
    return set(m.group(1).split())


def _evidence_steps(job: dict) -> list[dict]:
    return [s for s in _steps(job) if "require_evidence.py" in _run_text(s)]


def need_problems(job: dict, key: str, expected: set[str], label: str) -> list[str]:
    """S11: a promotion job must carry EXACTLY ONE require_evidence.py invocation, whose `--need`
    appears EXACTLY ONCE and equals `expected` as a SET. A second (unchecked) evidence step, or a
    repeated `--need` (argparse uses the LAST occurrence, so `--need R-BA R-NI --need` requires NO
    records) is caught here -- the codex S11 bypasses."""
    p: list[str] = []
    ev = _evidence_steps(job)
    if not ev:
        p.append(f"{label}: publish-pypi.yml `{key}` has no require_evidence.py gate")
        return p
    if len(ev) > 1:
        p.append(f"S11: publish-pypi.yml `{key}` has {len(ev)} require_evidence.py steps -- EXACTLY ONE evidence invocation per promotion job is required (a second, unchecked `--need` step could require no records)")
    # S11 (codex pass-3): the gate STEP itself must be a single simple require_evidence invocation --
    # `require_evidence --need ... || require_evidence` (a second, no-record call) is one step with one
    # `--need`, so the old checks passed while the shell ran an unchecked second gate. Reject operators.
    rt = str(ev[0].get("run", ""))
    p += _gate_command_problems(rt, "require_evidence.py", "S11", key)
    # Fix 1 (codex pass-5): the gate STEP's non-`run:` metadata must not mask/skip the gate
    # (continue-on-error / if / background / custom shell) -- checked on the single evidence step.
    p += _gate_step_metadata_problems(ev[0], label, key)
    # S11 ROOT FIX (codex pass-4): the gate step must EQUAL the exact pinned command -- this closes the
    # whole shell-composition-bypass CLASS (if/then/fi, trailing `true`, multiline, reserved words,
    # negation), not just the operators the blacklist above enumerates.
    if key in EXPECTED_NEED_ORDER:
        p += _exact_pin_problems(rt, _pinned_promotion_command(key), "S11", key)
    if len(re.findall(r"--need(?:\s|$)", rt)) > 1:
        p.append(f"S11: publish-pypi.yml `{key}` gate repeats `--need` (argparse uses the LAST occurrence -- `--need ... --need` requires NO records); use a single `--need {' '.join(sorted(expected))}`")
        return p
    got = parse_need(rt)
    if got != expected:
        p.append(f"{label}: publish-pypi.yml `{key}` gate must be exactly `--need {' '.join(sorted(expected))}` (got {sorted(got) if got else got})")
    return p


def _needs(job: dict) -> list[str]:
    n = job.get("needs") or []
    return [n] if isinstance(n, str) else list(n)


# ---- PERM (codex pass-4): every job that runs require_evidence.py / require_ci_success.py queries the
# Actions API (runs/jobs + the immutable artifact ZIPs), so it needs EFFECTIVE `actions: read` -- else an
# honest release/promotion 403s. "Effective" = the job's own `permissions:` block if it declares one (a job
# block fully REPLACES the workflow default), otherwise the workflow-level `permissions:`.
_EVIDENCE_API_SCRIPTS = ("require_evidence.py", "require_ci_success.py")


def _effective_permissions(job: dict, wf: dict):
    return job["permissions"] if isinstance(job, dict) and "permissions" in job else wf.get("permissions")


def _grants_actions_read(perms) -> bool:
    if isinstance(perms, str):
        return perms in ("read-all", "write-all")  # the string shorthands grant actions
    if isinstance(perms, dict):
        return perms.get("actions") in ("read", "write")
    return False  # None / absent / anything else -> fail closed (not granted)


def actions_read_problems(wf: dict, label: str) -> list[str]:
    p: list[str] = []
    for key, job in (wf.get("jobs") or {}).items():
        if not isinstance(job, dict):
            continue
        if any(any(s in str(step.get("run", "")) for s in _EVIDENCE_API_SCRIPTS) for step in _steps(job)):
            if not _grants_actions_read(_effective_permissions(job, wf)):
                p.append(f"PERM: {label} job `{key}` runs {'/'.join(_EVIDENCE_API_SCRIPTS)} (queries the Actions API) "
                         f"but lacks EFFECTIVE `actions: read` -- an honest release/promotion gets 403; grant "
                         f"`actions: read` at the workflow level or in this job's own permissions block")
    return p


def _on(wf: dict) -> dict:
    return wf.get("on") or wf.get(True) or {}  # PyYAML reads the bare `on:` key as boolean True


def _steps(job: dict) -> list[dict]:
    return [s for s in (job.get("steps") or []) if isinstance(s, dict)]


def _matrix_names(job: dict) -> list[str]:
    name = job.get("name")
    inc = ((job.get("strategy") or {}).get("matrix") or {}).get("include") or []
    if not isinstance(name, str) or "${{" not in name:
        return [name] if isinstance(name, str) else []
    out = []
    for row in inc:
        n = name
        for k, v in row.items():
            n = n.replace("${{ matrix." + k + " }}", str(v))
        out.append(n)
    return out


def _acyclic(jobs: dict) -> str | None:
    seen: dict[str, int] = {}

    def visit(j: str, stack: list[str]) -> str | None:
        if seen.get(j) == 1:
            return " -> ".join(stack + [j])
        if seen.get(j) == 2:
            return None
        seen[j] = 1
        for d in _needs(jobs.get(j, {})):
            if d not in jobs:
                return f"{j} needs unknown job {d}"
            c = visit(d, stack + [j])
            if c:
                return c
        seen[j] = 2
        return None

    for j in jobs:
        c = visit(j, [])
        if c:
            return c
    return None


def _transitive_needs(jobs: dict, start: str) -> set[str]:
    out: set[str] = set()
    todo = list(_needs(jobs.get(start, {})))
    while todo:
        j = todo.pop()
        if j in out:
            continue
        out.add(j)
        todo += _needs(jobs.get(j, {}))
    return out


def lint(release: dict, publish: dict) -> list[str]:
    p: list[str] = []
    jobs = release.get("jobs") or {}
    # ---- G1: graph
    for j in (ACCEPTANCE, MANIFEST, PREPARE):
        if j not in jobs:
            p.append(f"G1: release.yml has no `{j}` job")
    if p:
        return p
    if ACCEPTANCE in _transitive_needs(jobs, MANIFEST):
        p.append(f"G1: `{MANIFEST}` needs `{ACCEPTANCE}` (transitively) -- the rev-3 deadlock cycle")
    if MANIFEST not in _needs(jobs[ACCEPTANCE]):
        p.append(f"G1: `{ACCEPTANCE}.needs` must include `{MANIFEST}` (A0b binds to the manifest)")
    tn = _transitive_needs(jobs, MANIFEST)
    for j in (PREPARE, *DIST_PRODUCERS):
        if j in jobs and j not in tn:
            p.append(f"G1: `{MANIFEST}` must (transitively) need `{j}`")
    if "if" in jobs[MANIFEST]:
        p.append("G1: `manifest` must not carry an `if:` (it must not run on a failed dependency)")
    cyc = _acyclic(jobs)
    if cyc:
        p.append(f"G1: `needs` cycle / dangling edge: {cyc}")
    # ---- G2: the fixed prerequisite constant == real job display names
    names: set[str] = set()
    for key, job in jobs.items():
        for n in _matrix_names(job) or [key]:
            names.add(n)
    expect = {PREPARE, MANIFEST} | {n for n in _matrix_names(jobs.get("build", {})) if n.startswith("Build ")}
    if set(REQUIRED_PREREQ_JOBS) != expect:
        p.append(f"G2: REQUIRED_PREREQ_JOBS {sorted(REQUIRED_PREREQ_JOBS)} != release.yml display names {sorted(expect)}")
    missing = set(REQUIRED_PREREQ_JOBS) - names
    if missing:
        p.append(f"G2: REQUIRED_PREREQ_JOBS names no real release.yml job: {sorted(missing)}")
    # ---- B4: RECORD_PRODUCER_JOB is BOTH the job key AND the API display name. The producer job must
    # exist under that key in its workflow, and if it declares a `name:`, that name must equal the key
    # (else the Actions /jobs API returns a display name RECORD_PRODUCER_JOB does not match -> an honest
    # record is rejected, the codex B4 deadlock). A descriptive rename is caught here.
    for rec, jobkey in RECORD_PRODUCER_JOB.items():
        wf = release if RECORD_PRODUCER_WORKFLOW[rec] == "release.yml" else publish
        pj_job = (wf.get("jobs") or {}).get(jobkey)
        if pj_job is None:
            p.append(f"B4: producer job {jobkey!r} for {rec} is not a job of {RECORD_PRODUCER_WORKFLOW[rec]} (RECORD_PRODUCER_JOB is stale)")
        elif isinstance(pj_job.get("name"), str) and pj_job["name"] != jobkey:
            p.append(f"B4: producer job {jobkey!r} for {rec} declares `name: {pj_job['name']!r}` != the key -- the Actions /jobs API returns the display name, which RECORD_PRODUCER_JOB[{rec!r}] does not match; set `name: {jobkey}` or drop the descriptive name:")
    # ---- R1: roles
    def role_steps(wf: dict, want: str, label: str, require_no_if: bool) -> int:
        n = 0
        for key, job in (wf.get("jobs") or {}).items():
            steps = _steps(job)
            for i, s in enumerate(steps):
                run = s.get("run")
                if not isinstance(run, str) or "require_evidence.py" not in run:
                    continue
                n += 1
                m = ROLE_RE.search(run)
                args = m.group("args") if m else ""
                if f"--role {want}" not in args:
                    p.append(f"R1: {label} job `{key}` step {i} invokes require_evidence.py without the literal `--role {want}`: {run.strip()[:120]}")
                if "${{" in (args.split("--role", 1)[1][:40] if "--role" in args else ""):
                    p.append(f"R1: {label} job `{key}` step {i} derives --role from an expression")
                if require_no_if and "if" in s:
                    p.append(f"R1: {label} job `{key}` step {i} (require_evidence) carries an `if:` -- a bypassable gate")
                if require_no_if:
                    for later in steps[:i]:
                        r2 = str(later.get("run", "")) + str(later.get("uses", ""))
                        if any(x in r2 for x in ("python -m build", "gh-action-pypi-publish", "gh run download", "pip install", "pip download", "verify_dist_manifest")):
                            p.append(f"R1: {label} job `{key}`: a build/download/install/publish step precedes the evidence gate")
        return n
    if role_steps(release, "release-run", "release.yml", False) == 0:
        p.append("R1: release.yml has no require_evidence.py consumer step")
    if role_steps(publish, "promotion", "publish-pypi.yml", True) < 2:
        p.append("R1: publish-pypi.yml must gate BOTH jobs with require_evidence.py --role promotion")
    for key, job in (publish.get("jobs") or {}).items():
        steps = _steps(job)
        if not any("require_evidence.py" in str(s.get("run", "")) for s in steps):
            p.append(f"R1: publish-pypi.yml job `{key}` has no require_evidence.py step")
    for label, wf in (("release.yml", release), ("publish-pypi.yml", publish)):
        inputs = ((_on(wf).get("workflow_dispatch") or {}) or {}).get("inputs") or {}
        for k in inputs:
            if k.lower() in ("role", "mode", "evidence_role", "skip_evidence"):
                p.append(f"R1: {label} exposes `{k}` as a workflow_dispatch input -- the role/gate must be a step literal")
    p += lint_m6(release, publish)
    # ---- PERM (codex pass-4): effective `actions: read` for every evidence/CI-consuming job in BOTH workflows
    p += actions_read_problems(release, "release.yml")
    p += actions_read_problems(publish, "publish-pypi.yml")
    # ---- REPRO: the cibuildwheel step MUST set SOURCE_DATE_EPOCH in CIBW_ENVIRONMENT so every rebuild is
    # byte-identical. A re-cut's new wheel bytes mismatching the immutable registry copy -- then a delete --
    # is what BURNED a TestPyPI filename on v0.2.6 (the delete permanently reserves the name). This is the
    # paired guard: removing the epoch turns it RED. Verified locally (build-twice identical with it, distinct
    # without). Memory: the wheel-reproducibility class.
    cibw_envs = [str((s.get("env") or {}).get("CIBW_ENVIRONMENT", ""))
                 for j in (release.get("jobs") or {}).values() for s in _steps(j)
                 if isinstance(s.get("env"), dict) and "CIBW_ENVIRONMENT" in s["env"]]
    if not cibw_envs:
        p.append("REPRO: release.yml has no CIBW_ENVIRONMENT (the cibuildwheel step could not be found)")
    elif not all("SOURCE_DATE_EPOCH=" in e for e in cibw_envs):
        p.append("REPRO: a cibuildwheel CIBW_ENVIRONMENT lacks `SOURCE_DATE_EPOCH=` -- wheels would not be "
                 "byte-reproducible across re-cuts (the class that burned a TestPyPI filename on v0.2.6)")
    # ---- PP (codex 2026-09-18): the `pypi` publish is RESUMABLE (skip-existing:true), so the POST-publish
    # set-proof (assemble_rtp against upload.pypi.org -> PyPI SERVES exactly the manifest set + digests) is
    # the ONLY thing that catches a partial/wrong PRODUCTION upload. This is that gate's PAIRED negative
    # control: it goes RED if the proof is deleted, mispointed away from PyPI, or reordered before the
    # publish -- so a green lint can never accompany an unverified irreversible publish (anti-vacuity, d′).
    pypi_job = (publish.get("jobs") or {}).get("pypi") or {}
    pypi_steps = _steps(pypi_job)
    # D1 (fable): the single-writer `concurrency:` guard is a shipped gate -> lint + pair it (removing it, or
    # cancel-in-progress:true, must go RED: two racing publishes each partial-upload the immutable set).
    conc = publish.get("concurrency") or {}
    if not conc:
        p.append("PP: publish-pypi.yml has no top-level `concurrency:` guard -- concurrent publishes could each "
                 "partially upload the immutable distribution set for the same version")
    else:
        if not str(conc.get("group", "")).strip():
            p.append("PP: `concurrency.group` is empty -- publishes for the same version (ref) must serialize")
        if conc.get("cancel-in-progress") is not False:
            p.append("PP: `concurrency.cancel-in-progress` must be literal false -- a publish must NEVER be cancelled "
                     "mid-upload (a cancelled run strands a partial set)")
    # C1 (fable): pin the testpypi R-TP assembler too -- the SAME command is exact-pinned on the pypi path but
    # was unlinted on the testpypi path (its `|| true` or removal linted GREEN).
    tp_proofs = [s for s in _steps((publish.get("jobs") or {}).get("testpypi") or {}) if "assemble_rtp.py" in str(s.get("run", ""))]
    if len(tp_proofs) != 1:
        p.append(f"PP: the `testpypi` job has {len(tp_proofs)} assemble_rtp steps -- exactly one R-TP emit is expected")
    else:
        rt = str(tp_proofs[0].get("run", ""))
        p += _gate_command_problems(rt, "assemble_rtp.py", "PP", "testpypi R-TP")
        p += _exact_pin_problems(rt, TESTPYPI_PROOF_PIN, "PP", "testpypi R-TP")
    # B-1/S-1 (fable pass-2): pin the publish steps' `with:` on BOTH jobs. The `testpypi` job runs FIRST
    # (gated only by R-NI) -- if its repository-url is edited to production, it IRREVERSIBLY publishes to
    # PyPI before any R-TV. And attestations MUST stay off (default true) or the sidecars break the exact-set
    # proof on a correct publish, with no re-dispatch able to recover (each regenerates them).
    for jobkey, expect_url in (("testpypi", "https://test.pypi.org/legacy/"), ("pypi", None)):
        pubs = [s for s in _steps((publish.get("jobs") or {}).get(jobkey) or {}) if "gh-action-pypi-publish" in str(s.get("uses", ""))]
        for ps in pubs:
            w = ps.get("with") or {}
            url = w.get("repository-url")
            if jobkey == "testpypi" and url != expect_url:
                p.append(f"PP: the `testpypi` publish repository-url is {url!r}, not {expect_url!r} -- a prod URL here "
                         "would IRREVERSIBLY publish to PyPI from the R-NI-only testpypi job")
            if jobkey == "pypi" and url is not None and "test.pypi.org" in str(url):
                p.append("PP: the `pypi` publish repository-url points at TestPyPI -- production must publish to PyPI")
            if w.get("attestations") is not False:
                p.append(f"PP: the `{jobkey}` publish must set attestations:false -- PEP 740 sidecars in dist/ break the "
                         "exact-set proof on a correct publish (and every re-dispatch regenerates them)")
    # exec-CONTEXT hardening (codex astra pass-4): the step-key allowlist + exact-pin still leave the proof
    # neuterable by settings OUTSIDE the step -- a JOB-level continue-on-error (a failed proof no longer
    # fails the run) or a failure-masking defaults.run.shell (workflow OR job scope, e.g. `bash -c '... ||
    # true'`) that the proof inherits. Enforce the effective execution context too.
    if "continue-on-error" in pypi_job and pypi_job.get("continue-on-error") is not False:
        # not `is True`: `continue-on-error: ${{ true }}` / `${{ 1==1 }}` is a STRING expression that also
        # suppresses job failure -- reject unless the key is literal `false` (codex astra pass-5).
        p.append(f"PP: the `pypi` job sets continue-on-error: {pypi_job.get('continue-on-error')!r} -- a FAILED "
                 "post-publish proof would not fail the workflow; only literal `false` (or absence) is permitted")
    for scope, d in (("workflow", publish.get("defaults")), ("pypi-job", pypi_job.get("defaults"))):
        run_defaults = (d or {}).get("run") or {}
        sh = run_defaults.get("shell")
        if sh is not None and str(sh) not in GATE_STEP_APPROVED_SHELLS:
            p.append(f"PP: {scope} defaults.run.shell {sh!r} is inherited by the proof and could mask its failure "
                     f"-- only {sorted(GATE_STEP_APPROVED_SHELLS)} is permitted")
        if "working-directory" in run_defaults:
            p.append(f"PP: {scope} defaults.run.working-directory is inherited by the proof and can mispoint it "
                     "(the manifest/dist it reads); the proof must run from the repo root")
    # inherited ENVIRONMENT: a workflow/job `env` (or a prior step writing $GITHUB_ENV/$GITHUB_PATH) reaches
    # the proof and can no-op it -- SHELLOPTS=noexec, BASH_ENV, PYTHONPATH (an auto-imported sitecustomize.py
    # can stub sys.exit so main() returns 1 but the process exits 0), PATH/LD_PRELOAD shims, etc. A blacklist
    # of env keys "never converges" (this file's own lesson, lines 141-145 / fable pass); the honest pypi path
    # has NO env at workflow or job scope, so ALLOWLIST-shape it: forbid `env` at those two scopes outright,
    # and forbid ANY `$GITHUB_ENV`/`$GITHUB_PATH` write anywhere in the pypi job (codex astra + fable).
    for scope, env in (("workflow", publish.get("env")), ("pypi-job", pypi_job.get("env"))):
        if env:
            p.append(f"PP: {scope} sets `env` ({sorted(env)}) -- inherited by the proof, any env can no-op it "
                     "(SHELLOPTS/BASH_ENV/PYTHONPATH/PATH/LD_PRELOAD); the pypi path needs none, so it is forbidden here")
    for s in pypi_steps:
        if re.search(r"\$?\{?GITHUB_(ENV|PATH)\}?", str(s.get("run", ""))):
            p.append("PP: a step in the `pypi` job writes $GITHUB_ENV/$GITHUB_PATH -- it can inject an env no-op "
                     "(SHELLOPTS/BASH_ENV) or a PATH shim (a fake python3) into the proof; the pypi job needs neither")
            break
    pub_idxs = [i for i, s in enumerate(pypi_steps) if "gh-action-pypi-publish" in str(s.get("uses", ""))]
    if not pub_idxs:
        p.append("PP: publish-pypi.yml `pypi` job has no gh-action-pypi-publish step")
    else:
        if len(pub_idxs) > 1:  # a 2nd publisher after the proof would upload OUTSIDE the proof (astra pass-5)
            p.append(f"PP: the `pypi` job has {len(pub_idxs)} publish steps -- EXACTLY ONE is expected so the "
                     "post-publish proof covers every upload")
        for pubi in pub_idxs:
            if (pypi_steps[pubi].get("with") or {}).get("skip-existing") is not True:
                p.append("PP: a `pypi` publish step is not `skip-existing: true` (RESUMABLE) -- a partial upload could "
                         "not be completed by a fresh re-dispatch and would strand the version")
                break
    pub_last = max(pub_idxs) if pub_idxs else None
    # The proof must be an EFFECTIVE gate, not merely PRESENT: a substring check passes when the step is
    # `echo`-prefixed, `|| true`-suffixed, `continue-on-error`/`if:false`, or points at TestPyPI with a
    # `# upload.pypi.org` comment (all reproduced GREEN by codex astra). Validate it the SAME way as the
    # promotion gates: exactly-one invocation + no shell control/comment, an execution-neutral step-key
    # allowlist, and an EXACT command pin -- every one of those bypasses then fails by construction.
    proof_idxs = [i for i, s in enumerate(pypi_steps) if "assemble_rtp.py" in str(s.get("run", ""))]
    if not proof_idxs:
        p.append("PP: the `pypi` job has NO post-publish set-proof (assemble_rtp.py against PyPI) -- a partial or "
                 "wrong PRODUCTION upload would go undetected on the irreversible path")
    elif len(proof_idxs) > 1:
        p.append(f"PP: the `pypi` job has {len(proof_idxs)} assemble_rtp steps -- EXACTLY ONE post-publish proof is expected")
    else:
        pi = proof_idxs[0]
        proof = pypi_steps[pi]
        run = str(proof.get("run", ""))
        p += _gate_command_problems(run, "assemble_rtp.py", "PP", "pypi post-publish proof")
        p += _gate_step_metadata_problems(proof, "PP", "pypi post-publish proof")
        p += _exact_pin_problems(run, PYPI_PROOF_PIN, "PP", "pypi post-publish proof")
        # TIGHTER than the shared gate allowlist: the proof needs NO env (it queries the PUBLIC PyPI JSON;
        # GITHUB_* is auto-provided) and NO working-directory (it runs from the repo root). Forbidding both
        # closes the `env: {SHELLOPTS: noexec}` / `env: {BASH_ENV: ...}` no-op and a mispointing
        # working-directory -- neither of which the general step-key allowlist catches (codex astra pass-4).
        proof_extra = sorted(k for k in proof if k not in {"name", "id", "run", "shell"})
        if proof_extra:
            p.append(f"PP: the post-publish proof step carries disallowed key(s) {proof_extra} -- it may use ONLY "
                     "name/id/run/shell (env can set SHELLOPTS/BASH_ENV to no-op the command; working-directory can mispoint it)")
        if pub_last is not None and pi != pub_last + 1:
            p.append("PP: the post-publish set-proof must run IMMEDIATELY after the (single) publish step -- any step "
                     "between them (or the proof before a publish) can rewrite assemble_rtp.py / release_manifest.json / "
                     "dist/ or delay the proof past a second upload (fable pass)")
    # A4 (fable): "exactly one publisher" also means no OTHER uploader in a run: -- a `twine upload`/`uv publish`
    # appended after the proof would upload OUTSIDE it. Reject those in any pypi-job run: step.
    for s in pypi_steps:
        if re.search(r"\b(twine\s+upload|python\s+-m\s+twine|uv\s+publish)\b", str(s.get("run", ""))):
            p.append("PP: a `pypi` run: step uploads via twine/uv directly -- only the pinned gh-action-pypi-publish "
                     "step may upload, so the post-publish proof covers every upload")
            break
    # the proof must be RETAINED as an immutable audit artifact, and that upload must be if: always() (astra +
    # fable: total removal AND dropping `if: always()` -- which skips the upload exactly on the RED case -- both
    # lint GREEN otherwise).
    rpp_uploads = [s for s in pypi_steps if "upload-artifact" in str(s.get("uses", ""))
                   and "PyPI-registry-proof" in str((s.get("with") or {}).get("path", ""))]
    if not rpp_uploads:
        p.append("PP: the `pypi` job does not upload the PyPI registry proof (PyPI-registry-proof.json) as an Actions "
                 "artifact -- the immutable audit record of the post-publish set proof is missing")
    elif str(rpp_uploads[0].get("if", "")).strip() not in ("always()", "${{ always() }}"):
        p.append("PP: the PyPI registry-proof artifact upload must be `if: always()` -- otherwise it SKIPS on the "
                 "failed-proof (RED) case, dropping the audit record exactly when it matters")
    # ---- W1/W2: the native shell-only window
    steps = _steps(jobs[ACCEPTANCE])
    scrub_idx = [i for i, s in enumerate(steps) if str(s.get("id", "")).startswith("scrub")]
    restore_idx = [i for i, s in enumerate(steps) if str(s.get("id", "")).startswith("restore")]
    if not scrub_idx or not restore_idx:
        p.append("W2: node-free-acceptance needs `scrub*` and `restore*` step ids (the native window boundaries)")
        return p
    lo, hi = min(scrub_idx), max(restore_idx)
    if hi < lo:
        p.append("W2: a restore step precedes the scrub step")
    for i in range(lo, hi + 1):
        if "uses" in steps[i]:
            p.append(f"W1: step {i} ({steps[i].get('name')}) is a `uses:` action INSIDE the scrubbed window (the runner's Node is unavailable there)")
    post = [i for i, s in enumerate(steps) if i > hi and "uses" in s]
    if not post:
        p.append("W2: no `uses:` step follows the restore step (the loud fail-safe upload is missing)")
    for i, s in enumerate(steps):
        if i > lo and "uses" in s and i < hi:
            p.append(f"W2: `uses:` step {i} runs after scrub but before restore")
    return p


def _run_text(s: dict) -> str:
    return str(s.get("run", "")) + " " + str(s.get("uses", ""))


def _step_index(steps: list[dict], needle: str) -> int | None:
    for i, s in enumerate(steps):
        if needle in _run_text(s) or needle in json_dumps_env(s):
            return i
    return None


def json_dumps_env(s: dict) -> str:
    env = s.get("env") or {}
    return " ".join(str(v) for v in env.values()) if isinstance(env, dict) else ""


def lint_m6(release: dict, publish: dict) -> list[str]:
    """The M6 producers + promotion gates (plan M6.1-M6.5)."""
    p: list[str] = []
    jobs = release.get("jobs") or {}
    # ---- M1: the manifest producer is real, uploads the artifact; prepare checks stamp idempotence
    ms = _steps(jobs.get(MANIFEST, {}))
    if _step_index(ms, "assemble_manifest.py") is None:
        p.append("M1: `manifest` job does not run scripts/assemble_manifest.py (the producer body has not landed / a fail-closed placeholder)")
    if any("exit 1" in _run_text(s) and "assemble_manifest" not in _run_text(s) for s in ms):
        p.append("M1: `manifest` job still carries an unconditional `exit 1` placeholder step")
    if not any("upload-artifact" in _run_text(s) and (s.get("with") or {}).get("name") == "release-manifest" for s in ms):
        p.append("M1: `manifest` job does not upload the `release-manifest` artifact")
    if _step_index(_steps(jobs.get(PREPARE, {})), "publish.mjs --check") is None:
        p.append("M1: `prepare` job does not run `node npm/publish.mjs --check` (stamp idempotence, requirements §5.4)")
    # ---- M2: npm-identity after npm-publish, evidence step before the identity script; publish gated after evidence
    ni = jobs.get("npm-identity")
    if ni is None:
        p.append("M2: release.yml has no `npm-identity` job (R-NI producer, plan M6.2)")
    else:
        if not {"npm-publish", MANIFEST} <= set(_needs(ni)):
            p.append(f"M2: `npm-identity.needs` {_needs(ni)} must include `npm-publish` and `manifest`")
        st = _steps(ni)
        ev, ident = _step_index(st, "require_evidence.py"), _step_index(st, "verify_npm_identity.py")
        if ident is None:
            p.append("M2: `npm-identity` does not run scripts/verify_npm_identity.py")
        else:
            if ev is None or ev > ident:
                p.append("M2: `npm-identity` must verify the manifest (require_evidence.py --role release-run) BEFORE verify_npm_identity.py")
            # S9 (codex pass-3): verify_npm_identity must bind plugin/wrapper CONTENT to the checkout
            # (`--checkout`), or the immutable-registry hole reopens. A SUBSTRING search let `# --checkout .`
            # (a comment) pass lint while the runtime skipped binding. Now the step is parsed STRUCTURALLY:
            # a single simple invocation (no shell operators, no comment) that carries a real `--checkout
            # <value>` token (a commented flag is stripped first, so it no longer satisfies the check).
            ident_run = str(st[ident].get("run", ""))
            p += _gate_command_problems(ident_run, "verify_npm_identity.py", "S9", "npm-identity")
            # Fix 1 (codex pass-5): the npm-identity gate STEP's non-`run:` metadata must not mask/skip it
            # (continue-on-error / if / background / custom shell) -- same allowlist as the promotion gates.
            p += _gate_step_metadata_problems(st[ident], "S9", "npm-identity")
            # S9 ROOT FIX (codex pass-4): pin the EXACT command -- `--checkout ""`, if/then/fi, a
            # trailing `true`, multiline, reserved words and negation all deviate from the pin -> RED,
            # which no operator blacklist could guarantee.
            p += _exact_pin_problems(ident_run, NPM_IDENTITY_PIN, "S9", "npm-identity")
            ident_stripped, _ = _strip_comments(ident_run)
            if not re.search(r"--checkout(\s+|=)(?!-)\S", ident_stripped):
                p.append("S9: `npm-identity` must run verify_npm_identity.py with a real `--checkout <dir>` token (bind plugin/wrapper published content to the checkout of S; a commented `# --checkout` does not count)")
    npmjob = jobs.get("npm-publish", {})
    npub = _steps(npmjob)
    pub = _step_index(npub, "publish.mjs --yes")
    # ── Acceptance MODE enforcement (invariants-as-failing-gates). The workflow MUST match ACCEPTANCE_MODE,
    # so neither the mode nor the continue-on-error/always() that make "advisory" SAFE can silently drift
    # (codex 2026-09-17: without this, a future edit could re-introduce the indirect PyPI block or stop
    # attaching a failed R-BA while the lint stayed green).
    nfe = jobs.get("node-free-evidence", {})
    if ADVISORY:
        for jn in ("node-free-acceptance", "node-free-evidence"):
            if jobs.get(jn, {}).get("continue-on-error") is not True:
                p.append(f"ADVISORY: `{jn}` must set `continue-on-error: true` -- acceptance is advisory, a failed "
                         f"leg must not fail the release run or indirectly gate PyPI. To GATE on it, set ACCEPTANCE_MODE='blocking'.")
        attach = next((s for s in _steps(nfe) if str(s.get("name", "")).startswith("Attach R-BA")), None)
        if attach is None or "always()" not in str(attach.get("if", "")):
            p.append("ADVISORY: node-free-evidence `Attach R-BA` step must be `if: always() && ...` (a fail-verdict R-BA must still attach to the Release).")
        if "node-free-evidence" in _needs(npmjob):
            p.append("ADVISORY: `npm-publish.needs` must NOT include `node-free-evidence` (acceptance advisory; npm does not gate on R-BA).")
        if _step_index(npub, "evidence-R-BA") is not None or _step_index(npub, "evidence/R-BA.json") is not None:
            p.append("ADVISORY: `npm-publish` must NOT consume/validate R-BA (acceptance advisory).")
    else:  # blocking: the full R-BA gate
        if "node-free-evidence" not in _needs(npmjob):
            p.append(f"BLOCKING: `npm-publish.needs` {_needs(npmjob)} must include `node-free-evidence` (npm follows node-free acceptance -> R-BA).")
        if _step_index(npub, "evidence/R-BA.json") is None:
            p.append("BLOCKING: `npm-publish` must consume+validate R-BA (evidence/R-BA.json) before publishing.")
    # B1: npm publication is reachable ONLY from an exact vX.Y.Z tag build.
    if "refs/tags/" not in str(npmjob.get("if", "")):
        p.append("B1: `npm-publish` must carry `if: startsWith(github.ref, 'refs/tags/')` (npm reachable only from a tag build)")
    # B1/B6: guard_tag_version, require_evidence, require_ci_success all BEFORE publish.mjs --yes.
    # (v0.2.5: the R-BA consume gate is removed here -- node-free acceptance is advisory; re-add in v0.2.6.)
    if pub is None:
        p.append("M2: `npm-publish` has no `node npm/publish.mjs --yes` step")
    else:
        gates = {
            "require_evidence.py": _step_index(npub, "require_evidence.py"),
            "guard_tag_version.py (B1)": _step_index(npub, "guard_tag_version.py"),
            "require_ci_success.py (B6)": _step_index(npub, "require_ci_success.py"),
        }
        for label, idx in gates.items():
            if idx is None or idx > pub:
                p.append(f"M2/B1/B2: `npm-publish` must run {label} BEFORE `node npm/publish.mjs --yes`")
        # B1: the publish step binds the manifest version.
        if "--manifest" not in _run_text(npub[pub]):
            p.append("B1: `npm-publish` publish step must pass `--manifest release_manifest.json` (publish version bound to the manifest)")
    # ---- M3: the GitHub Release of binaries needs the manifest
    rel = jobs.get("release")
    if rel is not None and MANIFEST not in _transitive_needs(jobs, "release"):
        p.append("M3: `release` (GitHub Release of the binaries) must need `manifest` (a Release without a manifest is ungated)")
    # ---- M4: README spots in every wheel leg's test command
    wheel = jobs.get("wheel", {})
    test_cmd = ""
    for s in _steps(wheel):
        env = s.get("env") or {}
        if isinstance(env, dict) and "CIBW_TEST_COMMAND" in env:
            test_cmd = str(env["CIBW_TEST_COMMAND"])
    if "readme_spots.py" not in test_cmd or "--run" not in test_cmd:
        p.append("M4: the `wheel` legs' CIBW_TEST_COMMAND does not run `scripts/readme_spots.py --run` (validation §K)")
    # ---- P1: publish-pypi.yml
    pj = publish.get("jobs") or {}
    for key in ("testpypi", "pypi"):
        job = pj.get(key)
        if job is None:
            p.append(f"P1: publish-pypi.yml has no `{key}` job")
            continue
        st = _steps(job)
        # S11: exactly one evidence step, `--need` once, the SET compared exactly (a superset like
        # `R-BA R-NI R-TV` contains the `R-BA R-NI` substring and would slip past a `not in` test; a
        # trailing `--need` or a second evidence step would require no records).
        p += need_problems(job, key, EXPECTED_NEED[key], "P1")
        vd = _step_index(st, "verify_dist_manifest.py")
        pubi = _step_index(st, "gh-action-pypi-publish")
        if vd is None or pubi is None or vd > pubi:
            p.append(f"P1: publish-pypi.yml `{key}` must run verify_dist_manifest.py BEFORE the publish action")
        elif key == "pypi" and "--rtp" not in _run_text(st[vd]):
            p.append("P1: publish-pypi.yml `pypi` must bind dist/ to R-TP (`verify_dist_manifest.py --rtp`)")
        if _step_index(st, "gh run download") is None:
            p.append(f"P1: publish-pypi.yml `{key}` must download the manifest-bound dist/ (`gh run download`), never rebuild")
    # ---- S1 (opus M6 B1): the intermediate testpypi jobs gate EXACTLY `--need R-BA R-NI`. Adding R-TP/R-TV
    # is self-referential -- their producer.run_id is THIS in-progress run, which `verify_records` requires
    # to be completed+success, so the gate can never pass -> the pipeline deadlocks (R-TV never emitted).
    for key in ("testpypi-validate", "testpypi-evidence"):
        job = pj.get(key)
        if job is None:
            continue  # `testpypi-validate` existence is asserted below; evidence is optional here
        # S11 + opus M6 B1: exactly one evidence step, `--need` once, the SET == {R-BA, R-NI} (adding
        # R-TP/R-TV self-references an in-progress run and would deadlock).
        p += need_problems(job, key, EXPECTED_NEED[key], "S1")
    for key, job in pj.items():
        for s in _steps(job):
            if "python -m build" in _run_text(s):
                p.append(f"P1: publish-pypi.yml job `{key}` REBUILDS (`python -m build`) -- promotion must ship the manifest-bound bytes")
    tv = pj.get("testpypi-validate")
    if tv is None:
        p.append("P1: publish-pypi.yml has no `testpypi-validate` job (R-TV legs)")
    else:
        if "testpypi" not in _needs(tv):
            p.append("P1: `testpypi-validate.needs` must include `testpypi`")
        got = {r.get("target") for r in (((tv.get("strategy") or {}).get("matrix") or {}).get("include") or [])}
        from pythscribe.build._native import TRIPLE_TO_TAG  # noqa: PLC0415
        if got != set(TRIPLE_TO_TAG):
            p.append(f"P1: `testpypi-validate` matrix targets {sorted(got)} != the release target set {sorted(TRIPLE_TO_TAG)}")
        if _step_index(_steps(tv), "testpypi_validate.py") is None:
            p.append("P1: `testpypi-validate` does not run scripts/testpypi_validate.py")
    if not any(_step_index(_steps(job), "assemble_rtv.py") is not None and "testpypi-validate" in _needs(job) for job in pj.values()):
        p.append("P1: no publish-pypi.yml job assembles R-TV (assemble_rtv.py) after `testpypi-validate`")
    return p


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--release", default=str(REPO / ".github" / "workflows" / "release.yml"))
    ap.add_argument("--publish", default=str(REPO / ".github" / "workflows" / "publish-pypi.yml"))
    ns = ap.parse_args(argv)
    rel = yaml.safe_load(Path(ns.release).read_text(encoding="utf-8"))
    pub = yaml.safe_load(Path(ns.publish).read_text(encoding="utf-8"))
    problems = lint(rel, pub)
    if problems:
        print(f"lint_release_workflows: RED ({len(problems)}):", file=sys.stderr)
        for x in problems:
            print(f"  {x}", file=sys.stderr)
        return 1
    print("lint_release_workflows: GREEN -- job graph acyclic (manifest -> acceptance), roles literal, native window shell-only, restore before upload")
    return 0


if __name__ == "__main__":
    sys.exit(main())
