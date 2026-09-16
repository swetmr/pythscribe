"""wasm_net: the CPython arm (the semantic ground truth).

    python cpython_arm.py <module.ps> <spec.json>

Loads the module source as plain Python (the admitted `@wasm` shape space is a subset
of Python — no PythScribe syntax extensions are generated), drives every call of the
spec, and prints the SAME normalized outcome JSON the node runners print, so the
driver diffs strings:

    int   -> decimal string (exact, any magnitude)
    float -> little-endian IEEE-754 hex ("nan" for any NaN)
    bool  -> "True" / "False"
    None  -> "None"

Normalization is by the DECLARED return / element type (a Python bool under `-> int`
is "1" on every arm); a value of the WRONG Python type is tagged (`float:<bits>`,
`other:<type>`) rather than coerced, so a type-repr divergence stays visible.
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import json
import math
import struct
import sys


def bits(v: float) -> str:
    if math.isnan(v):
        return "nan"
    return struct.pack("<d", v).hex()


def norm(v, ty: str) -> str:
    if ty == "int":
        if isinstance(v, bool):
            return "1" if v else "0"
        if isinstance(v, int):
            return str(v)
        if isinstance(v, float):
            return "float:" + bits(v)
        return "other:" + type(v).__name__
    if ty == "float":
        if isinstance(v, (bool, int, float)):
            return bits(float(v))
        return "other:" + type(v).__name__
    if ty == "bool":
        # Only a real bool is "True"/"False": an int/float returned under `-> bool` is
        # TAGGED (CPython returns the value itself; a compiled boundary that coerces it
        # must be visible, not normalized away — opus r1/SF4).
        if isinstance(v, bool):
            return "True" if v else "False"
        if isinstance(v, int):
            return "int:" + str(v)
        if isinstance(v, float):
            return "float:" + bits(v)
        return "other:" + type(v).__name__
    if ty == "None":
        # Under `-> None` a returned value is a divergence; its KIND is tagged with the
        # same vocabulary the JS arm uses (list / int / float / bool / other).
        if v is None:
            return "None"
        kind = ("list" if isinstance(v, list) else "bool" if isinstance(v, bool) else "int" if isinstance(v, int)
                else "float" if isinstance(v, float) else "other")
        return "nonnull:" + kind
    raise ValueError(f"unknown declared type {ty}")


def elem_type(list_ty: str) -> str:
    return list_ty[len("list["):-1]


def decode_arg(v, ty: str):
    if ty == "int":
        return int(v)
    if ty == "float":
        if isinstance(v, str):
            # `float("nan")` is a FRESH object each time (never the `math.nan` singleton):
            # CPython's container comparison short-cuts on identity (`x is y or x == y`),
            # and identity is not a value property the compiled arms can express — the
            # oracle must speak value semantics (two distinct NaNs are unequal).
            return {"inf": math.inf, "-inf": -math.inf, "nan": float("nan"), "-0.0": -0.0}[v]
        return float(v)
    if ty == "bool":
        return bool(v)
    if ty.startswith("list["):
        et = elem_type(ty)
        return [decode_arg(e, et) for e in v]
    raise ValueError(f"unknown param type {ty}")


def main() -> None:
    src, spec_path = sys.argv[1], sys.argv[2]
    with open(spec_path, encoding="utf-8") as fh:
        spec = json.load(fh)
    loader = importlib.machinery.SourceFileLoader("wasm_net_module", src)
    mod_spec = importlib.util.spec_from_loader("wasm_net_module", loader)
    mod = importlib.util.module_from_spec(mod_spec)
    loader.exec_module(mod)

    out = {}
    for name, f in spec["functions"].items():
        fn = getattr(mod, name)
        results = []
        for args in f["calls"]:
            py_args = [decode_arg(a, ty) for a, ty in zip(args, f["params"])]
            for src, dst in f.get("alias", []):
                py_args[dst] = py_args[src]  # the same list object (identity semantics)
            try:
                r = fn(*py_args)
            except Exception as e:  # noqa: BLE001 — the exception KIND is the datum
                results.append({"kind": "err", "exc": type(e).__name__})
                continue
            lists = {}
            for i, ty in enumerate(f["params"]):
                if ty.startswith("list["):
                    et = elem_type(ty)
                    lists[str(i)] = [norm(e, et) for e in py_args[i]]
            results.append({"kind": "ok", "value": norm(r, f["ret"]), "lists": lists})
        out[name] = results
    sys.stdout.write(json.dumps(out))


if __name__ == "__main__":
    main()
