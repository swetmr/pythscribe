# Type stubs for `next/navigation` — App Router navigation hooks and
# the imperative redirect/notFound APIs.
#
# Mirrors the hooks already in `next.pyi` so `from next.navigation
# import use_pathname` resolves identically to `from next import
# use_pathname` (which existing code uses).

from typing import Any, Callable, Dict, List, Optional


# -------- Read-only navigation hooks --------
def use_pathname() -> str:
    ...

def use_search_params() -> Any:
    ...

def use_params() -> Dict[str, Any]:
    ...

def use_router() -> Any:
    ...

def use_selected_layout_segment() -> Optional[str]:
    ...

def use_selected_layout_segments() -> List[str]:
    ...


# -------- Imperative navigation --------
# `redirect()` and `not_found()` throw special errors that React's
# server runtime catches; they never return.
def redirect(path: str) -> None:
    ...

def permanent_redirect(path: str) -> None:
    ...

def not_found() -> None:
    ...
