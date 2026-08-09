# Type stubs for `next/server` — Next.js Edge / Middleware / Route
# Handler request and response types.

from typing import Any, Callable, Dict, List, Optional


# -------- NextRequest --------
class NextRequest:
    pass


# -------- NextResponse --------
# Construction is via the static factory methods: NextResponse.json(),
# NextResponse.redirect(), NextResponse.next(), NextResponse.rewrite().
# The instance constructor wraps a Response.
class NextResponse:
    pass


# -------- Server-side helpers --------
# Middleware-only. Server components should prefer `next.navigation.redirect`.
class MiddlewareResponse:
    pass


# -------- userAgent helpers --------
class UserAgent:
    pass

def user_agent(req: NextRequest) -> UserAgent:
    ...
