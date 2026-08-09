# Type stubs for `swr` — React Hooks for Data Fetching.

from typing import Any, Callable, Optional


class SWRResponse:
    pass


def use_swr(
    key: Any,
    fetcher: Optional[Callable[..., Any]] = None,
    options: Optional[Any] = None,
) -> SWRResponse:
    ...


def use_swr_infinite(
    get_key: Callable[..., Any],
    fetcher: Optional[Callable[..., Any]] = None,
    options: Optional[Any] = None,
) -> SWRResponse:
    ...


def use_swr_immutable(
    key: Any,
    fetcher: Optional[Callable[..., Any]] = None,
    options: Optional[Any] = None,
) -> SWRResponse:
    ...


def mutate(key: Any, data: Optional[Any] = None, options: Optional[Any] = None) -> Any:
    ...


class SWRConfig:
    pass
