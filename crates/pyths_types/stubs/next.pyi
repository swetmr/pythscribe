# Type stubs for Next.js core APIs.
# Covers next/router, next/link, next/image, next/head, plus App
# Router conventions (metadata, layout, page, loading, error).

from typing import Any, Callable, Dict, List, Optional


# -------- next/router (Pages Router) --------
class Router:
    pass

def use_router() -> Router:
    ...


# -------- next/navigation (App Router) --------
def use_pathname() -> str:
    ...

def use_search_params() -> Any:
    ...

def use_params() -> Dict[str, Any]:
    ...

def use_selected_layout_segment() -> Optional[str]:
    ...

def use_selected_layout_segments() -> List[str]:
    ...


# -------- next/link, next/image, next/head, next/script --------
class Link:
    pass

class Image:
    pass

class Head:
    pass

class Script:
    pass


# -------- Next.js special-export functions --------
# These names are recognized by the codegen and snake→camel'd at emit:
#   get_static_props → getStaticProps
#   get_server_side_props → getServerSideProps
#   ...
# The stubs declare expected signatures so user implementations type-check.
def get_static_props(context: Dict[str, Any]) -> Dict[str, Any]:
    ...

def get_server_side_props(context: Dict[str, Any]) -> Dict[str, Any]:
    ...

def get_static_paths() -> Dict[str, Any]:
    ...

def generate_metadata(params: Dict[str, Any]) -> Dict[str, Any]:
    ...

def generate_static_params() -> List[Dict[str, Any]]:
    ...
