# API Reference

Reference for all PythScribe standard library and web modules.

## Standard Library Modules

Import with `from pyths import <module>` or `from pyths.<module> import <name>`.

---

### `math` — Mathematical Functions

```python
from pyths import math
```

#### Constants

| Name | Value | Description |
|------|-------|-------------|
| `math.pi` | 3.141592653589793 | Ratio of circumference to diameter |
| `math.e` | 2.718281828459045 | Euler's number |
| `math.tau` | 6.283185307179586 | 2 * pi |
| `math.inf` | Infinity | Positive infinity |
| `math.nan` | NaN | Not a number |

#### Rounding

| Function | Description |
|----------|-------------|
| `math.ceil(x)` | Smallest integer >= x |
| `math.floor(x)` | Largest integer <= x |
| `math.trunc(x)` | Truncate to integer (toward zero) |

#### Powers and Logarithms

| Function | Description |
|----------|-------------|
| `math.sqrt(x)` | Square root |
| `math.pow(x, y)` | x raised to y |
| `math.exp(x)` | e raised to x |
| `math.log(x, base=e)` | Logarithm (natural by default) |
| `math.log2(x)` | Base-2 logarithm |
| `math.log10(x)` | Base-10 logarithm |

#### Trigonometry

| Function | Description |
|----------|-------------|
| `math.sin(x)`, `math.cos(x)`, `math.tan(x)` | Sine, cosine, tangent (radians) |
| `math.asin(x)`, `math.acos(x)`, `math.atan(x)` | Inverse trig functions |
| `math.atan2(y, x)` | Two-argument arctangent |
| `math.sinh(x)`, `math.cosh(x)`, `math.tanh(x)` | Hyperbolic functions |
| `math.degrees(x)` | Radians to degrees |
| `math.radians(x)` | Degrees to radians |
| `math.hypot(*args)` | Euclidean distance |

#### Combinatorics

| Function | Description |
|----------|-------------|
| `math.factorial(n)` | n! |
| `math.gcd(a, b)` | Greatest common divisor |
| `math.lcm(a, b)` | Least common multiple |
| `math.comb(n, k)` | Combinations (n choose k) |
| `math.perm(n, k=None)` | Permutations |

#### Other

| Function | Description |
|----------|-------------|
| `math.abs(x)` / `math.fabs(x)` | Absolute value |
| `math.copysign(x, y)` | x with sign of y |
| `math.fmod(x, y)` | Floating-point modulo |
| `math.isclose(a, b, rel_tol=1e-9, abs_tol=0.0)` | Approximate equality |
| `math.isfinite(x)` | True if finite |
| `math.isinf(x)` | True if infinite |
| `math.isnan(x)` | True if NaN |
| `math.prod(iterable, start=1)` | Product of elements |
| `math.fsum(iterable)` | Accurate floating-point sum |

#### Example

```python
from pyths import math

print(math.sqrt(16))              # 4.0
print(math.factorial(5))          # 120
print(math.gcd(12, 8))            # 4
print(math.isclose(0.1 + 0.2, 0.3, rel_tol=1e-9))  # true
angle = math.radians(45)
print(math.sin(angle))            # 0.7071067811865476
```

---

### `json` — JSON Serialization

```python
from pyths import json
```

| Function | Description |
|----------|-------------|
| `json.dumps(obj, indent=None, sort_keys=False)` | Serialize object to JSON string |
| `json.loads(s)` | Parse JSON string to object |

```python
from pyths import json

data = {"name": "Alice", "scores": [95, 87, 92]}
text = json.dumps(data, indent=2)
print(text)

parsed = json.loads(text)
print(parsed["name"])          # Alice
```

---

### `itertools` — Iterator Utilities

```python
from pyths import itertools
```

All functions return generators (lazy evaluation).

#### Infinite Iterators

| Function | Description |
|----------|-------------|
| `itertools.count(start=0, step=1)` | Infinite counter |
| `itertools.cycle(iterable)` | Cycle through elements forever |
| `itertools.repeat(value, times=None)` | Repeat value (optionally limited) |

#### Finite Iterators

