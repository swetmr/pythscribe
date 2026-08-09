# Type stubs for `@tanstack/react-query` v4+.
# Pythonic import path: `from at_tanstack.react_query import use_query`

from typing import Any, Callable, Dict, List, Optional


def use_query(options: Dict[str, Any]) -> Any:
    ...

def use_mutation(options: Dict[str, Any]) -> Any:
    ...

def use_query_client() -> Any:
    ...

def use_queries(options: Dict[str, Any]) -> List[Any]:
    ...

def use_infinite_query(options: Dict[str, Any]) -> Any:
    ...

def use_is_fetching(filters: Optional[Dict[str, Any]] = None) -> int:
    ...

def use_is_mutating(filters: Optional[Dict[str, Any]] = None) -> int:
    ...


class QueryClient:
    pass

class QueryClientProvider:
    pass
