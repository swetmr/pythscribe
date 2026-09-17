#!/usr/bin/env python3
"""B6: verify a required CI workflow ran GREEN at the EXACT commit SHA before any publication.

    python scripts/require_ci_success.py [--sha <40-hex>] [--workflow ci.yml]
      # --sha defaults to $GITHUB_SHA; needs $GITHUB_REPOSITORY (+ a token for private repos)

A tag push triggers release.yml, which BUILDS and PUBLISHES but runs no conformance tests -- every
gate lives in ci.yml on the main-branch PUSH of the commit being tagged. Nothing verified that ci.yml
was actually GREEN at that exact commit before npm/PyPI publication; a direct tag push over a red or
skipped CI run would publish anyway.

codex re-review B6 (PARTIAL->closed): comparing only the run's head_sha + overall conclusion accepted
a MANUAL `workflow_dispatch` CI run at the SHA -- where the push-only packaging job SKIPS (ci.yml
gates it on `event == 'push' && ref == 'refs/heads/main'`), yet the overall run is still `success`.
This now (1) requires the run's `event == 'push'` AND `head_branch == 'main'` and (2) verifies each
job of the FIXED REQUIRED_CI_JOBS set concluded `success` on its latest attempt in that run -- not
merely the overall run conclusion. Exit 0 GREEN; 1 RED; 2 usage.
"""
from __future__ import annotations

import argparse
import itertools
import json
import os
import sys
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Callable

DEFAULT_WORKFLOW = "ci.yml"
REPO = Path(__file__).resolve().parents[1]
CI_WORKFLOW_PATH = REPO / ".github" / "workflows" / "ci.yml"

# B6 (codex pass-3): the FIXED set of ci.yml jobs whose success is load-bearing for a release, by their
# COMPLETE Actions-API DISPLAY name -- a matrix job is enumerated as EVERY expanded leg, NOT a `<name> (`
# prefix. The prefix + `max()` scheme checked only ONE `Test (...)` leg, so Ubuntu=ok / Windows=fail /
# macOS=ok returned GREEN (the codex reproduction). Now every expected leg must be present and its latest
# attempt successful. `Packaging (fresh install)` is push+main-only -- the manual-dispatch skip bypass.
# `bench`/`lighthouse`/`coverage`/`kani` (informational / perf / bounded-model) are deliberately excluded.
# `ci_job_display_names()` recomputes these from ci.yml and `binding_problems()` asserts the set matches,
# so a rename or a matrix change to a required leg fails LOUD (a lint/test can never silently drop a leg).
REQUIRED_CI_JOBS: frozenset[str] = frozenset({
    "Test (ubuntu-latest, 1.94.0)",                # matrix: os x rust, each leg required (no single-leg pass)
    "Test (windows-latest, 1.94.0)",
    "Test (macos-latest, 1.94.0)",
    "pythscribe pip gate",
    "Lint",
    "Node + browser",
    "Packaging (fresh install)",                   # push+main-only -- the manual-dispatch skip bypass
    "Lean proofs",
    "Comparator (independent-kernel re-check)",
    "Clone parity (tri-track)",
    "M7 web-parity (frontend from the base wheel)",
})
# B6 (codex pass-5) ROOT FIX -- bind by required job KEY, not merely by display name. Before, binding was
# keyed only on DISPLAY names, so an attacker could delete `jobs.lint` and add a no-op job under key
# `fake-lint` with `name: Lint`: the display name "Lint" still existed, so both `binding_problems()` and
# an all-green `verify()` accepted the run -- a required job replaced by an unrelated one reusing its name.
# The FIXED authority is now the set of ci.yml job KEYS below; each required leg's COMPLETE display-name
# family is DERIVED from these exact keys against ci.yml. A missing/renamed required KEY is RED, a duplicate
# display name across keys is RED (ambiguous binding), and the derived family must equal REQUIRED_CI_JOBS
# exactly (a required leg that vanished, or an added matrix leg not in REQUIRED_CI_JOBS, is RED). These keys
# and REQUIRED_CI_JOBS are pinned to agree with ci.yml by binding_problems() (asserted GREEN on the real
# ci.yml in the test suite), so neither the key set nor the name set can silently drift.
REQUIRED_CI_JOB_KEYS: frozenset[str] = frozenset({
    "test",            # matrix os x rust -> the three Test (<os>, 1.94.0) legs
    "pip-gate",        # -> "pythscribe pip gate"
    "lint",            # -> "Lint"
    "node-and-e2e",    # -> "Node + browser"
    "packaging",       # -> "Packaging (fresh install)" (push+main-only -- the manual-dispatch skip bypass)
    "verification",    # -> "Lean proofs"
    "comparator",      # -> "Comparator (independent-kernel re-check)"
    "clones",          # -> "Clone parity (tri-track)"
    "web-parity",      # -> "M7 web-parity (frontend from the base wheel)"
})
# A callable `(workflow_file, head_sha) -> [run dicts]` / `(run_id) -> [job dicts]` -- injectable for tests/offline.
RunsFetcher = Callable[[str, str], list[dict]]
JobsFetcher = Callable[[int], list[dict]]


