# Tutorial 4: Advanced Features

This tutorial covers PythScribe's advanced features: dataclasses, pattern matching, generators, type checking, and the standard library.

## Dataclasses

The `@dataclass` decorator auto-generates a full class from field annotations:

```python
from dataclasses import dataclass, field

@dataclass
class User:
    name: str
    age: int
    email: str = ""
    tags: list = field(default_factory=list)

# Auto-generated: constructor, toString(), __eq__(), toDict(), fromDict()
user = User("Alice", 30, "alice@example.com")
print(user)                    # User(name=Alice, age=30, email=alice@example.com, tags=[])
print(user.toDict())           # {"name": "Alice", "age": 30, ...}

copy = User.fromDict(user.toDict())
print(user == copy)            # true
```

### Type Validation

The generated constructor validates types at runtime:

```python
@dataclass
class Product:
    name: str
    price: float
    quantity: int

Product("Widget", 9.99, 5)       # OK
Product("Widget", "free", 5)     # TypeError: Expected number for price, got string
```

### Field Constraints

Use `Field()` for validation rules:

```python
from dataclasses import dataclass, field, Field

@dataclass
class SignupForm:
    username: str = Field(min_length=3, max_length=20)
    password: str = Field(min_length=8)
    age: int = Field(gt=0, lt=150)
    email: str = Field(pattern=r"^[\w.+-]+@[\w-]+\.[\w.]+$")
```

Available constraints:
- `gt`, `ge`, `lt`, `le` — numeric bounds
- `min_length`, `max_length` — string/list length
- `pattern` — regex validation

### Frozen Dataclasses

```python
@dataclass(frozen=True)
class Point:
    x: float
    y: float

p = Point(1.0, 2.0)
p.x = 3.0   # Error: Cannot modify frozen dataclass
```

### Validators

```python
@dataclass
class User:
    name: str
    email: str

    @validator
    def validate_email(self):
        if "@" not in self.email:
            raise ValueError("Invalid email")
```

Validators run automatically after the constructor sets all fields.

## Pattern Matching

PythScribe supports full `match/case` with all Python pattern types:

### Literal Patterns

```python
def http_status(code):
    match code:
        case 200:
            return "OK"
        case 404:
            return "Not Found"
        case 500:
            return "Internal Server Error"
        case _:
            return f"Status {code}"
```

### Capture and Guard Patterns

```python
def classify(value):
    match value:
        case x if x < 0:
            return "negative"
        case 0:
            return "zero"
        case x if x > 100:
            return "large"
        case x:
            return f"small positive: {x}"
```

### Sequence Patterns

```python
def process(command):
    match command:
        case ["quit"]:
            return "Goodbye"
        case ["greet", name]:
            return f"Hello, {name}!"
        case ["add", *numbers]:
            return sum(int(n) for n in numbers)
        case _:
            return "Unknown command"

process(["greet", "Alice"])       # "Hello, Alice!"
process(["add", "1", "2", "3"])   # 6
```

### Class Patterns

```python
class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

def describe(shape):
    match shape:
        case Point(0, 0):
            return "origin"
        case Point(x, 0):
            return f"on x-axis at {x}"
        case Point(0, y):
            return f"on y-axis at {y}"
        case Point(x, y):
            return f"point at ({x}, {y})"
```

### OR Patterns

```python
def weekend(day):
    match day:
        case "Saturday" | "Sunday":
            return True
        case _:
            return False
```

### Mapping Patterns

```python
def process_event(event):
    match event:
        case {"type": "click", "x": x, "y": y}:
            return f"Click at ({x}, {y})"
        case {"type": "keypress", "key": key}:
            return f"Key: {key}"
        case _:
            return "Unknown event"
```

## Generators

Generator functions use `yield` and compile to JS `function*`:

```python
def fibonacci():
    a, b = 0, 1
    while True:
        yield a
        a, b = b, a + b

# Use with for loop
fib = fibonacci()
for i in range(10):
    print(next(fib))

# Generator expression
squares = (x ** 2 for x in range(10))
```

### Practical Example: Pagination

```python
def paginate(items, page_size):
    for i in range(0, len(items), page_size):
        yield items[i:i + page_size]

all_items = list(range(100))
for page in paginate(all_items, 10):
    print(f"Page: {page}")
```

