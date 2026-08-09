# Type stubs (`.pyi` files)

PythScribe ships hand-authored type stubs for the React/Next.js ecosystem. Drop your own to extend the type checker for any other package.

## What stubs do

When you write `from react import use_state`, the type checker needs to know:
- That `use_state` is a callable.
- That it takes one positional argument.
- That it returns a tuple of `(value, setter)`.

Without a stub, the checker treats `use_state` as `Any` — your code compiles but typos and wrong-arity calls slip through. With a stub, those errors are caught at `pyths check` time.

## Bundled stubs

PythScribe ships bundled stubs for the packages it has first-class support for:

| Module | Coverage |
|---|---|
| `react`, `pyths.react` | All hooks, `@component`, `@psx`, `Fragment`, `Suspense`, `StrictMode`, core API |
| `next` | App Router + Pages Router APIs, `Link`/`Image`/`Head`/`Script`, special-export functions |
| `react_router_dom`, `react_router` | Hooks + components |
| `at_tanstack.react_query` | TanStack Query hooks + `QueryClient` |

These load automatically when your `.ps` file imports from one of those modules. No setup needed.

## Adding stubs for a new package

1. **Create a `.pyi` file** following the standard Python type stub format:

   ```python
   # stubs/my_lib.pyi
   from typing import Any, Callable, List, Optional

   def use_my_hook(options: dict) -> Any:
       ...

   def configure(opts: Optional[dict] = None) -> None:
       ...

   class MyClass:
       pass
   ```

   PythScribe's stub parser uses the same lexer/parser as `.ps` files, so all standard PEP 484 type-annotation syntax works (`Optional`, `List[int]`, `Callable[[...], T]`, etc.).

   **Important**: function bodies must be on a separate indented line:

   ```python
   def foo(x: int) -> str:
       ...                # <— indented `...` on its own line
   ```

   Single-line `def foo(): ...` isn't yet supported by the parser.

2. **Drop the file** in a directory inside your project (typically `stubs/`) and register that directory in `pyths.toml`:

   ```toml
   # pyths.toml at your project root
   [stubs]
   paths = ["./stubs"]
   ```

   Multiple directories are searched in declaration order; the first `<dir>/<module>.pyi` that exists wins. Paths are resolved relative to the `pyths.toml` file's directory.

3. **Verify** by running `pyths check` on a `.ps` file that imports from your module. With the stub registered, mistyped calls produce a clear error; without it, references fall back to `Any`.

## Stub resolution order

When the checker sees `from <module> import <name>`:

1. **Project-local stubs** in each `pyths.toml [stubs.paths]` directory, in declaration order. The first `<dir>/<module>.pyi` that exists wins. Project-local stubs **override** bundled ones for the same module — useful for pinning a specific package version's API in your project.
2. **Bundled stub**: looks up `<module>` in `crates/pyths_types/src/stubs.rs::STUBS`. If found, the stub is parsed and `<name>` is bound to its declared type.
3. **Fallback**: any name not covered binds as `Type::Any`. No error — stubs are best-effort and missing entries are silent.

## What gets type-checked

With a stub bound for `use_state`:

```python
from react import use_state

count, set_count = use_state(0)   # ✓ typechecks
count: str = use_state(0)          # ✗ Type mismatch: expected str, got Tuple[Any, Callable[[Any], None]]
use_state()                        # ✗ wrong arity (when arity-checking is wired)
```

What's *now* checked thanks to generic resolution:

```python
count, set_count = use_state(0)     # count: int, set_count: Callable[[int], None]
result: str = count                  # ✗ Type mismatch: expected str, got int

use_ref(0).current = "x"             # ✗ Ref is generic T; T bound to int by initial arg
```

Generic signatures in stubs use single-letter type-variable names (`T`, `U`, `S`, `A`, `K`, `V`, `E`) by convention. PEP 484 variance suffixes (`T_co`, `T_contra`) are also recognized. At each call site, the checker unifies the declared parameter types against the actual argument types and substitutes the bindings into the return type.

## Authoring conventions

- **Use `...` (ellipsis) as the body**, indented on its own line.
- **Annotate everything**: parameters, return types, class fields where relevant.
- **Use `Any` liberally for now** — finer types will arrive with full generics support.
- **Keep one stub file per top-level package**. Sub-packages (e.g., `next/router`) can be inline class/function blocks in the parent stub or separate files registered under sub-keys.
- **Don't import from compiled-only modules**: `from typing import Any` works because `typing` is a stub-only convention; PythScribe silently skips that import at codegen.

## Where to put stubs

**Project-local** (most common): a `stubs/` (or similarly named) directory at your project root, registered in `pyths.toml` under `[stubs] paths`. Edit and reload at `pyths check` time without recompiling the toolchain.

**Upstream** (for community-wide coverage): add to `crates/pyths_types/stubs/` and register in `STUBS`. Bundled stubs are embedded in the binary via `include_str!` and ship with every release.

## Testing your stub

Add a test case to `crates/pyths_types/src/checker.rs::tests` along the lines of:

```rust
#[test]
fn my_lib_stub_resolves() {
    let errors = check("from my_lib import use_my_hook\nuse_my_hook({})");
    assert!(errors.is_empty(), "stub-bound hook call: {:?}", errors);
}
```

Run with `cargo test -p pyths_types`.