# ----------------------------------------------------------------------------- ci.yml job-name binding (B6)
def _matrix_combos(matrix: dict) -> list[dict]:
    """Every combination of a job's `strategy.matrix` (the cross product of its list dimensions, then
    any `include` rows appended). `exclude` is honoured. ci.yml's only matrix (Test: os x rust) has no
    include/exclude, but the general expansion keeps the binding correct if one is added later."""
    dims = {k: v for k, v in matrix.items() if k not in ("include", "exclude") and isinstance(v, list)}
    keys = list(dims)
    combos = [dict(zip(keys, vals)) for vals in itertools.product(*(dims[k] for k in keys))] if keys else [{}]
    excl = matrix.get("exclude") or []
    combos = [c for c in combos if not any(all(c.get(k) == v for k, v in e.items()) for e in excl)]
    for inc in matrix.get("include") or []:
        merged = False
        for c in combos:
            if all(c.get(k) == v for k, v in inc.items() if k in c):
                c.update(inc)
                merged = True
        if not merged:
            combos.append(dict(inc))
    return combos


def _job_display_names(job_key: str, job: dict) -> list[str]:
    """The Actions-API display name(s) of a job. A matrix job with an explicit `name:` that contains no
    `${{ matrix.* }}` expression is auto-suffixed by GitHub as `<name> (v1, v2, ...)` -- values in
    matrix-declaration order, comma-space separated. A `${{ matrix.k }}` in the name is substituted."""
    name = job.get("name") or job_key
    matrix = ((job.get("strategy") or {}).get("matrix") or {})
    combos = _matrix_combos(matrix)
    if not matrix or combos == [{}]:
        return [name]
    out: list[str] = []
    for c in combos:
        if "${{" in name:
            n = name
            for k, v in c.items():
                n = n.replace("${{ matrix." + k + " }}", str(v)).replace("${{matrix." + k + "}}", str(v))
            out.append(n)
        else:
            out.append(f"{name} (" + ", ".join(str(c[k]) for k in c) + ")")
    return out


def ci_job_display_names(ci_path: Path = CI_WORKFLOW_PATH) -> set[str]:
    """Every job display name ci.yml actually produces (matrix legs expanded)."""
    import yaml  # lazy: only the static binding check needs a YAML parser, not the API path
    ci = yaml.safe_load(ci_path.read_text(encoding="utf-8"))
    names: set[str] = set()
    for key, job in (ci.get("jobs") or {}).items():
        if isinstance(job, dict):
            names.update(_job_display_names(key, job))
    return names


