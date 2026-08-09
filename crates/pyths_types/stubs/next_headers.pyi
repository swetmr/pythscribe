# Type stubs for `next/headers` — Next.js App Router server-side
# request-header / cookie / draft-mode accessors.
#
# Available only inside async server components, server actions, route
# handlers, and middleware. The compiler does not enforce that
# constraint (it's a runtime contract); the stub gives `pyths check`
# the type signatures so call sites are checked correctly.

from typing import Any, Callable, Dict, List, Optional


# -------- headers() --------
# Read-only access to the inbound request's HTTP headers.
class ReadonlyHeaders:
    pass

def headers() -> ReadonlyHeaders:
    ...


# -------- cookies() --------
# Read/write the request's cookies. Returned object exposes get/set/
# delete; semantics differ by context (read-only in server components,
# read/write in server actions / route handlers).
class RequestCookie:
    pass

class RequestCookies:
    pass

def cookies() -> RequestCookies:
    ...


# -------- draftMode() --------
class DraftMode:
    pass

def draft_mode() -> DraftMode:
    ...
