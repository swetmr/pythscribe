"""pythscribe.runtime.abi -- the ONE runtime authority for the WASM ABI loud-fail contract
(spec 13-09-26-lib-selfcontained-pip-wheel, requirements §5.6; plan §M2.1).

A pip runtime that loads a module emitted by a DIFFERENT compiler must fail LOUD at load, never
silently misread linear memory at the wrong layout. Two layers, one authority (this module):

  Layer 1 -- the in-module `pyths.abi` custom section. The compiler (the SINGLE emitted source,
  `crates/pyths_codegen_wasm/src/abi.rs`) stamps every module with
  `{"abi": <major>, "list_layout": ..., "array_layout": ..., "compiler": ..., "history": [...]}`
  and exports the immutable i32 global `__pyths_abi`. `check_module()` reads the section with a
  dependency-free LEB128 section walk and compares the major AND BOTH layout strings -- field by
  field, none informational -- against this runtime's DECLARED MIRRORS (`SUPPORTED_ABI_MAJOR`
  here; the layout strings at their K7 homes `runtime.LAYOUT_VERSION` /
  `array_buffer.ARRAY_LAYOUT_VERSION`, byte-identical to the three `list_buffer.mjs` copies). A
  mismatch raises `AbiMismatchError` naming the FIELD and both sides, before any export is called.
  The mirrors are bound to the Rust source by the shipped-path drift gate
  `tests/pythscribe/test_abi_binding.py` (+ `abi_golden.json` pinning `ABI_HISTORY`).

  Layer 2 -- the manifest compiler range. `ACCEPTED_COMPILER_RANGE` (`_pin.py`) is the range of
  compiler versions this runtime loads (`COMPILER_VERSION` stays the exact BUILD pin);
  `artifacts.resolve()` classifies an out-of-range `manifest.compiler.version` as `unusable`.

Neither layer consults the ambient compiler / optimizer (not a freshness check; M3 separation).
"""
from __future__ import annotations

import json
import re
from typing import Any

from .._pin import ACCEPTED_COMPILER_RANGE

__all__ = [
    "ABI_SECTION_NAME",
    "ABI_GLOBAL_EXPORT",
    "SUPPORTED_ABI_MAJOR",
    "ACCEPTED_COMPILER_RANGE",
    "AbiMismatchError",
    "expected_layouts",
    "expected_contract",
    "read_abi_section",
    "find_custom_sections",
    "encode_custom_section",
    "check_abi_global_export",
    "check_module",
    "module_problem",
    "parse_version",
    "version_in_range",
    "compiler_version_problem",
]

ABI_SECTION_NAME = "pyths.abi"
ABI_GLOBAL_EXPORT = "__pyths_abi"
# The ABI major this runtime speaks. Mirrors `abi.rs::PYTHS_ABI_MAJOR` (bound by the drift gate).
SUPPORTED_ABI_MAJOR = 1

# The section fields the runtime COMPARES (each independently; `compiler`/`history` are recorded
# provenance, not contract). A mutant that drops one is caught by the per-field patched-module
# controls in tests/pythscribe/test_wheel_m2.py (validation §G-ABI-1 / §G-ABI-3 (v)(vi)).
CONTRACT_FIELDS = ("abi", "list_layout", "array_layout")


class AbiMismatchError(RuntimeError):
    """The module's `pyths.abi` contract disagrees with this runtime (or is absent/unreadable).
    `field` names the first disagreeing field (`abi` | `list_layout` | `array_layout` |
    `section`); `expected` / `actual` carry both sides."""

    def __init__(self, message: str, *, field: str, expected: Any = None, actual: Any = None):
        super().__init__(message)
        self.field = field
        self.expected = expected
        self.actual = actual


def expected_layouts() -> tuple[str, str]:
    """(list_layout, array_layout) from their K7 homes (unchanged): `runtime.LAYOUT_VERSION` and
    `array_buffer.ARRAY_LAYOUT_VERSION`. Imported lazily -- `pythscribe.runtime` imports this module."""
    from . import LAYOUT_VERSION
    from .array_buffer import ARRAY_LAYOUT_VERSION

    return LAYOUT_VERSION, ARRAY_LAYOUT_VERSION


def expected_contract() -> dict[str, Any]:
    lst, arr = expected_layouts()
    return {"abi": SUPPORTED_ABI_MAJOR, "list_layout": lst, "array_layout": arr}