def _ci_job_names_by_key(ci_path: Path = CI_WORKFLOW_PATH) -> dict[str, set[str]]:
    """{job key: the COMPLETE set of Actions display names it produces (matrix legs expanded)}."""
    import yaml  # lazy: only the static binding check needs a YAML parser, not the API path
    ci = yaml.safe_load(ci_path.read_text(encoding="utf-8"))
    out: dict[str, set[str]] = {}
    for key, job in (ci.get("jobs") or {}).items():
        if isinstance(job, dict):
            out[key] = set(_job_display_names(key, job))
    return out


def binding_problems(ci_path: Path = CI_WORKFLOW_PATH) -> list[str]:
    """Bind the required CI jobs to ci.yml by required job KEY (codex pass-5, B6 ROOT FIX):

      (1) KEY EXISTENCE -- every REQUIRED_CI_JOB_KEYS entry must be a real ci.yml job KEY. A required job
          deleted, or its KEY renamed, is RED here -- so the key-substitution attack (delete `jobs.lint`,
          add `fake-lint` with `name: Lint`) can no longer pass: the required KEY `lint` is gone.
      (2) NO DUPLICATE DISPLAY NAMES ACROSS KEYS -- if two job keys expand to the same display name the
          run->required binding is ambiguous (a foreign job could shadow a required one by reusing its
          name) -> RED. This catches keeping `jobs.lint` while ADDING `fake-lint` also named `Lint`.
      (3) EXACT FAMILY -- the COMPLETE display-name family DERIVED from exactly REQUIRED_CI_JOB_KEYS must
          equal REQUIRED_CI_JOBS. A required leg that vanished/renamed (missing) is RED, and an added
          matrix leg on a required job (`freebsd-latest` on `test` -> `Test (freebsd-latest, 1.94.0)`,
          not in REQUIRED_CI_JOBS) is RED -- the required matrix family stays EXACTLY the ci.yml legs."""
    names_by_key = _ci_job_names_by_key(ci_path)
    problems: list[str] = []
    # (1) every required KEY must exist in ci.yml (deletion / KEY rename -> RED)
    for k in sorted(REQUIRED_CI_JOB_KEYS):
        if k not in names_by_key:
            problems.append(f"required CI job KEY {k!r} is not a job of ci.yml (deleted or the KEY renamed?) -- "
                            f"a required job cannot be dropped or replaced by a differently-keyed job reusing its "
                            f"display name (REQUIRED_CI_JOB_KEYS is the authority, not the display name)")
    # (2) no display name produced by more than one KEY (ambiguous run->required binding -> RED)
    name_to_keys: dict[str, set[str]] = {}
    for key, names in names_by_key.items():
        for n in names:
            name_to_keys.setdefault(n, set()).add(key)
    for n, keys in sorted(name_to_keys.items()):
        if len(keys) > 1:
            problems.append(f"CI display name {n!r} is produced by MULTIPLE job keys {sorted(keys)} -- the "
                            f"run->required-job binding is by display name, so a duplicate name lets one job "
                            f"shadow another (e.g. a no-op job reusing a required job's name); rename one")
    # (3) the family DERIVED from exactly the required KEYS must equal REQUIRED_CI_JOBS
    derived: set[str] = set().union(*(names_by_key[k] for k in REQUIRED_CI_JOB_KEYS if k in names_by_key)) \
        if any(k in names_by_key for k in REQUIRED_CI_JOB_KEYS) else set()
    for missing in sorted(REQUIRED_CI_JOBS - derived):
        problems.append(f"required CI job {missing!r} is not produced by any required KEY in ci.yml (renamed / "
                        f"matrix changed / dropped?) -- REQUIRED_CI_JOBS is stale against ci.yml")
    for extra in sorted(derived - REQUIRED_CI_JOBS):
        problems.append(f"ci.yml required job expands to leg {extra!r} that is NOT in REQUIRED_CI_JOBS -- a "
                        f"required matrix family must be EXACTLY the ci.yml legs (add the new leg to "
                        f"REQUIRED_CI_JOBS, or it silently escapes the gate)")
    return problems