## Type Checking

PythScribe can validate type annotations at compile time:

```python
# types_demo.ps
def add(a: int, b: int) -> int:
    return a + b

def greet(name: str) -> str:
    return f"Hello, {name}!"

x: int = 42
y: str = "hello"
```

```bash
# Check types
pyths check types_demo.ps
# ✓ types_demo.ps — no errors
```

### Catching Type Errors

```python
# bad_types.ps
x: int = "hello"              # Type mismatch: expected int, got str

def add(a: int, b: int) -> int:
    return "not a number"      # Return type mismatch
```

```bash
pyths check bad_types.ps
# error: Type mismatch — expected int, got str
#   ┌─ bad_types.ps:1:10
# ...
```

### TypeScript Declaration Files

Generate `.d.ts` files for TypeScript interop:

```bash
pyths compile app.ps -o app.js --dts
```

This creates both `app.js` and `app.d.ts`:

```typescript
// app.d.ts
export declare function add(a: number, b: number): number;
export declare function greet(name: string): string;
export declare const x: number;
```

## Standard Library

PythScribe includes Python standard library ports. Import them like Python modules:

### Math

```python
from pyths import math

print(math.pi)                # 3.141592653589793
print(math.sqrt(16))          # 4
print(math.factorial(5))      # 120
print(math.gcd(12, 8))        # 4
```

### JSON

```python
from pyths import json

data = {"name": "Alice", "scores": [95, 87, 92]}
text = json.dumps(data, indent=2)
print(text)

parsed = json.loads(text)
print(parsed["name"])          # Alice
```

### Itertools

```python
from pyths import itertools

# Chain multiple iterables
combined = list(itertools.chain([1, 2], [3, 4], [5, 6]))
print(combined)                # [1, 2, 3, 4, 5, 6]

# Combinations
combos = list(itertools.combinations([1, 2, 3, 4], 2))
print(combos)                  # [[1,2], [1,3], [1,4], [2,3], [2,4], [3,4]]

# Group by
data = [("a", 1), ("a", 2), ("b", 3)]
for key, group in itertools.groupby(data, lambda x: x[0]):
    print(f"{key}: {list(group)}")
```

### Collections

```python
from pyths import collections

# Counter
words = ["apple", "banana", "apple", "cherry", "banana", "apple"]
counter = collections.Counter(words)
print(counter.most_common(2))  # [["apple", 3], ["banana", 2]]

# defaultdict
dd = collections.defaultdict(list)
dd["fruits"].append("apple")
dd["fruits"].append("banana")
print(dd["fruits"])            # ["apple", "banana"]

# deque
dq = collections.deque([1, 2, 3], maxlen=5)
dq.appendleft(0)
dq.append(4)
print(list(dq))                # [0, 1, 2, 3, 4]
```

### Web Fetch

```python
from pyths.fetch import get, post

async def load_data():
    response = await get("https://api.example.com/data")
    data = await response.json()
    return data

async def create_item(item):
    response = await post("https://api.example.com/items", json=item)
    response.raise_for_status()
    return await response.json()
```

### Storage

```python
from pyths.storage import local, session

# localStorage
local.set("user", {"name": "Alice", "theme": "dark"})
user = local.get("user")
print(user["name"])            # Alice

# sessionStorage
session.set("token", "abc123")
```

## Linting

PythScribe's linter catches common issues:

```bash
pyths lint src/
```

Rules:
- **W001** — Unused variable
- **W002** — Unused import
- **W003** — Unreachable code after return/break
- **W004** — Naming convention violations
- **W005** — Unnecessary `pass` in non-empty block
- **W006** — Mutable default argument

## Testing

Write tests in `test_*.ps` or `*_test.ps` files:

```python
# test_math.ps
def test_add():
    assert 1 + 1 == 2

def test_string():
    name = "PythScribe"
    assert len(name) == 10
    assert name.upper() == "PYTHSCRIBE"
```

```bash
pyths test
```

## Bundling

Bundle a project with all its local imports into a single JS file:

```bash
pyths bundle app.ps -o dist/app.js
pyths bundle app.ps -o dist/app.min.js --minify
```

## Decorators

