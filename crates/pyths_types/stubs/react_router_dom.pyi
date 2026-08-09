# Type stubs for `react-router-dom` v6+.

from typing import Any, Callable, Dict, List, Optional, Tuple


# -------- Hooks --------
def use_navigate() -> Callable[[str], None]:
    ...

def use_params() -> Dict[str, str]:
    ...

def use_location() -> Any:
    ...

def use_search_params() -> Tuple[Any, Callable[[Dict[str, str]], None]]:
    ...

def use_route_match() -> Optional[Any]:
    ...


# -------- Components --------
class Link:
    pass

class NavLink:
    pass

class Routes:
    pass

class Route:
    pass

class Outlet:
    pass

class Navigate:
    pass

class BrowserRouter:
    pass

class HashRouter:
    pass
