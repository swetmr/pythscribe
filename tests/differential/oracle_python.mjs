// THE CPython differential-oracle resolver — the ONE place every live-CPython
// lane resolves its interpreter (docs/python-oracle-policy.md).
//
// The pinned oracle is CPython 3.14.7. CI installs it via actions/setup-python
// so plain `python` on PATH *is* the oracle there. Locally, machines often
// default `python` to something else — set PYTHS_ORACLE_PYTHON to target the
// pinned interpreter explicitly:
//
//   PYTHS_ORACLE_PYTHON="py -3.14"   (Windows launcher)
//   PYTHS_ORACLE_PYTHON=python3.14   (POSIX)
//
// The value is whitespace-split into argv so launcher forms work. EVERY
// differential-oracle lane that spawns live CPython (the semantic corpus, the
// shared harness behind the fuzz/identifier generators, the i64-boundary and
// Livermore differentials, and the format-spec differential) MUST route through
// this module — benchmark reference-runners under examples/ are NOT oracle lanes
// and are out of scope. A lane
// spawning bare "python" itself silently escapes the pin (the bug this module
// removes: four independent copies, only one of which honored the env var).

/** Resolve the oracle interpreter once. */
const ORACLE_ARGV = (process.env.PYTHS_ORACLE_PYTHON || "python")
    .trim()
    .split(/\s+/);

/** Executable name/path of the oracle CPython (argv[0]). */
export const ORACLE_BIN = ORACLE_ARGV[0];

/** Leading interpreter args (e.g. `["-3.14"]` for the Windows launcher). */
export const ORACLE_PRE_ARGS = ORACLE_ARGV.slice(1);

/** Human-readable form for log/skip messages. */
export const ORACLE_DISPLAY = ORACLE_ARGV.join(" ");

/** Build the full argv tail: interpreter args + the call's own args.
 *  Usage: `spawnSync(ORACLE_BIN, oracleArgs(["-c", code]), opts)`. */
export function oracleArgs(args) {
    return [...ORACLE_PRE_ARGS, ...args];
}
