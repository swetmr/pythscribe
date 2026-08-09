# Type stubs for `zustand` — minimal state management for React.
#
# `create(state_creator)` returns a hook callable that returns the
# store. The hook accepts an optional selector function.

from typing import Any, Callable, Optional


def create(state_creator: Callable[..., Any]) -> Callable[..., Any]:
    ...


def create_store(state_creator: Callable[..., Any]) -> Any:
    ...


# Middleware
def persist(config: Callable[..., Any], options: Optional[Any] = None) -> Any:
    ...

def devtools(config: Callable[..., Any], options: Optional[Any] = None) -> Any:
    ...

def immer(config: Callable[..., Any]) -> Any:
    ...

def subscribe_with_selector(config: Callable[..., Any]) -> Any:
    ...
