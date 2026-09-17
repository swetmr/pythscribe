"""Test-side helpers for the M2.1 WASM ABI controls (validation §G-ABI): rewrite / strip /
append the `pyths.abi` custom section of a module, and re-hash an artifact after editing its
`.wasm` or manifest. The section walk is the runtime's own (`pythscribe.runtime.abi`), so a
helper cannot drift from what the runtime reads."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from pythscribe._pin import COMPILER_VERSION
from pythscribe.artifacts import MANIFEST_NAME, manifest_self_hash, sha256_file
from pythscribe.runtime import abi


def _sections(data: bytes) -> list[tuple[int, int, int, int]]:
    """[(section_id, start_of_section, payload_start, end)] over the whole module (raises on a
    malformed container, exactly like the runtime's walk)."""
    assert data[:4] == b"\x00asm", "not a wasm module"
    out = []
    i = 8
    while i < len(data):
        start = i
        sid = data[i]
        i += 1
        size, i = abi._read_leb128_u32(data, i)
        end = i + size
        assert end <= len(data), "truncated section"
        out.append((sid, start, i, end))
        i = end
    return out


def _is_abi_section(data: bytes, sid: int, payload_start: int, end: int) -> bool:
    if sid != 0:
        return False
    name_len, p = abi._read_leb128_u32(data, payload_start)
    return data[p : p + name_len] == abi.ABI_SECTION_NAME.encode()


def strip_abi_section(data: bytes) -> bytes:
    out = bytearray(data[:8])
    for sid, start, ps, end in _sections(data):
        if not _is_abi_section(data, sid, ps, end):
            out += data[start:end]
    return bytes(out)


def rewrite_abi_section(data: bytes, **fields: Any) -> bytes:
    """Replace the module's `pyths.abi` payload IN PLACE (same position) with the parsed JSON
    updated by `fields` (a field set to `None` is DELETED)."""
    got = abi.read_abi_section(data)
    for k, v in fields.items():
        if v is None:
            got.pop(k, None)
        else:
            got[k] = v
    new = abi.encode_custom_section(abi.ABI_SECTION_NAME, json.dumps(got, separators=(",", ":")).encode())
    out = bytearray(data[:8])
    replaced = False
    for sid, start, ps, end in _sections(data):
        if _is_abi_section(data, sid, ps, end):
            assert not replaced
            out += new
            replaced = True
        else:
            out += data[start:end]
    assert replaced, "module had no pyths.abi section to rewrite"
    return bytes(out)


def stamp_abi_section(data: bytes, **overrides: Any) -> bytes:
    """Append a section carrying THIS runtime's contract (for hand-built modules that must reach
    the checks that come after the ABI gate, e.g. the sandbox import check)."""
    payload = {**abi.expected_contract(), "compiler": COMPILER_VERSION, "history": [], **overrides}
    return bytes(data) + abi.encode_custom_section(abi.ABI_SECTION_NAME, json.dumps(payload, separators=(",", ":")).encode())


def abi_global_wat(major: int | None = None) -> str:
    """WAT for the exported immutable i32 `__pyths_abi` global (B7). The ABI gate requires BOTH a
    `pyths.abi` section AND this global INSIDE the module (check_abi_global_export); a trailing
    custom section alone is necessary but NOT sufficient. Inject it so a hand-built probe passes the
    gate and is refused (if at all) by the check it TARGETS -- sandbox / import -- not the ABI gate."""
    return f'(global (export "{abi.ABI_GLOBAL_EXPORT}") i32 (i32.const {abi.SUPPORTED_ABI_MAJOR if major is None else major}))'


def stamped_probe_from_wat(wat: str, **overrides: Any) -> bytes:
    """wat2wasm a hand-built probe WITH the `__pyths_abi` global injected, then append the pyths.abi
    section -- the FULL B7 contract. `overrides` patch the section fields (e.g. abi=major+1).
    The global goes at the END of the module body: WAT requires all `(import ...)` to precede any
    non-import definition, so injecting after `(module` (before the imports) is a syntax error."""
    import wasmtime

    w = wat.rstrip()
    assert w.endswith(")"), "WAT must end with the module's closing paren"
    return stamp_abi_section(wasmtime.wat2wasm(w[:-1] + " " + abi_global_wat() + ")"), **overrides)


def _sleb128(v: int) -> bytes:
    """Minimal signed-LEB128 encoder (small ints; the inverse of abi._read_sleb128)."""
    out = bytearray()
    while True:
        byte = v & 0x7F
        v >>= 7
        if (v == 0 and not (byte & 0x40)) or (v == -1 and (byte & 0x40)):
            out.append(byte)
            return bytes(out)
        out.append(byte | 0x80)


def module_with_unreachable_start(*, abi_global: str = "ok", **overrides: Any) -> bytes:
    """A VALID hand-built module whose START function is a single `unreachable` -- instantiating it
    traps (`RuntimeError: unreachable` in JS, WasmTrap under wasmtime) while merely COMPILING it does
    not. Stamped with THIS runtime's contract unless `overrides` patch a section field (e.g.
    `abi=SUPPORTED_ABI_MAJOR + 1`). The §G-ABI-5 witness: a loader that instantiates BEFORE the ABI
    check runs the start (RED); check-first never does.

    `abi_global` shapes the exported `__pyths_abi` global (B7 controls), all producing a VALID module:
      "ok"          immutable i32 == SUPPORTED_ABI_MAJOR, exported as a global (the real contract)
      "missing"     no `__pyths_abi` global/export at all
      "mutable"     a MUTABLE i32 == major exported as `__pyths_abi`
      "wrong_type"  an immutable i64 == major exported as `__pyths_abi`
      "wrong_value" an immutable i32 == major+1 exported as `__pyths_abi`
      "func_kind"   `__pyths_abi` exported as a FUNCTION (kind 0), not a global
    """
    from pythscribe.runtime.abi import SUPPORTED_ABI_MAJOR

    type_sec = b"\x01" + b"\x60\x00\x00"  # 1 functype: () -> ()
    func_sec = b"\x01" + b"\x00"  # 1 function of type 0
    start_sec = b"\x00"  # start = function 0
    body = b"\x00" + b"\x00" + b"\x0b"  # 0 locals; `unreachable`; `end`
    code_sec = b"\x01" + abi._encode_leb128_u32(len(body)) + body

    def sec(sid: int, payload: bytes) -> bytes:
        return bytes([sid]) + abi._encode_leb128_u32(len(payload)) + payload

    name = abi.ABI_GLOBAL_EXPORT.encode("utf-8")
    name_vec = abi._encode_leb128_u32(len(name)) + name
    global_sec = b""
    export_sec = b""
    if abi_global == "missing":
        pass
    elif abi_global == "func_kind":
        # export __pyths_abi as function 0 (kind 0x00) -- a wrong-kind export, still a valid module
        export_sec = sec(7, b"\x01" + name_vec + b"\x00" + b"\x00")
    else:
        if abi_global == "ok":
            valtype, mut, initop, val = b"\x7f", b"\x00", b"\x41", SUPPORTED_ABI_MAJOR
        elif abi_global == "mutable":
            valtype, mut, initop, val = b"\x7f", b"\x01", b"\x41", SUPPORTED_ABI_MAJOR
        elif abi_global == "wrong_type":
            valtype, mut, initop, val = b"\x7e", b"\x00", b"\x42", SUPPORTED_ABI_MAJOR  # i64
        elif abi_global == "wrong_value":
            valtype, mut, initop, val = b"\x7f", b"\x00", b"\x41", SUPPORTED_ABI_MAJOR + 1
        else:  # pragma: no cover - test misuse
            raise ValueError(f"unknown abi_global knob {abi_global!r}")
        one_global = valtype + mut + initop + _sleb128(val) + b"\x0b"
        global_sec = sec(6, b"\x01" + one_global)  # 1 defined global at index 0
        export_sec = sec(7, b"\x01" + name_vec + b"\x03" + b"\x00")  # export global 0 as __pyths_abi

    module = (
        b"\x00asm\x01\x00\x00\x00"
        + sec(1, type_sec)
        + sec(3, func_sec)
        + global_sec
        + export_sec
        + sec(8, start_sec)
        + sec(10, code_sec)
    )
    return stamp_abi_section(module, **overrides)


def rewrite_artifact(adir: Path, *, wasm_bytes: bytes | None = None, manifest_edit=None) -> None:
    """Edit an artifact's `.wasm` and/or manifest and RE-SELF-HASH so `verify()` still passes
    (the ABI / range classification is what the test then observes, not an integrity failure)."""
    adir = Path(adir)
    mpath = adir / MANIFEST_NAME
    manifest = json.loads(mpath.read_text(encoding="utf-8"))
    if wasm_bytes is not None:
        wpath = adir / manifest["wasm"]
        wpath.write_bytes(wasm_bytes)
        manifest["files"][manifest["wasm"]] = sha256_file(wpath)
    if manifest_edit is not None:
        manifest_edit(manifest)
    manifest["manifest_sha256"] = manifest_self_hash(manifest)
    mpath.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