def github_runs_fetcher(repo: str, token: str | None, api_url: str = "https://api.github.com") -> RunsFetcher:
    def fetch(workflow: str, head_sha: str) -> list[dict]:
        url = (f"{api_url.rstrip('/')}/repos/{repo}/actions/workflows/{urllib.parse.quote(workflow)}/runs?"
               + urllib.parse.urlencode({"head_sha": head_sha, "per_page": 100}))
        req = urllib.request.Request(url, headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            **({"Authorization": f"Bearer {token}"} if token else {}),
        })
        with urllib.request.urlopen(req, timeout=60) as r:  # noqa: S310 -- fixed https API host
            return json.loads(r.read().decode("utf-8")).get("workflow_runs", [])
    return fetch


def github_jobs_fetcher(repo: str, token: str | None, api_url: str = "https://api.github.com") -> JobsFetcher:
    def fetch(run_id: int) -> list[dict]:
        jobs: list[dict] = []
        page = 1
        while True:
            url = (f"{api_url.rstrip('/')}/repos/{repo}/actions/runs/{run_id}/jobs?"
                   + urllib.parse.urlencode({"filter": "all", "per_page": 100, "page": page}))
            req = urllib.request.Request(url, headers={
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
                **({"Authorization": f"Bearer {token}"} if token else {}),
            })
            with urllib.request.urlopen(req, timeout=60) as r:  # noqa: S310 -- fixed https API host
                chunk = json.loads(r.read().decode("utf-8")).get("jobs", [])
            jobs.extend(chunk)
            if len(chunk) < 100:
                return jobs
            page += 1
    return fetch


def required_job_problems(jobs: list[dict]) -> list[str]:
    """B6: EVERY REQUIRED_CI_JOBS leg (by its COMPLETE display name) must be present, and its LATEST
    attempt must have concluded success. A matrix leg is matched EXACTLY -- so a run where one leg
    (Windows) failed while the siblings (Ubuntu/macOS) passed is RED, and a leg that never ran (a
    manual-dispatch skip) is RED. Grouping by complete name means no `max()` over a prefix can hide a
    failed sibling leg (the codex pass-3 reproduction).

    B6 (codex pass-6): the selection at the latest attempt is WORST-CONCLUSION-WINS over duplicate
    rows, NOT list order. If the API returns two rows for the same display name at the SAME attempt
    (a duplicate), a `success` row can no longer mask a `failure` row merely by appearing first (or
    last) -- ANY non-success row at the selected attempt is RED. Fail closed on duplicate disagreement:
    the release gate never trusts an order-of-arrival tie-break."""
    # name -> {attempt -> [conclusion, ...]} : keep EVERY row (do not collapse duplicates to one rep).
    by_name: dict[str, dict[int, list]] = {}
    for j in jobs:
        name = str(j.get("name", ""))
        attempt = int(j.get("run_attempt", 1))
        by_name.setdefault(name, {}).setdefault(attempt, []).append(j.get("conclusion"))
    p: list[str] = []
    for required in sorted(REQUIRED_CI_JOBS):
        attempts = by_name.get(required)
        if not attempts:
            p.append(f"required CI job `{required}` did not run in this CI run (skipped on a manual dispatch, or a matrix leg missing?) -- do NOT publish over a partial CI run")
            continue
        latest_attempt = max(attempts)
        conclusions = attempts[latest_attempt]
        # WORST-conclusion-wins: any non-success row at the selected attempt fails the leg, regardless
        # of order or of how many duplicate rows agree (codex pass-6: no success-first GREEN masking).
        bad = [c for c in conclusions if c != "success"]
        if bad:
            extra = f" ({len(conclusions)} rows at that attempt, {len(bad)} non-success)" if len(conclusions) > 1 else ""
            p.append(f"required CI job `{required}` (attempt {latest_attempt}) concluded {bad[0]!r}, not success{extra}")
    return p