| Function | Description |
|----------|-------------|
| `itertools.chain(*iterables)` | Concatenate iterables |
| `itertools.chain_from_iterable(iterable)` | Chain from nested iterable |
| `itertools.islice(iterable, stop)` / `islice(iterable, start, stop, step)` | Slice an iterator |
| `itertools.zip_longest(*iterables)` | Zip filling with None |
| `itertools.accumulate(iterable, func=add, initial=None)` | Running accumulation |
| `itertools.takewhile(predicate, iterable)` | Take while predicate is true |
| `itertools.dropwhile(predicate, iterable)` | Drop while predicate is true |
| `itertools.filterfalse(predicate, iterable)` | Elements where predicate is false |
| `itertools.starmap(func, iterable)` | Apply func to unpacked arguments |
| `itertools.groupby(iterable, key=None)` | Group consecutive elements |
| `itertools.tee(iterable, n=2)` | Create n independent iterators |
| `itertools.pairwise(iterable)` | Consecutive pairs |

#### Combinatoric Iterators

| Function | Description |
|----------|-------------|
| `itertools.product(*iterables)` | Cartesian product |
| `itertools.permutations(iterable, r=None)` | r-length permutations |
| `itertools.combinations(iterable, r)` | r-length combinations |
| `itertools.combinations_with_replacement(iterable, r)` | Combinations with repeats |

#### Example

```python
from pyths import itertools

# Chain
combined = list(itertools.chain([1, 2], [3, 4]))   # [1, 2, 3, 4]

# Combinations
combos = list(itertools.combinations("ABCD", 2))
# [["A","B"], ["A","C"], ["A","D"], ["B","C"], ["B","D"], ["C","D"]]

# Accumulate
running = list(itertools.accumulate([1, 2, 3, 4]))  # [1, 3, 6, 10]

# Group by
data = [("a", 1), ("a", 2), ("b", 3)]
for key, group in itertools.groupby(data, lambda x: x[0]):
    print(key, list(group))
```

---

### `functools` — Function Utilities

```python
from pyths import functools
```

| Function | Description |
|----------|-------------|
| `functools.reduce(func, iterable, initializer=None)` | Left fold |
| `functools.partial(func, *args)` | Partial function application |
| `functools.lru_cache(maxsize=128)` | Memoization decorator |
| `functools.cache(func)` | Unbounded memoization decorator |
| `functools.wraps(wrapped)` | Preserve function metadata |
| `functools.cmp_to_key(mycmp)` | Comparison to key function |
| `functools.total_ordering(cls)` | Class decorator for comparison methods |

#### Example

```python
from pyths import functools

# reduce
total = functools.reduce(lambda a, b: a + b, [1, 2, 3, 4])  # 10

# partial
def power(base, exp):
    return base ** exp
square = functools.partial(power, exp=2)
print(square(5))           # 25

# lru_cache
@functools.lru_cache(maxsize=128)
def fibonacci(n):
    if n < 2:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)
print(fibonacci(30))       # 832040
```

---

### `collections` — Collection Types

```python
from pyths import collections
```

#### `Counter(iterable=None)`

Counts element frequencies.

```python
c = collections.Counter(["a", "b", "a", "c", "a", "b"])
print(c.most_common(2))       # [["a", 3], ["b", 2]]
print(c.total())               # 6
```

| Method | Description |
|--------|-------------|
| `most_common(n=None)` | n most frequent elements |
| `elements()` | Generator of all elements |
| `update(iterable)` | Add counts |
| `subtract(iterable)` | Subtract counts |
| `total()` | Sum of all counts |

#### `defaultdict(default_factory, entries=None)`

Dict with auto-generated default values.

```python
dd = collections.defaultdict(list)
dd["fruits"].append("apple")
dd["fruits"].append("banana")
print(dd["fruits"])            # ["apple", "banana"]
print(dd["empty"])             # []  (auto-created)
```

#### `deque(iterable=None, maxlen=None)`

Double-ended queue.

```python
dq = collections.deque([1, 2, 3], maxlen=5)
dq.appendleft(0)
dq.append(4)
print(list(dq))                # [0, 1, 2, 3, 4]
dq.rotate(2)
print(list(dq))                # [3, 4, 0, 1, 2]
```

| Method | Description |
|--------|-------------|
| `append(x)` / `appendleft(x)` | Add to right/left |
| `pop()` / `popleft()` | Remove from right/left |
| `extend(iterable)` / `extendleft(iterable)` | Extend right/left |
| `rotate(n=1)` | Rotate n steps right |
| `clear()` | Remove all elements |
| `count(x)` | Count occurrences |
| `index(x, start=0, stop=None)` | Find index |
| `remove(x)` | Remove first occurrence |
| `reverse()` | Reverse in-place |
| `copy()` | Shallow copy |

#### `OrderedDict(entries=None)`

Dict preserving insertion order.

