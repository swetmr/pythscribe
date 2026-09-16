"""The static-readability check for `@wasm` (plan §5: "the decorator must be statically
readable -- no conditional decoration, no aliasing, no dynamic application").

Both the decorator (on a live function object) and the build step (on a source file,
without importing it) go through THIS module, so "what the build compiles" and "what the
runtime accepts" are the same textual predicate. The runtime additionally verifies the
BINDING (the decorator expression must resolve to `pythscribe.decorators.wasm` in the
module's globals) -- the build, working from the AST alone, cannot, so a module that
shadows the name `pythscribe` can make the build emit an artifact the runtime refuses to
bind (never the other way round: the runtime never wraps what the build would not compile).

A function is statically decorated iff, in its source file:
  * there is a `def <name>` at MODULE TOP LEVEL (not under if/for/try/with/def/class),
  * whose first line (decorators included) is the function's `co_firstlineno`,
  * carrying EXACTLY ONE decorator, which is literally `@wasm`, `@wasm(...)`,
    `@pythscribe.wasm` or `@pythscribe.decorators.wasm` (with or without a call),
  * and no other top-level binding of the same name exists in the module (a shadowed
    kernel would leave an artifact for a function the module never exposes).
Everything else -- `@alias`, `f = wasm(f)`, decoration under a condition, a nested def,
stacked decorators, `exec`'d source -- is rejected with StaticDecorationError. A rejected
function is never silently left unrouted: the ERROR is the point.

One deliberate exception (review R1/B4a): if the SOURCE ITSELF is unavailable at import
(a .pyc-only / zipapp / frozen deployment) there is nothing to build and nothing to check;
`locate` raises SourceUnavailable and the decorator falls back to plain Python with a
logged warning instead of breaking the app.
"""
from __future__ import annotations

import ast
import hashlib
import linecache
import textwrap
from dataclasses import dataclass


class StaticDecorationError(TypeError):
    """`@wasm` was applied in a way the build step cannot see."""


class SourceUnavailable(StaticDecorationError):
    """The module's source text cannot be read (sourceless deployment) -- fallback, not error."""


@dataclass(frozen=True)
class Located:
    node: ast.FunctionDef
    module_source: str
    kernel_source: str  # `@wasm` + the def, exactly what the compiler is fed
    source_sha256: str  # sha256 of kernel_source -- the staleness key in the manifest


_ADMITTED_CHAINS = (("pythscribe", "wasm"), ("pythscribe", "decorators", "wasm"))


def _chain(expr: ast.expr) -> tuple[str, ...] | None:
    """`a.b.c` -> ('a','b','c'); `a` -> ('a',); anything else -> None."""
    parts: list[str] = []
    node = expr
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if not isinstance(node, ast.Name):
        return None
    parts.append(node.id)
    return tuple(reversed(parts))


def decorator_head(dec: ast.expr) -> ast.expr:
    return dec.func if isinstance(dec, ast.Call) else dec


def decorator_is_wasm(dec: ast.expr) -> bool:
    chain = _chain(decorator_head(dec))
    return chain == ("wasm",) or chain in _ADMITTED_CHAINS


def first_line(node: ast.FunctionDef | ast.AsyncFunctionDef) -> int:
    """First physical line of the def INCLUDING decorators (== code.co_firstlineno)."""
    if node.decorator_list:
        return min(d.lineno for d in node.decorator_list)
    return node.lineno


def kernel_source(module_source: str, node: ast.FunctionDef) -> str:
    seg = ast.get_source_segment(module_source, node)
    if seg is None:  # pragma: no cover - only for synthetic nodes
        raise StaticDecorationError(f"cannot extract source of `{node.name}`")
    return "@wasm\n" + textwrap.dedent(seg).rstrip() + "\n"


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def signature_texts(node: ast.FunctionDef) -> tuple[tuple[tuple[str, str], ...], str]:
    """((param, annotation text), ...) and the return annotation text of a kernel def --
    `ast.unparse`d, so `list[int]` and `List[int]` keep their spelling; a missing annotation
    is the empty string (the compiler refuses unannotated kernels anyway).

    EVERY parameter is surfaced, in call order: positional-only, positional, then keyword-
    only, with `*args` / `**kwargs` spelled `*name` / `**name` (review opus r1/B3: a
    signature that silently dropped keyword-only params let the FFI shim call a 2-arg
    export with 1 argument -> NaN reported as success). The FFI grammar refuses the
    non-positional forms; it must SEE them to refuse them."""
    a = node.args
    params = [
        (p.arg, ast.unparse(p.annotation) if p.annotation is not None else "")
        for p in a.posonlyargs + a.args
    ]
    if a.vararg is not None:
        params.append((f"*{a.vararg.arg}", ast.unparse(a.vararg.annotation) if a.vararg.annotation is not None else ""))
    for p in a.kwonlyargs:
        params.append((p.arg, ast.unparse(p.annotation) if p.annotation is not None else ""))
    if a.kwarg is not None:
        params.append((f"**{a.kwarg.arg}", ast.unparse(a.kwarg.annotation) if a.kwarg.annotation is not None else ""))
    ret = ast.unparse(node.returns) if node.returns is not None else ""
    return tuple(params), ret


