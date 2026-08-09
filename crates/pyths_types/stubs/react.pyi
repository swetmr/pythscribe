# Type stubs for the `react` package — and PythScribe's `pyths.react`
# wrapper, which shares the same surface.
#
# These declarations let `pyths check` catch wrong-arity calls, wrong
# argument types, and wrong-shape return-value usage at compile time.
# Hand-authored from the React 18 type definitions; covers the surface
# PythScribe user code commonly hits.
#
# Multi-line body convention: `def foo(...): ` followed by `\n    ...`
# (Ellipsis on the next line, indented). PythScribe's parser requires
# function bodies to be indented; a single-line `def foo(...): ...`
# isn't yet accepted.

from typing import Any, Callable, Optional, Tuple, List, Dict


# -------- @component / @psx (codegen-meta passthroughs) --------
def component(fn: Any) -> Any:
    ...

def psx(fn: Any) -> Any:
    ...


# -------- Hooks --------
#
# Generic signatures use single-letter `T`, `S`, `A` as type variables
# (PythScribe convention — single uppercase letters are recognized as
# generics). At each call site the checker substitutes the inferred
# argument type into the return type so `count, set_count =
# use_state(0)` types `count` as `int` and a later `count + "hello"`
# is flagged as an int-vs-str mismatch.

def use_state(initial: T) -> Tuple[T, Callable[[T], None]]:
    ...

def use_reducer(reducer: Callable[[S, A], S], initial: S) -> Tuple[S, Callable[[A], None]]:
    ...

def use_ref(initial: T = None) -> T:
    ...

def use_effect(effect: Callable[[], Any], deps: Optional[List[Any]] = None) -> None:
    ...

def use_layout_effect(effect: Callable[[], Any], deps: Optional[List[Any]] = None) -> None:
    ...

def use_insertion_effect(effect: Callable[[], Any], deps: Optional[List[Any]] = None) -> None:
    ...

def use_context(context: T) -> T:
    ...

def use_memo(factory: Callable[[], T], deps: List[Any]) -> T:
    ...

def use_callback(callback: Callable[..., Any], deps: List[Any]) -> Callable[..., Any]:
    ...

def use_transition() -> Tuple[bool, Callable[[Callable[[], Any]], None]]:
    ...

def use_deferred_value(value: Any) -> Any:
    ...

def use_id() -> str:
    ...

def use_imperative_handle(ref: Any, factory: Callable[[], Any], deps: Optional[List[Any]] = None) -> None:
    ...

def use_debug_value(value: Any, formatter: Optional[Callable[[Any], str]] = None) -> None:
    ...

# React 19: `use()` unwraps Promises and Contexts inside components
# (including async Server Components). Argument can be a Promise or a
# Context object; returns the resolved value / context value.
def use(resource: Any) -> Any:
    ...


# -------- React API --------
def create_context(default: Any = None) -> Any:
    ...

def create_element(type: Any, props: Optional[Dict[str, Any]] = None) -> Any:
    ...

def create_ref() -> Any:
    ...

def create_portal(children: Any, container: Any) -> Any:
    ...

def forward_ref(render: Callable[[Any, Any], Any]) -> Any:
    ...

def memo(component: Any, compare: Optional[Callable[[Any, Any], bool]] = None) -> Any:
    ...

def lazy(loader: Callable[[], Any]) -> Any:
    ...

def start_transition(scope: Callable[[], Any]) -> None:
    ...


# -------- Special components --------
class Fragment:
    pass

class Suspense:
    pass

class StrictMode:
    pass