```python
od = collections.OrderedDict([["b", 2], ["a", 1], ["c", 3]])
od.move_to_end("b")           # Move "b" to end
print(list(od.keys()))         # ["a", "c", "b"]
```

| Method | Description |
|--------|-------------|
| `move_to_end(key, last=True)` | Move key to end (or beginning) |
| `popitem(last=True)` | Pop last (or first) item |

#### `namedtuple(name, fields)`

Factory for tuple-like classes with named fields.

```python
Point = collections.namedtuple("Point", ["x", "y"])
p = Point(1, 2)
print(p.x, p.y)               # 1 2
print(p._asdict())            # {"x": 1, "y": 2}
p2 = p._replace(x=10)
print(p2)                     # Point(x=10, y=2)
```

---

### `random` — Random Numbers

```python
from pyths import random
```

| Function | Description |
|----------|-------------|
| `random.seed(n)` | Seed the module PRNG — reproducible sequences (see note) |
| `random.random()` | Float in [0.0, 1.0) |
| `random.randint(a, b)` | Integer in [a, b] |
| `random.randrange(start, stop=None, step=1)` | Random from range |
| `random.choice(seq)` | Random element |
| `random.choices(population, weights=None, k=1)` | Weighted choices (with replacement) |
| `random.shuffle(arr)` | Shuffle in-place |
| `random.sample(population, k)` | k unique random elements |
| `random.uniform(a, b)` | Float in [a, b] |
| `random.gauss(mu=0, sigma=1)` | Gaussian distribution |
| `random.expovariate(lambd)` | Exponential distribution |
| `random.triangular(low=0, high=1, mode=None)` | Triangular distribution |
| `random.betavariate(alpha, beta)` | Beta distribution |
| `random.gammavariate(alpha, beta)` | Gamma distribution |
| `random.Random(seed)` | Independent seedable PRNG instance |

> **Seeding note (by-design deviation):** `random.seed(n)` makes every
> `random.*` call deterministic *within PythScribe* (same seed → same
> sequence). The generator is mulberry32, not CPython's Mersenne Twister, so
> the values do **not** match CPython's for the same seed — same class of
> deviation as the ≤4-ULP transcendentals. See `docs/known-limitations.md`.

#### Example

```python
from pyths import random

print(random.randint(1, 6))        # Dice roll
print(random.choice(["a", "b", "c"]))
items = [1, 2, 3, 4, 5]
random.shuffle(items)
print(items)
print(random.sample(range(100), 5))
```

---

### `datetime` — Date and Time

```python
from pyths import datetime
```

#### `timedelta(days=0, seconds=0, microseconds=0, milliseconds=0, minutes=0, hours=0, weeks=0)`

Represents a time duration.

```python
from pyths.datetime import timedelta

d = timedelta(days=5, hours=3)
print(d.total_seconds())       # 442800
print(d.days)                  # 5
```

#### `date(year, month, day)`

Represents a calendar date.

```python
from pyths.datetime import date

today = date.today()
print(today.isoformat())      # "2026-03-03"
print(today.weekday())         # 0 = Monday

d = date(2026, 1, 1)
diff = today - d               # timedelta
```

| Method | Description |
|--------|-------------|
| `date.today()` | Current date (static) |
| `date.fromisoformat(s)` | Parse ISO format (static) |
| `isoformat()` | Format as ISO string |
| `weekday()` | 0 = Monday, 6 = Sunday |
| `isoweekday()` | 1 = Monday, 7 = Sunday |
| `strftime(fmt)` | Format with directives |

#### `time(hour=0, minute=0, second=0, microsecond=0)`

Represents a time of day.

```python
from pyths.datetime import time

t = time(14, 30, 0)
print(t.isoformat())          # "14:30:00"
```

#### `datetime(year, month, day, hour=0, minute=0, second=0, microsecond=0)`

Represents date and time.

```python
from pyths.datetime import datetime

now = datetime.now()
print(now.isoformat())

dt = datetime(2026, 3, 15, 10, 30)
print(dt.timestamp())
print(dt.date())               # date object
print(dt.time())               # time object
```

| Method | Description |
|--------|-------------|
| `datetime.now()` | Current datetime (static) |
| `datetime.today()` | Same as now() (static) |
| `datetime.fromisoformat(s)` | Parse ISO format (static) |
| `datetime.fromtimestamp(ts)` | From Unix timestamp (static) |
| `isoformat(sep="T")` | Format as ISO string |
| `timestamp()` | Unix timestamp |
| `date()` | Extract date part |
| `time()` | Extract time part |
| `weekday()` | 0 = Monday |
| `strftime(fmt)` | Format with directives |