def verify(sha: str, *, workflow: str = DEFAULT_WORKFLOW, fetch_runs: RunsFetcher, fetch_jobs: JobsFetcher | None = None,
           ci_path: Path = CI_WORKFLOW_PATH) -> list[str]:
    """RED lines (empty == GREEN). Requires a completed+successful `event==push`, `head_branch==main`
    run at EXACTLY `sha`, AND every REQUIRED_CI_JOBS leg green in it. B6 (codex pass-3): `fetch_jobs`
    is MANDATORY -- with no jobs fetcher the run's per-leg success cannot be verified, so it FAILS
    CLOSED (never a runs-only GREEN that trusts the overall conclusion of a partial matrix).

    B6 (codex pass-4): the EXACT ci.yml matrix-family binding (`binding_problems`) now runs INSIDE
    production `verify()` -- not only in the offline lint -- so a drift (a new/renamed required leg, a
    silently-widened required matrix) fails the release gate itself, before any run is trusted."""
    bp = binding_problems(ci_path)
    if bp:
        return bp  # a stale/widened required-job binding is fatal: never verify against a drifted contract
    if not sha:
        return ["no commit SHA to verify CI against (GITHUB_SHA / --sha is empty)"]
    if fetch_jobs is None:
        return ["no jobs fetcher supplied -- the per-leg CI job set cannot be verified; fail closed "
                "(never publish over a run whose individual required jobs were not checked)"]
    try:
        runs = fetch_runs(workflow, sha)
    except Exception as e:  # noqa: BLE001 -- an unreachable CI API is a FAILURE, never a pass
        return [f"could not query `{workflow}` runs at {sha}: {e}"]
    ok = [r for r in runs
          if r.get("head_sha") == sha and r.get("status") == "completed" and r.get("conclusion") == "success"
          and r.get("event") == "push" and r.get("head_branch") == "main"]
    if not ok:
        seen = [{"event": r.get("event"), "branch": r.get("head_branch"), "status": r.get("status"), "conclusion": r.get("conclusion")}
                for r in runs if r.get("head_sha") == sha] or "none at this sha"
        return [f"no completed+successful PUSH-to-main `{workflow}` run at commit {sha} (found {seen}) -- "
                "do NOT publish over a red / skipped / manual-dispatch / absent required CI run"]
    # Verify the fixed required job set individually in the (latest) matching run -- a workflow_dispatch
    # run would already be rejected above, but a push run could still skip / fail a required leg.
    problems: list[str] = []
    for run in sorted(ok, key=lambda r: int(r.get("run_attempt", 1)), reverse=True):
        rid = run.get("id")
        try:
            jobs = fetch_jobs(int(rid))
        except Exception as e:  # noqa: BLE001
            problems = [f"could not query jobs of run {rid}: {e}"]
            continue
        jp = required_job_problems(jobs)
        if not jp:
            return []  # this matching run has every required job green
        problems = [f"CI run {rid}: {x}" for x in jp]
    return problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="require_ci_success.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--sha", default=os.environ.get("GITHUB_SHA", ""))
    ap.add_argument("--workflow", default=DEFAULT_WORKFLOW)
    ns = ap.parse_args(argv)
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        print("require_ci_success: GITHUB_REPOSITORY is not set", file=sys.stderr)
        return 2
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    api_url = os.environ.get("GITHUB_API_URL", "https://api.github.com")
    problems = verify(ns.sha, workflow=ns.workflow,
                      fetch_runs=github_runs_fetcher(repo, token, api_url),
                      fetch_jobs=github_jobs_fetcher(repo, token, api_url))
    if problems:
        print("require_ci_success: RED:", file=sys.stderr)
        for x in problems:
            print(f"  {x}", file=sys.stderr)
        return 1
    print(f"require_ci_success: GREEN -- `{ns.workflow}` is a completed+success push-to-main run at {ns.sha[:12]} with every required job green")
    return 0


if __name__ == "__main__":
    sys.exit(main())