PythScribe supports generic decorators — any `@decorator` compiles to `fn = decorator(fn)`. Built-in decorators like `@component`, `@dataclass`, and `@staticmethod` have special compile-time behavior, while all others are applied at runtime.

### Built-in Decorators

| Decorator | Module | Description |
|-----------|--------|-------------|
| `@component` | `pyths.react` | Marks a function as a React component, enables PSX |
| `@dataclass` | `dataclasses` | Auto-generates constructor, toString, __eq__, toDict/fromDict |
| `@staticmethod` | (builtin) | Emits a `static` method in JS classes |
| `@validator` | `dataclasses` | Runs validation after dataclass construction |

### Standard Library Decorators

```python
from functools import lru_cache, cache

@lru_cache(maxsize=128)
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n - 1) + fibonacci(n - 2)

# Results are cached — subsequent calls with the same arg are instant
print(fibonacci(100))
```

### @retry — Resilient Functions (tenacity-style)

PythScribe provides a built-in `@retry` decorator inspired by Python's [tenacity](https://github.com/jd/tenacity) library:

```python
from pyths.utils.tenacity import retry

# Simple retry — up to 3 attempts
@retry()
def connect():
    db = open_connection()
    return db

# Retry with fixed delay
@retry(max_attempts=5, delay=1.0)
async def fetch_data(url):
    response = await get(url)
    return response.json()

# Exponential backoff — 1s, 2s, 4s, 8s...
@retry(max_attempts=5, delay=1.0, backoff="exponential")
async def call_api(endpoint):
    response = await get(endpoint)
    response.raise_for_status()
    return response

# Retry only on specific errors
@retry(max_attempts=3, retry_on=TypeError)
def parse_input(data):
    return process(data)

# With callback on each retry
@retry(max_attempts=3, delay=0.5, on_retry=lambda err, n: print(f"Attempt {n} failed: {err}"))
async def send_message(msg):
    await post("/api/messages", json=msg)
```

#### @retry Options

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_attempts` | 3 | Maximum number of attempts |
| `delay` | 0 | Base delay between retries (seconds) |
| `backoff` | `"fixed"` | `"fixed"` or `"exponential"` |
| `multiplier` | 2 | Multiplier for exponential backoff |
| `max_delay` | 60 | Maximum delay cap (seconds) |
| `retry_on` | all errors | Error class or list of error classes to retry on |
| `on_retry` | None | `callback(error, attempt)` called before each retry |
| `reraise` | True | Re-throw the last error when all attempts fail |

### Custom Decorators

You can write your own decorators in `.ps` files — they compile to standard JS wrapper functions:

```python
# decorators.ps
def log_calls(fn):
    def wrapper(*args):
        print(f"Calling {fn.name} with {args}")
        result = fn(*args)
        print(f"{fn.name} returned {result}")
        return result
    return wrapper

def require_auth(fn):
    def wrapper(*args):
        if not is_authenticated():
            raise Error("Not authenticated")
        return fn(*args)
    return wrapper

# Usage
@log_calls
@require_auth
def delete_user(user_id):
    api.delete(f"/users/{user_id}")
```

Decorators with arguments work too:

```python
def rate_limit(max_calls, period):
    def decorator(fn):
        calls = []
        def wrapper(*args):
            now = Date.now()
            calls[:] = [t for t in calls if now - t < period * 1000]
            if len(calls) >= max_calls:
                raise Error("Rate limit exceeded")
            calls.append(now)
            return fn(*args)
        return wrapper
    return decorator

@rate_limit(max_calls=10, period=60)
def search(query):
    return fetch_results(query)
```

### Planned Utility Decorators

Future releases will add more decorators to `pyths.utils`:

| Decorator | Description |
|-----------|-------------|
| `@debounce(wait)` | Delay execution until input settles |
| `@throttle(interval)` | Limit execution frequency |
| `@deprecated(message)` | Warn when a function is called |
| `@timeout(seconds)` | Abort if function takes too long |
| `@validate_args` | Validate function arguments from type annotations |

## Next Steps

- [Language Reference](../language-reference.md) — Complete syntax documentation
- [API Reference](../api-reference.md) — All standard library modules
- [Migration Guide](../migration-guide.md) — Coming from Python