**strftime directives**: `%Y` (year), `%m` (month), `%d` (day), `%H` (hour), `%M` (minute), `%S` (second), `%A` (weekday name), `%B` (month name), `%p` (AM/PM).

---

### `re` — Regular Expressions

```python
from pyths import re
```

#### Flags

| Flag | Alias | Description |
|------|-------|-------------|
| `re.IGNORECASE` | `re.I` | Case-insensitive matching |
| `re.MULTILINE` | `re.M` | `^`/`$` match line boundaries |
| `re.DOTALL` | `re.S` | `.` matches newline |
| `re.GLOBAL` | — | Find all matches |

#### Functions

| Function | Description |
|----------|-------------|
| `re.search(pattern, string, flags=0)` | First match anywhere in string |
| `re.match(pattern, string, flags=0)` | Match at start of string |
| `re.fullmatch(pattern, string, flags=0)` | Match entire string |
| `re.findall(pattern, string, flags=0)` | List of all matches |
| `re.finditer(pattern, string, flags=0)` | Generator of Match objects |
| `re.sub(pattern, repl, string, count=0, flags=0)` | Replace matches |
| `re.subn(pattern, repl, string, count=0, flags=0)` | Replace with count |
| `re.split(pattern, string, maxsplit=0, flags=0)` | Split at matches |
| `re.compile(pattern, flags=0)` | Compile pattern for reuse |
| `re.escape(string)` | Escape special characters |

#### Match Object

| Method | Description |
|--------|-------------|
| `m.group(n=0)` | Matched text (0 = full, 1+ = groups) |
| `m.groups(default=None)` | All capture groups |
| `m.groupdict(default=None)` | Named groups as dict |
| `m.start(group=0)` | Start index |
| `m.end(group=0)` | End index |
| `m.span(group=0)` | (start, end) tuple |

#### Example

```python
from pyths import re

# Search
m = re.search(r"(\d+)-(\d+)", "call 555-1234")
if m:
    print(m.group(0))          # "555-1234"
    print(m.group(1))          # "555"
    print(m.groups())          # ["555", "1234"]

# Find all
emails = re.findall(r"[\w.]+@[\w.]+", text)

# Replace
cleaned = re.sub(r"\s+", " ", "  too   many   spaces  ")

# Compile for reuse
pattern = re.compile(r"^[A-Z]\w+", re.MULTILINE)
matches = pattern.findall(text)
```

---

## Web Modules

Import with `from pyths.<module> import <name>`.

---

### `fetch` — HTTP Requests

```python
from pyths.fetch import get, post, put, patch, delete_, head
```

All functions are **async** and return a `Response` object.

#### Functions

| Function | Description |
|----------|-------------|
| `get(url, headers=None, params=None, timeout=None)` | GET request |
| `post(url, data=None, json=None, headers=None, timeout=None)` | POST request |
| `put(url, data=None, json=None, headers=None, timeout=None)` | PUT request |
| `patch(url, data=None, json=None, headers=None, timeout=None)` | PATCH request |
| `delete_(url, headers=None, timeout=None)` | DELETE request |
| `head(url, headers=None, timeout=None)` | HEAD request |

Note: `delete_` has a trailing underscore to avoid conflict with the Python `del` keyword.

#### Response Object

| Property/Method | Description |
|-----------------|-------------|
| `response.status` | HTTP status code |
| `response.status_code` | Alias for status |
| `response.ok` | True if status 200-299 |
| `response.headers` | Response headers |
| `response.url` | Response URL |
| `await response.text()` | Body as string |
| `await response.json()` | Body parsed as JSON |
| `await response.blob()` | Body as Blob |
| `await response.array_buffer()` | Body as ArrayBuffer |
| `response.raise_for_status()` | Throw if not ok |

#### Example

```python
from pyths.fetch import get, post

async def load_users():
    response = await get("https://api.example.com/users")
    response.raise_for_status()
    users = await response.json()
    return users

async def create_user(name, email):
    response = await post(
        "https://api.example.com/users",
        json={"name": name, "email": email},
        headers={"Authorization": "Bearer TOKEN"}
    )
    return await response.json()

# With query parameters
async def search(query):
    response = await get(
        "https://api.example.com/search",
        params={"q": query, "limit": 10}
    )
    return await response.json()
```

---

### `storage` — Browser Storage

```python
from pyths.storage import local, session, cookies
```