def positional_only_signature(node: ast.FunctionDef) -> bool:
    """True iff every parameter can be supplied positionally -- the shape the list-buffer
    FFI calls with an exact arity check. Positional params WITH defaults qualify (the caller
    passes every argument; the default is simply never used -- codex r2); keyword-only,
    `*args` and `**kwargs` do not."""
    a = node.args
    return not a.vararg and not a.kwonlyargs and not a.kwarg


def validate_node(node: ast.FunctionDef | ast.AsyncFunctionDef, name: str) -> None:
    """Raise unless `node` is a top-level def decorated exactly once with a literal @wasm."""
    if isinstance(node, ast.AsyncFunctionDef):
        raise StaticDecorationError(f"`{name}`: async functions cannot be @wasm")
    decs = node.decorator_list
    if len(decs) == 0:
        raise StaticDecorationError(
            f"`{name}` is not decorated in source: dynamic application (`{name} = wasm({name})`) "
            "is not statically readable. Write a literal `@wasm` above the def."
        )
    if len(decs) != 1:
        raise StaticDecorationError(
            f"`{name}`: @wasm must be the ONLY decorator (found {len(decs)}); stacked decorators "
            "are not statically readable."
        )
    if not decorator_is_wasm(decs[0]):
        raise StaticDecorationError(
            f"`{name}` is decorated with `@{ast.unparse(decs[0])}`, not a literal `@wasm`: aliased "
            "decoration is not statically readable. Write `@wasm` (or `@pythscribe.wasm`)."
        )


def _top_level_bindings(tree: ast.Module) -> dict[str, list[tuple[int, str]]]:
    """Every top-level statement that (re)binds -- or deletes -- a module name -> the
    (line, kind) pairs it does so. Covers def/class/assign/augassign/annassign/import/for/
    with/walrus/del/match captures (R2/S7 closed the class, not the site)."""
    out: dict[str, list[tuple[int, str]]] = {}

    def add(name: str, line: int, kind: str = "bind") -> None:
        out.setdefault(name, []).append((line, kind))

    def targets(t: ast.expr) -> None:
        if isinstance(t, ast.Name):
            add(t.id, t.lineno)
        elif isinstance(t, (ast.Tuple, ast.List)):
            for e in t.elts:
                targets(e)
        elif isinstance(t, ast.Starred):
            targets(t.value)

    def pattern_names(pat: ast.pattern) -> None:
        for node in ast.walk(pat):
            if isinstance(node, (ast.MatchAs, ast.MatchStar)) and node.name:
                add(node.name, node.lineno)
            elif isinstance(node, ast.MatchMapping) and node.rest:
                add(node.rest, node.lineno)

    for stmt in tree.body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            add(stmt.name, stmt.lineno)
        elif isinstance(stmt, ast.Assign):
            for t in stmt.targets:
                targets(t)
        elif isinstance(stmt, (ast.AnnAssign, ast.AugAssign)):
            targets(stmt.target)
        elif isinstance(stmt, (ast.Import, ast.ImportFrom)):
            for a in stmt.names:
                add((a.asname or a.name).split(".")[0], stmt.lineno, "import")
        elif isinstance(stmt, (ast.For, ast.AsyncFor)):
            targets(stmt.target)
        elif isinstance(stmt, (ast.With, ast.AsyncWith)):
            for item in stmt.items:
                if item.optional_vars is not None:
                    targets(item.optional_vars)
        elif isinstance(stmt, ast.Delete):
            for t in stmt.targets:
                targets(t)
        elif isinstance(stmt, ast.Match):
            for case in stmt.cases:
                pattern_names(case.pattern)
        # a top-level walrus anywhere in the statement's expressions (e.g. `print((f := 1))`)
        for node in ast.walk(stmt):
            if isinstance(node, ast.NamedExpr) and isinstance(node.target, ast.Name):
                add(node.target.id, node.lineno)
    return out


def _check_unique_binding(tree: ast.Module, name: str, def_line: int) -> None:
    """The kernel must be the module's ONLY top-level binding of its name. One exemption: an
    `import ... as name` that occurs strictly BEFORE the def is simply replaced by the def
    (the def wins), so it cannot shadow the kernel (R2/S7 false positive)."""
    others = [
        (line, kind) for line, kind in _top_level_bindings(tree).get(name, [])
        if line != def_line and not (kind == "import" and line < def_line)
    ]
    if others:
        lines = sorted(line for line, _ in others)
        raise StaticDecorationError(
            f"`{name}` is bound/deleted again at module top level (lines {lines}, def at {def_line}): a @wasm "
            "kernel must be the module's ONLY binding of its name (a shadowed kernel would leave an artifact "
            "for a function the module never exposes)."
        )


