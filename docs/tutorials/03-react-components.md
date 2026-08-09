# Tutorial 3: React Components with PythScribe

This tutorial shows how to build interactive React components using PythScribe's PSX syntax and hook integration.

## Prerequisites

- PythScribe compiler installed
- Node.js 18+
- A React or Next.js project (or use `create-pyths-app`)

## Scaffolding a Project

```bash
npx create-pyths-app my-app
cd my-app
npm install
```

Or add PythScribe to an existing Vite + React project:

```bash
npm install pyths-runtime vite-plugin-pyths
```

```js
// vite.config.js
import pyths from 'vite-plugin-pyths';
export default { plugins: [pyths()] };
```

## Your First Component

Create `src/Counter.ps`:

```python
from pyths.react import component, use_state

@component
def Counter():
    count, set_count = use_state(0)

    return div(class_name="counter",
        h2(f"Count: {count}"),
        button(on_click=lambda: set_count(count - 1), "-"),
        button(on_click=lambda: set_count(count + 1), "+"),
    )
```

### What's happening here?

1. **`@component`** — Marks the function as a React component and enables PSX
2. **`use_state`** — React's `useState` hook (PythScribe auto-converts snake_case to camelCase)
3. **PSX syntax** — HTML elements as function calls: `div(...)`, `h2(...)`, `button(...)`
4. **Props** — Keyword args: `class_name` becomes `className`, `on_click` becomes `onClick`
5. **String children** — Positional args: `"+"` becomes text content
6. **f-strings** — `f"Count: {count}"` becomes a template literal in the output

### Compiled output

```bash
pyths compile src/Counter.ps --stdout
```

```javascript
import { createElement } from "react";
import { useState } from "react";

export default function Counter() {
    const [count, set_count] = useState(0);
    return createElement("div", {className: "counter"},
        createElement("h2", null, `Count: ${count}`),
        createElement("button", {onClick: () => set_count(count - 1)}, "-"),
        createElement("button", {onClick: () => set_count(count + 1)}, "+")
    );
}
```

## Handling Events

```python
from pyths.react import component, use_state

@component
def LoginForm():
    username, set_username = use_state("")
    password, set_password = use_state("")
    error, set_error = use_state("")

    def handle_submit(e):
        e.preventDefault()
        if not username:
            set_error("Username is required")
            return
        if len(password) < 6:
            set_error("Password must be at least 6 characters")
            return
        set_error("")
        print(f"Logging in as {username}")

    return form(on_submit=handle_submit,
        h2("Login"),
        input(type="text", value=username,
            on_change=lambda e: set_username(e.target.value),
            placeholder="Username"),
        input(type="password", value=password,
            on_change=lambda e: set_password(e.target.value),
            placeholder="Password"),
        button(type="submit", "Log In"),
    )
```

## Lists and Iteration

Render lists using list comprehensions inside PSX:

```python
from pyths.react import component, use_state

@component
def TodoList():
    todos, set_todos = use_state([
        {"text": "Learn PythScribe", "done": True},
        {"text": "Build components", "done": False},
    ])
    text, set_text = use_state("")

    def add():
        if text:
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(index):
        updated = [t for t in todos]
        updated[index] = {**updated[index], "done": not updated[index]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(
        h2("Todo List"),
        div(
            input(value=text, on_change=lambda e: set_text(e.target.value),
                placeholder="New todo..."),
            button(on_click=lambda: add(), "Add"),
        ),
        ul(*[li(todo["text"], on_click=lambda: toggle(i)) for i, todo in enumerate(todos)]),
        p(f"{pending} items remaining"),
    )
```

## Using Effects

```python
from pyths.react import component, use_state, use_effect

@component
def Timer():
    seconds, set_seconds = use_state(0)
    running, set_running = use_state(False)

    def tick():
        if running:
            timer = setInterval(lambda: set_seconds(seconds + 1), 1000)
            return lambda: clearInterval(timer)

    use_effect(tick, [running, seconds])

    return div(
        h2(f"Timer: {seconds}s"),
        button(on_click=lambda: set_running(not running),
            "Stop" if running else "Start"),
        button(on_click=lambda: set_seconds(0), "Reset"),
    )
```

## Next.js Integration

### Client Components

```python
# src/components/ClientCounter.ps
"use client"

from pyths.react import component, use_state

@component
def ClientCounter():
    count, set_count = use_state(0)
    return button(on_click=lambda: set_count(count + 1),
        f"Clicked {count} times")
```