# ----------------------------------------------------------------------------- section walk
def _read_leb128_u32(data: bytes, pos: int) -> tuple[int, int]:
    """(value, new_pos). Raises ValueError on a truncated / oversized varuint."""
    result = 0
    shift = 0
    for i in range(5):
        if pos + i >= len(data):
            raise ValueError("truncated LEB128")
        b = data[pos + i]
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, pos + i + 1
        shift += 7
    raise ValueError("LEB128 too long for u32")


def _encode_leb128_u32(v: int) -> bytes:
    out = bytearray()
    while True:
        byte = v & 0x7F
        v >>= 7
        if v == 0:
            out.append(byte)
            return bytes(out)
        out.append(byte | 0x80)


def find_custom_sections(wasm_bytes: bytes, name: str) -> list[bytes]:
    """Payloads (after the name) of every custom section called `name`, in order. Raises
    ValueError when the bytes are not a well-formed WebAssembly section stream (bad magic,
    truncated section) -- the caller decides how to report that."""
    data = bytes(wasm_bytes)
    if len(data) < 8 or data[:4] != b"\x00asm":
        raise ValueError("not a WebAssembly module (bad magic)")
    want = name.encode("utf-8")
    found: list[bytes] = []
    i = 8
    while i < len(data):
        sid = data[i]
        i += 1
        size, i = _read_leb128_u32(data, i)
        end = i + size
        if end > len(data):
            raise ValueError(f"truncated section id={sid}")
        if sid == 0:
            name_len, p = _read_leb128_u32(data, i)
            name_end = p + name_len
            if name_end > end:
                raise ValueError("custom section name overruns the section")
            if data[p:name_end] == want:
                found.append(data[name_end:end])
        i = end
    return found


def encode_custom_section(name: str, payload: bytes) -> bytes:
    """One encoded custom section (id 0 + size + name + payload) -- the inverse of the walk.
    Used by the test fixtures (a hand-built module needs a valid section to reach the sandbox
    check) and by the M3 optimizer pass to re-append a stripped section into its candidate."""
    n = name.encode("utf-8")
    body = _encode_leb128_u32(len(n)) + n + bytes(payload)
    return b"\x00" + _encode_leb128_u32(len(body)) + body


# ----------------------------------------------------------------- export/global walk (B7)
# The `pyths.abi` section is provenance the caller can forge without touching the module body;
# the exported immutable i32 global `__pyths_abi` (emit.rs) is the load-bearing half -- a module
# that misreads memory at the wrong layout also carries the wrong global. `check_module` requires
# BOTH: a matching section AND exactly one DEFINED, immutable i32 `__pyths_abi` export whose
# constant value == the runtime major. All of this is a dependency-free byte walk that runs BEFORE
# wasmtime compiles/instantiates anything (ServerKernel), so a stripped/mutated global fails LOUD.
#
# WASM section ids: 2=import 6=global 7=export. valtype i32=0x7F. globaltype mut: 0x00=const 0x01=var.
# exportdesc kind: 0x00=func 0x01=table 0x02=mem 0x03=global.

_VALTYPE_I32 = 0x7F
_MUT_CONST = 0x00
_EXPORT_KIND_GLOBAL = 0x03

# B7 (codex pass-3): the WASM binary-format section id -> (name, canonical rank). Every NON-custom
# section (id != 0) may appear AT MOST ONCE and the non-custom sequence must be strictly increasing by
# rank (the spec's canonical order: type, import, function, table, memory, global, export, start,
# element, datacount, code, data). Custom sections (id 0) may appear anywhere and are exempt. A module
# with a duplicate/mis-ordered/invalid-id non-custom section is malformed -- before this, a last-write-
# wins parse could smuggle a second `__pyths_abi` past a duplicate global/export section, and duplicate
# type/function/start/code sections passed check_module (wasmtime rejected them only on later compile).
_SECTION_INFO: dict[int, tuple[str, int]] = {
    1: ("type", 0), 2: ("import", 1), 3: ("function", 2), 4: ("table", 3), 5: ("memory", 4),
    6: ("global", 5), 7: ("export", 6), 8: ("start", 7), 9: ("element", 8), 12: ("datacount", 9),
    10: ("code", 10), 11: ("data", 11),
}


