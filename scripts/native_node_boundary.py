#!/usr/bin/env python3
"""M4 native-leg node boundary check -- A4b / A5b (spec 13-09-26, plan M4 topology rev-6..8; validation §A).

Runs as root/admin on the macOS / Windows acceptance runner. It enumerates EVERY `node` / `node.exe` /
`nodejs*` FILE on the box and probes EACH one as the unprivileged `app` user with the PLATFORM-CORRECT
probe. Fail-closed and NON-POLLING: it asks the kernel's permission decision, not a process list, so a
short-lived child cannot slip past it. The process-tree sampler is a diagnostic, never the verdict.

    python native_node_boundary.py probe-one <file>            exit 0 == DENIED for `app`; 1 == reachable;
                                                               3 == probe broken (callers treat != 0 as RED)
    python native_node_boundary.py check --allow-under ~ctl <runner>/externals --app-tmp <dir> --out A4b.json

Probes (validation §A "native-leg node boundary", rev-7 SF-A):
  * POSIX (macOS): in a REAL `app`-uid subprocess (`sudo -u app`), `open(p, "rb")` must raise
    PermissionError(EACCES) AND `os.access(p, X_OK)` must be False (POSIX `access` evaluates the real
    permission bits, so it is sound here). Liveness: the subprocess itself must run and report JSON.
  * Windows: `os.access` is DACL-blind and is NEVER consulted. The load-bearing probe is a REAL exec
    attempt as `app` via `CreateProcessWithLogonW` (`"<p>" -e 0`) which must FAIL with WinError 5
    (ERROR_ACCESS_DENIED); AND `open(p, "rb")` run as `app` (python launched with the same primitive)
    must exit non-zero. Liveness: `python -c pass` launched as `app` must exit 0 (proves the app logon
    works and python is reachable, so a non-zero open is the file's denial, not the probe's failure).
    A logon failure (bad credentials etc.) is `probe broken` -> RED, never a silent pass.
  * Shared-temp check: as `ctl`, creating a file inside `app`'s TMPDIR must FAIL (no shared writable
    dir between the app and the controller).

Credentials come from the environment set by the workflow's ACQUIRE step: ACC_APP_USER (default `app`),
ACC_CTL_USER (default `ctl`), and on Windows ACC_APP_PASSWORD / ACC_CTL_PASSWORD.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from wheel_acceptance import a0_verdict, find_node_files, which_on_path  # noqa: E402

APP_USER = os.environ.get("ACC_APP_USER", "app")
CTL_USER = os.environ.get("ACC_CTL_USER", "ctl")
DENIED, REACHABLE, BROKEN = 0, 1, 3

_POSIX_SNIPPET = (
    "import json, os, sys, errno\n"
    "p = sys.argv[1]\n"
    "r = {'uid': os.getuid(), 'access_x': os.access(p, os.X_OK), 'open': 'ok'}\n"
    "try:\n"
    "    open(p, 'rb').close()\n"
    "except PermissionError as e:\n"
    "    r['open'] = 'EACCES' if e.errno == errno.EACCES else f'perm:{e.errno}'\n"
    "except OSError as e:\n"
    "    r['open'] = f'oserror:{e.errno}'\n"
    "print(json.dumps(r))\n"
)


# ----------------------------------------------------------------------------- POSIX (macOS)


def _posix_run_as(user: str, argv: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
    return subprocess.run(["sudo", "-n", "-u", user, "-H", *argv], capture_output=True, text=True, timeout=timeout)


def probe_posix(path: Path) -> tuple[int, str]:
    r = _posix_run_as(APP_USER, [sys.executable, "-c", _POSIX_SNIPPET, str(path)])
    if r.returncode != 0:
        return BROKEN, f"app-uid subprocess failed ({r.returncode}): {r.stderr.strip()[-300:]}"
    try:
        j = json.loads(r.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        return BROKEN, f"unparseable probe output: {r.stdout[-300:]}"
    if j.get("uid") == 0:
        return BROKEN, "probe ran as root, not as app"
    denied = j.get("open") == "EACCES" and j.get("access_x") is False
    return (DENIED if denied else REACHABLE), json.dumps(j)


def shared_tmp_posix(app_tmp: Path) -> tuple[bool, str]:
    st = app_tmp.stat()
    if st.st_mode & 0o077:
        return False, f"{app_tmp} mode {oct(st.st_mode & 0o777)} is group/other accessible"
    r = _posix_run_as(CTL_USER, [sys.executable, "-c", "import sys; open(sys.argv[1]+'/ctl-probe','w').close()", str(app_tmp)])
    return (r.returncode != 0), f"ctl create-in-app-tmp rc={r.returncode} {r.stderr.strip()[-200:]}"


# ----------------------------------------------------------------------------- Windows


def _win_exec_as(user: str, password: str, cmdline: str, timeout: int = 60) -> tuple[bool, int, int | None]:
    """CreateProcessWithLogonW. Returns (launched, winerror_if_not_launched, exit_code)."""
    import ctypes
    from ctypes import wintypes

    class STARTUPINFO(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("lpReserved", wintypes.LPWSTR), ("lpDesktop", wintypes.LPWSTR),
                    ("lpTitle", wintypes.LPWSTR), ("dwX", wintypes.DWORD), ("dwY", wintypes.DWORD),
                    ("dwXSize", wintypes.DWORD), ("dwYSize", wintypes.DWORD), ("dwXCountChars", wintypes.DWORD),
                    ("dwYCountChars", wintypes.DWORD), ("dwFillAttribute", wintypes.DWORD), ("dwFlags", wintypes.DWORD),
                    ("wShowWindow", wintypes.WORD), ("cbReserved2", wintypes.WORD), ("lpReserved2", ctypes.c_void_p),
                    ("hStdInput", wintypes.HANDLE), ("hStdOutput", wintypes.HANDLE), ("hStdError", wintypes.HANDLE)]

    class PROCESS_INFORMATION(ctypes.Structure):
        _fields_ = [("hProcess", wintypes.HANDLE), ("hThread", wintypes.HANDLE), ("dwProcessId", wintypes.DWORD), ("dwThreadId", wintypes.DWORD)]

    advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    fn = advapi32.CreateProcessWithLogonW
    fn.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPWSTR,
                   wintypes.DWORD, ctypes.c_void_p, wintypes.LPCWSTR, ctypes.POINTER(STARTUPINFO), ctypes.POINTER(PROCESS_INFORMATION)]
    fn.restype = wintypes.BOOL
    LOGON_WITH_PROFILE, CREATE_NO_WINDOW, STILL_ACTIVE = 1, 0x08000000, 259
    si = STARTUPINFO()
    si.cb = ctypes.sizeof(si)
    pi = PROCESS_INFORMATION()
    buf = ctypes.create_unicode_buffer(cmdline)
    ok = fn(user, ".", password, LOGON_WITH_PROFILE, None, buf, CREATE_NO_WINDOW, None, None, ctypes.byref(si), ctypes.byref(pi))
    if not ok:
        return False, ctypes.get_last_error(), None
    kernel32.WaitForSingleObject(pi.hProcess, int(timeout * 1000))
    code = wintypes.DWORD()
    kernel32.GetExitCodeProcess(pi.hProcess, ctypes.byref(code))
    if code.value == STILL_ACTIVE:
        kernel32.TerminateProcess(pi.hProcess, 1)
    kernel32.CloseHandle(pi.hProcess)
    kernel32.CloseHandle(pi.hThread)
    return True, 0, int(code.value)


def _win_creds(user_env: str, pw_env: str, default_user: str) -> tuple[str, str]:
    user = os.environ.get(user_env, default_user)
    pw = os.environ.get(pw_env)
    if not pw:
        raise SystemExit(f"native_node_boundary: {pw_env} is not set (the ACQUIRE step exports the {user} credentials)")
    return user, pw


def probe_windows(path: Path) -> tuple[int, str]:
    user, pw = _win_creds("ACC_APP_USER", "ACC_APP_PASSWORD", "app")
    # liveness: python runs as app at all
    launched, err, code = _win_exec_as(user, pw, f'"{sys.executable}" -c "pass"')
    if not launched or code != 0:
        return BROKEN, f"liveness: python as {user} launched={launched} winerror={err} exit={code}"
    # the load-bearing probe: a REAL exec attempt of the node binary as app
    launched, err, code = _win_exec_as(user, pw, f'"{path}" -e 0')
    if launched:
        return REACHABLE, f"exec as {user} LAUNCHED (exit {code}) -- node is executable by app"
    if err != 5:
        return BROKEN, f"exec as {user} failed with WinError {err}, not 5 (ERROR_ACCESS_DENIED)"
    # and open() as app must fail (Access denied)
    snippet = "import sys; open(sys.argv[1], 'rb').close()"
    launched, err2, code2 = _win_exec_as(user, pw, f'"{sys.executable}" -c "{snippet}" "{path}"')
    if not launched:
        return BROKEN, f"open-probe python as {user} did not launch (WinError {err2})"
    if code2 == 0:
        return REACHABLE, f"open() as {user} succeeded -- node is readable by app"
    return DENIED, f"exec WinError 5; open() as {user} exit {code2}"


def shared_tmp_windows(app_tmp: Path) -> tuple[bool, str]:
    user, pw = _win_creds("ACC_CTL_USER", "ACC_CTL_PASSWORD", "ctl")
    launched, err, code = _win_exec_as(user, pw, f'"{sys.executable}" -c "pass"')
    if not launched or code != 0:
        return False, f"ctl liveness failed launched={launched} winerror={err} exit={code}"
    snippet = "import sys, os; open(os.path.join(sys.argv[1], 'ctl-probe'), 'w').close()"
    launched, err, code = _win_exec_as(user, pw, f'"{sys.executable}" -c "{snippet}" "{app_tmp}"')
    return (launched and code != 0), f"ctl create-in-app-tmp launched={launched} exit={code}"


# ----------------------------------------------------------------------------- dispatch


def probe_one(path: Path) -> tuple[int, str]:
    return probe_windows(path) if os.name == "nt" else probe_posix(path)


def probe_for_verdict(path: Path) -> tuple[bool, str]:
    rc, detail = probe_one(path)
    return rc == DENIED, detail  # BROKEN counts as reachable: fail-closed


def check(allow_under: list[Path], app_tmp: Path | None, roots: list[Path] | None, step: str) -> dict:
    roots = roots or ([Path("/")] if os.name != "nt" else [Path(f"{d}:\\") for d in "CDEF" if Path(f"{d}:\\").exists()])
    found = find_node_files(roots, skip=[Path("/proc"), Path("/sys"), Path("/dev"), Path("/private/var/folders")])
    probes: list[dict] = []

    def probe(p: Path) -> tuple[bool, str]:
        rc, detail = probe_one(p)
        probes.append({"path": str(p), "rc": rc, "denied": rc == DENIED, "detail": detail})
        return rc == DENIED, detail

    problems = a0_verdict(found, allow_under, probe, which_on_path())
    if not found and step != "A0":
        problems.append(f"{step}: no node file found at all -- the controller (Playwright) is expected under ~{CTL_USER}; an empty enumeration means the walk did not run")
    tmp_ok, tmp_detail = (True, "not checked")
    if app_tmp is not None:
        tmp_ok, tmp_detail = (shared_tmp_windows if os.name == "nt" else shared_tmp_posix)(app_tmp)
        if not tmp_ok:
            problems.append(f"{step}: shared temp -- {tmp_detail}")
    return {"step": step, "found": [str(f) for f in found], "probes": probes, "app_tmp": {"path": str(app_tmp) if app_tmp else None, "isolated": tmp_ok, "detail": tmp_detail},
            "path_hits": which_on_path(), "problems": problems, "verdict": "fail" if problems else "pass"}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="native_node_boundary.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("probe-one")
    s.add_argument("file")
    s = sub.add_parser("check")
    s.add_argument("--allow-under", nargs="*", default=[])
    s.add_argument("--app-tmp", default=None)
    s.add_argument("--root", nargs="*", default=None)
    s.add_argument("--step", default="A4b", choices=("A0", "A4b", "A5b"))
    s.add_argument("--out", default=None)
    ns = ap.parse_args(argv)
    if ns.cmd == "probe-one":
        rc, detail = probe_one(Path(ns.file))
        print(f"{['DENIED', 'REACHABLE', '?', 'BROKEN'][rc]}: {ns.file}: {detail}")
        return rc
    res = check([Path(a) for a in ns.allow_under], Path(ns.app_tmp) if ns.app_tmp else None, [Path(r) for r in ns.root] if ns.root else None, ns.step)
    text = json.dumps(res, indent=2, sort_keys=True)
    if ns.out:
        Path(ns.out).parent.mkdir(parents=True, exist_ok=True)
        Path(ns.out).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 1 if res["problems"] else 0


if __name__ == "__main__":
    sys.exit(main())