def find_wasm_defs(module_source: str, filename: str = "<module>") -> list[ast.FunctionDef]:
    """Top-level defs carrying a @wasm decorator (the build step's input; no import needed).

    Any def ANYWHERE in the file that carries `@wasm` but is not statically readable
    (nested / conditional / stacked / async / duplicated / shadowed) raises -- the build
    must never silently skip one, nor build one the module does not expose.
    """
    tree = ast.parse(module_source, filename)
    top = [n for n in tree.body if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))]
    top_ids = {id(n) for n in top}
    found: list[ast.FunctionDef] = []
    for node in ast.walk(tree):
        if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        has_wasm = any(decorator_is_wasm(d) for d in node.decorator_list)
        if not has_wasm:
            continue
        if id(node) not in top_ids:
            raise StaticDecorationError(
                f"`{node.name}` (line {node.lineno}) carries @wasm but is not at module top level: "
                "conditional/nested decoration is not statically readable."
            )
        validate_node(node, node.name)
        _check_unique_binding(tree, node.name, node.lineno)
        found.append(node)  # type: ignore[arg-type]  (validate_node rejected async)
    return found


def _resolve_binding(fn, dec: ast.expr):
    """The object the decorator expression evaluates to in the function's module globals."""
    chain = _chain(decorator_head(dec))
    if not chain:
        return None
    obj = fn.__globals__.get(chain[0])
    for attr in chain[1:]:
        if obj is None:
            return None
        obj = getattr(obj, attr, None)
    return obj


def read_module_source(fn) -> str:
    """The module source text of a live function, or raise SourceUnavailable."""
    name = getattr(fn, "__name__", "<anonymous>")
    filename = fn.__code__.co_filename
    if not filename or filename.startswith("<"):
        raise StaticDecorationError(
            f"`{name}`: source is not a readable file ({filename!r}); @wasm must be statically "
            "readable (no exec/eval/REPL definitions)."
        )
    linecache.checkcache(filename)  # never trust a cached copy of a file that changed on disk
    # module_globals lets linecache ask the module's LOADER for the source (zipimport etc.)
    lines = linecache.getlines(filename, fn.__globals__)
    if not lines:
        raise SourceUnavailable(f"`{name}`: cannot read source file {filename!r} (sourceless deployment?)")
    return "".join(lines)


def locate(fn) -> Located:
    """Static check on a LIVE function object (the decorator's entry point)."""
    name = getattr(fn, "__name__", "<anonymous>")
    code = getattr(fn, "__code__", None)
    if code is None:
        raise StaticDecorationError(f"@wasm expects a plain Python function, got {fn!r}")
    if getattr(fn, "__qualname__", name) != name:
        raise StaticDecorationError(
            f"`{fn.__qualname__}` is not a module-level function: nested/method decoration is "
            "not statically readable."
        )
    module_source = read_module_source(fn)
    filename = code.co_filename
    try:
        tree = ast.parse(module_source, filename)
    except SyntaxError as e:  # pragma: no cover - the file just imported, so it parses
        raise StaticDecorationError(f"`{name}`: cannot parse {filename!r}: {e}") from e

    target = code.co_firstlineno
    top: ast.FunctionDef | ast.AsyncFunctionDef | None = None
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name and first_line(node) == target:
            top = node
            break
    if top is None:
        for node in ast.walk(tree):
            if (
                isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
                and node.name == name
                and first_line(node) == target
            ):
                raise StaticDecorationError(
                    f"`{name}` (line {target}) is not at module top level: conditional/nested "
                    "decoration is not statically readable."
                )
        raise StaticDecorationError(
            f"`{name}`: no top-level `def {name}` starts at line {target} of {filename!r}; "
            "the decoration is not statically readable (dynamic application?)."
        )
    validate_node(top, name)
    _check_unique_binding(tree, name, top.lineno)

    # The decorator expression must resolve to pythscribe's decorator in the module --
    # for the bare name AND the attribute forms (a shim named `pythscribe` or `wasm` would
    # make the build compile something the runtime never wrapped; review R1/SF1).
    from . import decorators  # local import: decorators imports this module

    bound = _resolve_binding(fn, top.decorator_list[0])
    if bound is not decorators.wasm:
        raise StaticDecorationError(
            f"`{name}`: `@{ast.unparse(top.decorator_list[0])}` in {filename!r} does not resolve to "
            f"`pythscribe.wasm` (it is {bound!r}); shadowed decoration is not statically readable."
        )

    assert isinstance(top, ast.FunctionDef)
    ks = kernel_source(module_source, top)
    return Located(node=top, module_source=module_source, kernel_source=ks, source_sha256=sha256_text(ks))