### Server Components and Data Fetching

```python
# src/app/page.ps
from pyths.react import component

def get_server_side_props(context):
    return {"props": {"title": "My Page"}}

@component
def Page(**props):  # `**props` = the whole props object; named params are prop names
    return main(
        h1(props["title"]),
        p("Built with PythScribe"),
    )
```

### Next.js Setup

```bash
npm install next-plugin-pyths
```

```js
// next.config.js
const withPythScribe = require('next-plugin-pyths');
module.exports = withPythScribe({});
```

## Hook Reference

| PythScribe | React | Description |
|----------|-------|-------------|
| `use_state(initial)` | `useState` | State management |
| `use_effect(fn, deps)` | `useEffect` | Side effects |
| `use_context(ctx)` | `useContext` | Context access |
| `use_ref(initial)` | `useRef` | Mutable ref |
| `use_memo(fn, deps)` | `useMemo` | Memoized value |
| `use_callback(fn, deps)` | `useCallback` | Memoized function |
| `use_reducer(reducer, init)` | `useReducer` | Complex state |
| `use_layout_effect(fn, deps)` | `useLayoutEffect` | Synchronous effect |
| `use_id()` | `useId` | Unique ID |

## PSX Prop Reference

Common prop conversions:

| PythScribe | React/HTML |
|----------|-----------|
| `class_name` | `className` |
| `on_click` | `onClick` |
| `on_change` | `onChange` |
| `on_submit` | `onSubmit` |
| `on_key_down` | `onKeyDown` |
| `tab_index` | `tabIndex` |
| `col_span` | `colSpan` |
| `html_for` | `htmlFor` |

All snake_case props are automatically converted to camelCase in the compiled output.

## Redux / State Management

PythScribe supports Redux Toolkit and React-Redux with Pythonic import names:

### Setting Up a Store

```python
# store.ps
from reduxjs.toolkit import create_slice, configure_store

counter_slice = create_slice(
    name="counter",
    initial_state={"value": 0},
    reducers={
        "increment": lambda state: state.value + 1,
        "decrement": lambda state: state.value - 1,
        "increment_by_amount": lambda state, action: state.value + action.payload,
    }
)

increment = counter_slice.actions.increment
decrement = counter_slice.actions.decrement
increment_by_amount = counter_slice.actions.increment_by_amount

store = configure_store(reducer={"counter": counter_slice.reducer})
```

### Using Redux in Components

```python
# counter.ps
from pyths.react import component
from react_redux import use_selector, use_dispatch
from store import increment, decrement

@component
def Counter():
    count = use_selector(lambda state: state.counter.value)
    dispatch = use_dispatch()

    return div(
        h1(f"Count: {count}"),
        button(on_click=lambda: dispatch(increment()), "Increment"),
        button(on_click=lambda: dispatch(decrement()), "Decrement"),
    )
```

### Async Thunks

```python
from reduxjs.toolkit import create_async_thunk, create_slice

fetch_users = create_async_thunk(
    "users/fetch",
    async lambda: await (await fetch("/api/users")).json()
)

users_slice = create_slice(
    name="users",
    initial_state={"items": [], "loading": False},
    extra_reducers=lambda builder: (
        builder.add_case(fetch_users.pending, lambda state: setattr(state, "loading", True)),
        builder.add_case(fetch_users.fulfilled, lambda state, action: (
            setattr(state, "loading", False),
            setattr(state, "items", action.payload),
        )),
    )
)
```

### Import Name Mapping

| PythScribe | npm Package |
|-------------|-------------|
| `react_redux` | `react-redux` |
| `reduxjs.toolkit` | `@reduxjs/toolkit` |

| PythScribe | JavaScript |
|-------------|------------|
| `use_selector` | `useSelector` |
| `use_dispatch` | `useDispatch` |
| `use_store` | `useStore` |
| `create_slice` | `createSlice` |
| `configure_store` | `configureStore` |
| `create_async_thunk` | `createAsyncThunk` |
| `create_reducer` | `createReducer` |
| `create_action` | `createAction` |
| `create_selector` | `createSelector` |

## Next Steps

- [Tutorial 4: Advanced Features](04-advanced-features.md) — Dataclasses, pattern matching, generators
- [Migration Guide](../migration-guide.md) — Full Python-to-PythScribe differences
- [API Reference](../api-reference.md) — Standard library modules