#### `local` / `session`

Wrappers for `localStorage` and `sessionStorage`. Values are automatically JSON-encoded/decoded.

| Method | Description |
|--------|-------------|
| `storage.get(key, default=None)` | Get value (JSON-decoded) |
| `storage.set(key, value)` | Set value (JSON-encoded) |
| `storage.delete(key)` | Remove key |
| `storage.has(key)` | Check if key exists |
| `storage.clear()` | Remove all keys |
| `storage.keys()` | All keys |
| `storage.values()` | All values |
| `storage.items()` | All (key, value) pairs |
| `storage.length` | Number of stored items |

Both are iterable: `for key, value in local:`.

Returns `None` if storage is unavailable (e.g., in Node.js or private browsing).

#### `cookies`

Cookie management.

| Method | Description |
|--------|-------------|
| `cookies.get(name, default=None)` | Get cookie value |
| `cookies.set(name, value, days=None, path="/", secure=False, same_site="Lax")` | Set cookie |
| `cookies.delete(name, path="/")` | Delete cookie |
| `cookies.has(name)` | Check if cookie exists |

#### Example

```python
from pyths.storage import local, session, cookies

# localStorage
local.set("user", {"name": "Alice", "theme": "dark"})
user = local.get("user")
print(user["name"])                # "Alice"

if local.has("user"):
    local.delete("user")

# sessionStorage
session.set("token", "abc123")

# Cookies
cookies.set("theme", "dark", days=30)
print(cookies.get("theme"))        # "dark"
```

---

### `router` — Client-Side Routing

```python
from pyths.router import route, navigate, start, current_path, query_params
```

SPA-style client-side routing with path parameters.

#### Functions

| Function | Description |
|----------|-------------|
| `route(path, handler)` | Register route handler |
| `not_found(handler)` | Register 404 handler |
| `navigate(path, replace=False)` | Navigate to path |
| `on_navigate(callback)` | Called on every navigation |
| `current_path()` | Get current pathname |
| `query_params()` | Get query parameters as dict |
| `start()` | Initialize router (call after registering routes) |

#### Route Patterns

Routes support path parameters with `:param` or `<param>` syntax:

```python
route("/users/:id", show_user)        # /users/123 → {id: "123"}
route("/posts/<slug>", show_post)     # /posts/hello → {slug: "hello"}
route("/", show_home)                 # Exact match
```

#### Router Class

For multiple router instances:

```python
from pyths.router import Router

api_router = Router()
api_router.route("/api/users/:id", handle_user)
result = api_router.match("/api/users/42")
# {"handler": handle_user, "params": {"id": "42"}}
```

#### Example

```python
from pyths.router import route, not_found, navigate, start

def home(params):
    print("Home page")

def user_profile(params):
    print(f"User: {params['id']}")

def page_not_found(params):
    print("404 - Page not found")

route("/", home)
route("/users/:id", user_profile)
not_found(page_not_found)

start()

# Programmatic navigation
navigate("/users/42")
```

---

## Utility Modules

### `utils.tenacity` — Retry Decorator

```python
from pyths.utils.tenacity import retry
```

A retry decorator inspired by Python's [tenacity](https://github.com/jd/tenacity) library. Works with both sync and async functions.

#### `retry(**options)`

Returns a decorator that retries the function on failure.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_attempts` | int | 3 | Maximum number of attempts |
| `delay` | float | 0 | Base delay between retries (seconds) |
| `backoff` | str | `"fixed"` | `"fixed"` or `"exponential"` |
| `multiplier` | int | 2 | Multiplier for exponential backoff |
| `max_delay` | float | 60 | Maximum delay cap (seconds) |
| `retry_on` | class/list | all errors | Error class(es) to retry on |
| `on_retry` | callable | None | `callback(error, attempt)` before each retry |
| `reraise` | bool | True | Re-throw last error when all attempts fail |

#### Example

```python
from pyths.utils.tenacity import retry
from pyths.fetch import get

# Basic retry
@retry()
def connect():
    return open_connection()

# Exponential backoff with async
@retry(max_attempts=5, delay=1.0, backoff="exponential", max_delay=30)
async def fetch_with_retry(url):
    response = await get(url)
    response.raise_for_status()
    return await response.json()

# Retry only on specific errors, with logging
@retry(
    max_attempts=3,
    delay=2.0,
    retry_on=ConnectionError,
    on_retry=lambda err, n: print(f"Retry {n}: {err}")
)
async def send_event(payload):
    await post("/api/events", json=payload)
```
