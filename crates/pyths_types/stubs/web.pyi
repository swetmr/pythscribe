# Type stubs for `pyths.web` — the W* preset expansion target.
#
# W* expands to: from pyths.web import handler, Response
#
# Declares `handler` + `Response` so `pyths check` can type-check W*-tier
# modules at compile time.
#
# `Response` mirrors the standard Web API Response constructor.
# `handler(fn)` returns the Cloudflare Worker default-export entry shape
# { fetch: fn } — typed here as a plain Any since the shape is JS-native.
#
# Design follow-up: when codegen adds @handler auto-wiring, expand this
# stub to reflect the resulting __default__ type (WorkerEntrypoint or similar).

from typing import Any, Callable


# -------- Response --------
# The standard Web API Response constructor, re-exported from globalThis.
# Construct with: Response(body, { "status": 200, "headers": {...} })

class Response:
    status: int
    ok: bool
    headers: Any
    body: Any

    def __init__(self, body: Any = None, init: Any = None) -> None:
        ...

    async def text(self) -> str:
        ...

    async def json(self) -> Any:
        ...

    async def blob(self) -> Any:
        ...

    async def array_buffer(self) -> Any:
        ...

    def clone(self) -> "Response":
        ...


# -------- handler --------
# Wraps a fetch function into the Cloudflare Worker default-export entry
# shape { fetch: fn }.
#
# Two forms are supported:
#
#   Plain call (B-029):
#     __default__ = handler(my_fetch)
#     → compiles to:  export default handler(my_fetch);
#       (runtime returns { fetch: my_fetch })
#
#   Decorator (B-029 follow-up C — codegen auto-wires):
#     @handler
#     async def fetch(request): ...
#     → compiles to:  async function fetch(request) { ... }
#                     export default { fetch: fetch };
#     The decorator is consumed by codegen; the function itself is
#     emitted as a plain named function.  No runtime handler() call
#     is emitted — the default export shape is wired directly.

def handler(fn: Callable[..., Any]) -> Any:
    ...