def _section_structure_problem(sid: int, seen: dict[int, int], last_rank: int) -> tuple[str | None, int]:
    """B7: validate one NON-custom section id against the sections already seen. Returns (error or None,
    new last_rank). The caller has already excluded custom sections (id 0)."""
    if sid not in _SECTION_INFO:
        return f"module carries a section with invalid id {sid} (not a WASM binary-format section id 0-12)", last_rank
    name, rank = _SECTION_INFO[sid]
    seen[sid] = seen.get(sid, 0) + 1
    if seen[sid] > 1:
        return f"module carries {seen[sid]} sections with id {sid} (duplicate {name} section; malformed)", last_rank
    if rank <= last_rank:
        return f"module `{name}` section (id {sid}) is out of canonical order (a section of rank {rank} follows rank {last_rank}); malformed", last_rank
    return None, rank


def _read_sleb128(data: bytes, pos: int) -> tuple[int, int]:
    """(value, new_pos) for a signed LEB128 (the i32/i64.const operand). Raises ValueError."""
    result = 0
    shift = 0
    while True:
        if pos >= len(data):
            raise ValueError("truncated SLEB128")
        b = data[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        shift += 7
        if not (b & 0x80):
            if b & 0x40 and shift < 64:
                result |= -(1 << shift)
            return result, pos
        if shift > 70:
            raise ValueError("SLEB128 too long")


def _read_name_at(data: bytes, pos: int) -> tuple[bytes, int]:
    n, pos = _read_leb128_u32(data, pos)
    end = pos + n
    if end > len(data):
        raise ValueError("name overruns module")
    return data[pos:end], end


def _iter_sections(data: bytes):
    """Yield (section_id, payload_start, payload_end) over a well-formed module (same container
    checks as `find_custom_sections`). Raises ValueError on a malformed container. B7: the binary
    version word (bytes 4..8) must be exactly 1 -- a module stamped with another WASM version is not
    one this walk can read, and is refused (never parsed at a guessed layout)."""
    if len(data) < 8 or data[:4] != b"\x00asm":
        raise ValueError("not a WebAssembly module (bad magic)")
    if data[4:8] != b"\x01\x00\x00\x00":
        raise ValueError(f"unsupported WebAssembly version {int.from_bytes(data[4:8], 'little')} (expected 1)")
    i = 8
    while i < len(data):
        sid = data[i]
        i += 1
        size, i = _read_leb128_u32(data, i)
        end = i + size
        if end > len(data):
            raise ValueError(f"truncated section id={sid}")
        yield sid, i, end
        i = end


def _skip_limits(data: bytes, i: int) -> int:
    flags = data[i]
    i += 1
    _, i = _read_leb128_u32(data, i)  # min
    if flags & 0x01:
        _, i = _read_leb128_u32(data, i)  # max
    return i


def _count_imported_globals(data: bytes, ps: int, end: int) -> int:
    """Imported globals occupy the low global-index space; the export index is offset by them."""
    n, i = _read_leb128_u32(data, ps)
    count = 0
    for _ in range(n):
        _, i = _read_name_at(data, i)  # module
        _, i = _read_name_at(data, i)  # field
        if i >= end:
            raise ValueError("import overruns section")
        kind = data[i]
        i += 1
        if kind == 0x00:  # func: typeidx
            _, i = _read_leb128_u32(data, i)
        elif kind == 0x01:  # table: reftype + limits
            i += 1
            i = _skip_limits(data, i)
        elif kind == 0x02:  # mem: limits
            i = _skip_limits(data, i)
        elif kind == 0x03:  # global: valtype + mut
            i += 2
            count += 1
        else:
            raise ValueError(f"unknown import kind 0x{kind:02x}")
        if i > end:
            raise ValueError("import overruns section")
    if i != end:
        raise ValueError(f"import section has {end - i} trailing byte(s) after its declared entries (malformed)")
    return count


def _read_const_expr(data: bytes, pos: int) -> tuple[int | None, int]:
    """Read one constant expression up to and including its 0x0B end. Returns (int value for an
    i32/i64.const, else None, new_pos). Raises ValueError on an unsupported opcode / truncation."""
    value: int | None = None
    while True:
        if pos >= len(data):
            raise ValueError("truncated const expr")
        op = data[pos]
        pos += 1
        if op == 0x0B:  # end
            return value, pos
        if op in (0x41, 0x42):  # i32.const / i64.const
            value, pos = _read_sleb128(data, pos)
        elif op == 0x43:  # f32.const
            pos += 4
            value = None
        elif op == 0x44:  # f64.const
            pos += 8
            value = None
        elif op == 0x23:  # global.get
            _, pos = _read_leb128_u32(data, pos)
            value = None
        elif op == 0xD0:  # ref.null
            pos += 1
            value = None
        elif op == 0xD2:  # ref.func
            _, pos = _read_leb128_u32(data, pos)
            value = None
        else:
            raise ValueError(f"unsupported const-expr opcode 0x{op:02x}")
        if pos > len(data):
            raise ValueError("const expr overruns module")


def _parse_defined_globals(data: bytes, ps: int, end: int) -> list[tuple[int, int, int | None]]:
    """[(valtype, mut, const_value_or_None)] for each DEFINED global, in index order."""
    n, i = _read_leb128_u32(data, ps)
    out: list[tuple[int, int, int | None]] = []
    for _ in range(n):
        if i + 2 > end:
            raise ValueError("global type overruns section")
        valtype = data[i]
        mut = data[i + 1]
        i += 2
        value, i = _read_const_expr(data, i)
        if i > end:
            raise ValueError("global overruns section")
        out.append((valtype, mut, value))
    if i != end:
        raise ValueError(f"global section has {end - i} trailing byte(s) after its declared entries (malformed)")
    return out


def _parse_exports(data: bytes, ps: int, end: int) -> list[tuple[bytes, int, int]]:
    """[(name, kind, index)] for each export."""
    n, i = _read_leb128_u32(data, ps)
    out: list[tuple[bytes, int, int]] = []
    for _ in range(n):
        name, i = _read_name_at(data, i)
        if i >= end:
            raise ValueError("export overruns section")
        kind = data[i]
        i += 1
        idx, i = _read_leb128_u32(data, i)
        if i > end:
            raise ValueError("export overruns section")
        out.append((name, kind, idx))
    if i != end:
        raise ValueError(f"export section has {end - i} trailing byte(s) after its declared entries (malformed)")
    return out


def check_abi_global_export(wasm_bytes: bytes, *, name: str = "<wasm>", major: int = SUPPORTED_ABI_MAJOR) -> int:
    """B7: require exactly ONE defined, immutable, i32 `__pyths_abi` export whose constant value
    equals `major`. Returns that value. Raises AbiMismatchError(field='abi_global') naming the
    violation (missing / duplicated / wrong-kind / imported-not-defined / wrong-type / mutable /
    wrong-value), or ValueError for a malformed module container (the caller lets wasmtime diagnose
    garbage bytes). Runs BEFORE compilation/instantiation."""
    imported_globals = 0
    defined_globals: list[tuple[int, int, int | None]] = []
    exports: list[tuple[bytes, int, int]] = []
    seen: dict[int, int] = {}
    last_rank = -1
    for sid, ps, end in _iter_sections(wasm_bytes):
        # B7 (codex pass-3): full structural validation of the section stream -- EVERY non-custom
        # section id valid, unique, and in canonical order (not just import/global/export). A duplicate
        # global/export could smuggle a second `__pyths_abi` past a last-write-wins parse; a duplicate
        # type/function/start/code (or a mis-ordered / invalid-id section) is malformed and refused here,
        # BEFORE wasmtime ever compiles it -- check_module no longer accepts bytes wasmtime later rejects.
        if sid != 0:
            problem, last_rank = _section_structure_problem(sid, seen, last_rank)
            if problem:
                raise ValueError(problem)
        if sid == 2:
            imported_globals = _count_imported_globals(wasm_bytes, ps, end)
        elif sid == 6:
            defined_globals = _parse_defined_globals(wasm_bytes, ps, end)
        elif sid == 7:
            exports = _parse_exports(wasm_bytes, ps, end)
    want = ABI_GLOBAL_EXPORT.encode("utf-8")
    hits = [(kind, idx) for (nm, kind, idx) in exports if nm == want]
    if not hits:
        raise AbiMismatchError(
            f"{name}: module exports no immutable i32 `{ABI_GLOBAL_EXPORT}` global -- the ABI-major "
            "export the runtime binds is absent (stripped by a transform, or never emitted); refused "
            "(never silently). Rebuild the artifact with the pinned compiler",
            field="abi_global", expected=major, actual=None,
        )
    if len(hits) > 1:
        raise AbiMismatchError(
            f"{name}: module carries {len(hits)} `{ABI_GLOBAL_EXPORT}` exports (ambiguous; expected exactly one)",
            field="abi_global", expected=1, actual=len(hits),
        )
    kind, gidx = hits[0]
    if kind != _EXPORT_KIND_GLOBAL:
        raise AbiMismatchError(
            f"{name}: `{ABI_GLOBAL_EXPORT}` is exported with kind {kind} (expected a global, kind {_EXPORT_KIND_GLOBAL})",
            field="abi_global", expected=_EXPORT_KIND_GLOBAL, actual=kind,
        )
    if gidx < imported_globals:
        raise AbiMismatchError(
            f"{name}: `{ABI_GLOBAL_EXPORT}` names an IMPORTED global (index {gidx} < {imported_globals}), "
            "not a defined one -- an importer could supply any value; refused",
            field="abi_global", expected="defined", actual="imported",
        )
    local = gidx - imported_globals
    if local >= len(defined_globals):
        raise ValueError(f"`{ABI_GLOBAL_EXPORT}` export references global {gidx} but only {len(defined_globals)} are defined")
    valtype, mut, value = defined_globals[local]
    if valtype != _VALTYPE_I32:
        raise AbiMismatchError(
            f"{name}: `{ABI_GLOBAL_EXPORT}` global has valtype 0x{valtype:02x}, not i32 (0x{_VALTYPE_I32:02x})",
            field="abi_global", expected="i32", actual=f"0x{valtype:02x}",
        )
    if mut != _MUT_CONST:
        raise AbiMismatchError(
            f"{name}: `{ABI_GLOBAL_EXPORT}` global is mutable -- the ABI major must be immutable (a mutable "
            "global could be rewritten after the check); refused",
            field="abi_global", expected="immutable", actual="mutable",
        )
    if value != major:
        raise AbiMismatchError(
            f"{name}: `{ABI_GLOBAL_EXPORT}` global value is {value!r}, not the runtime ABI major {major!r} -- "
            "the module was emitted by a compiler with a different ABI; rebuild the artifact",
            field="abi_global", expected=major, actual=value,
        )
    return value


def read_abi_section(wasm_bytes: bytes) -> dict[str, Any]:
    """The parsed `pyths.abi` contract. Raises AbiMismatchError(field='section') when the module
    carries no such section, more than one (ambiguous -- a forged append), a non-JSON payload, or
    a payload missing a contract field; raises ValueError for a malformed module container."""
    secs = find_custom_sections(wasm_bytes, ABI_SECTION_NAME)
    if not secs:
        raise AbiMismatchError(
            f"module carries no `{ABI_SECTION_NAME}` custom section: it was not emitted by a pyths "
            f"compiler this runtime can load (accepted ABI major {SUPPORTED_ABI_MAJOR}, compiler range "
            f"{ACCEPTED_COMPILER_RANGE}); rebuild the artifact (`pyths build --force <module.py>`)",
            field="section", expected=SUPPORTED_ABI_MAJOR, actual=None,
        )
    if len(secs) > 1:
        raise AbiMismatchError(
            f"module carries {len(secs)} `{ABI_SECTION_NAME}` sections (ambiguous; expected exactly one)",
            field="section", expected=1, actual=len(secs),
        )
    try:
        got = json.loads(secs[0].decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as e:
        raise AbiMismatchError(f"`{ABI_SECTION_NAME}` section is not valid JSON ({e})", field="section") from e
    if not isinstance(got, dict):
        raise AbiMismatchError(f"`{ABI_SECTION_NAME}` section is not a JSON object", field="section")
    for f in CONTRACT_FIELDS:
        if f not in got:
            raise AbiMismatchError(f"`{ABI_SECTION_NAME}` section lacks the `{f}` field", field="section", expected=f)
    return got


def check_module(wasm_bytes: bytes, *, name: str = "<wasm>") -> dict[str, Any]:
    """Layer 1: compare the module's section against this runtime -- major AND both layout
    strings, each independently. Returns the parsed section; raises AbiMismatchError naming the
    first mismatching FIELD and both sides. Raises ValueError for a malformed module container
    (the caller lets wasmtime diagnose garbage bytes first)."""
    got = read_abi_section(wasm_bytes)
    want = expected_contract()
    # NOTE: three explicit, independent comparisons -- NOT a dict equality -- so each field has its
    # own absent-comparison mutant (validation §G-ABI-3 (v)/(vi)) and its own patched-module control.
    # B7: strict integer typing FIRST -- JSON `true`/`1.0` must NOT compare equal to the int major
    # (`True == 1` / `1.0 == 1` are Python-true). `type(x) is int` rejects bool (its type is bool)
    # and float; only a genuine int reaches the value comparison.
    if type(got["abi"]) is not int:
        raise AbiMismatchError(
            f"{name}: `{ABI_SECTION_NAME}` `abi` (major) is {got['abi']!r} ({type(got['abi']).__name__}), "
            f"not an integer -- refused (a boolean/float must not masquerade as the ABI major)",
            field="abi", expected=want["abi"], actual=got["abi"],
        )
    if got["abi"] != want["abi"]:
        raise AbiMismatchError(
            f"{name}: WASM ABI mismatch on `abi` (major): module={got['abi']!r} runtime={want['abi']!r} -- "
            "the module was emitted by a compiler with a different ABI; rebuild the artifact with the pinned compiler",
            field="abi", expected=want["abi"], actual=got["abi"],
        )
    if got["list_layout"] != want["list_layout"]:
        raise AbiMismatchError(
            f"{name}: WASM ABI mismatch on `list_layout`: module={got['list_layout']!r} runtime={want['list_layout']!r} -- "
            "a list buffer would be read at the wrong layout; refused (never silently)",
            field="list_layout", expected=want["list_layout"], actual=got["list_layout"],
        )
    if got["array_layout"] != want["array_layout"]:
        raise AbiMismatchError(
            f"{name}: WASM ABI mismatch on `array_layout`: module={got['array_layout']!r} runtime={want['array_layout']!r} -- "
            "a typed-array header would be read at the wrong layout; refused (never silently)",
            field="array_layout", expected=want["array_layout"], actual=got["array_layout"],
        )
    # B7: the section is provenance; the exported immutable i32 `__pyths_abi` global is the
    # load-bearing half. Require exactly one defined immutable i32 export == the major, BEFORE any
    # compilation/instantiation. A module with a matching section but a missing / mutable / wrong-kind
    # / wrong-value global fails LOUD here (never silently misreads at a foreign layout).
    check_abi_global_export(wasm_bytes, name=name, major=want["abi"])
    return got


def module_problem(wasm_bytes: bytes, *, name: str = "<wasm>") -> str | None:
    """`check_module` as a predicate: None when the module is loadable, else the reason."""
    try:
        check_module(wasm_bytes, name=name)
    except (AbiMismatchError, ValueError) as e:
        return str(e)
    return None


# ----------------------------------------------------------------------------- Layer 2: range
_VERSION = re.compile(r"^\s*v?(\d+)(?:\.(\d+))?(?:\.(\d+))?")
_CLAUSE = re.compile(r"^\s*(>=|<=|==|>|<|!=)\s*v?(\d+(?:\.\d+){0,2})\s*$")


def parse_version(s: Any) -> tuple[int, int, int] | None:
    """`"0.2.4"` -> (0, 2, 4); a pre-release tag (`0.2.5a0`) keeps its numeric prefix. Non-string /
    unparsable -> None (an out-of-range answer, never a guess)."""
    if not isinstance(s, str):
        return None
    m = _VERSION.match(s)
    if not m:
        return None
    return tuple(int(x or 0) for x in m.groups())  # type: ignore[return-value]


def version_in_range(version: Any, spec: str = ACCEPTED_COMPILER_RANGE) -> bool:
    """Dependency-free PEP-440-style range check for the clauses `_pin.py` uses (`>=`, `<`, and the
    other comparison operators), comma-separated. An unparsable version is OUT of range."""
    v = parse_version(version)
    if v is None:
        return False
    for raw in spec.split(","):
        m = _CLAUSE.match(raw)
        if not m:
            raise ValueError(f"unsupported range clause {raw!r} in {spec!r}")
        op, bound = m.group(1), parse_version(m.group(2))
        assert bound is not None
        ok = {
            ">=": v >= bound, "<=": v <= bound, "==": v == bound, "!=": v != bound, ">": v > bound, "<": v < bound,
        }[op]
        if not ok:
            return False
    return True


def compiler_version_problem(version: Any) -> str | None:
    """Layer 2 predicate for `artifacts.resolve()`: None when `manifest.compiler.version` is within
    `ACCEPTED_COMPILER_RANGE`, else a message naming the version and the range."""
    if version_in_range(version):
        return None
    return (
        f"artifact was built by pyths {version!r}, outside the compiler range this pythscribe runtime "
        f"loads ({ACCEPTED_COMPILER_RANGE}); rebuild it with the pinned compiler (`pyths build --force <module.py>`)"
    )
