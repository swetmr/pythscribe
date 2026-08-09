# Tutorial 2: Building a Todo App

This tutorial builds a complete todo application, demonstrating classes, lists, f-strings, and control flow.

## The Todo Class

Create `todo.ps`:

```python
class Todo:
    def __init__(self, title, done):
        self.title = title
        self.done = done

    def __str__(self):
        status = "x" if self.done else " "
        return f"[{status}] {self.title}"

    def toggle(self):
        self.done = not self.done
```

This class has:
- A constructor storing `title` and `done` status
- `__str__` for display (compiles to `toString()`)
- `toggle` to flip completion state

## Building the App

Add the rest of the app:

```python
class Todo:
    def __init__(self, title, done):
        self.title = title
        self.done = done

    def __str__(self):
        status = "x" if self.done else " "
        return f"[{status}] {self.title}"

    def toggle(self):
        self.done = not self.done

class TodoApp:
    def __init__(self):
        self.todos = []

    def add(self, title):
        self.todos.append(Todo(title, False))

    def complete(self, index):
        self.todos[index].toggle()

    def remove(self, index):
        self.todos.pop(index)

    def pending(self):
        return [t for t in self.todos if not t.done]

    def display(self):
        if len(self.todos) == 0:
            print("No todos yet!")
            return

        for i, todo in enumerate(self.todos):
            print(f"  {i + 1}. {todo}")

        p = self.pending()
        print(f"\n{len(p)} of {len(self.todos)} items remaining")

# Use it
app = TodoApp()
app.add("Learn PythScribe")
app.add("Build a web app")
app.add("Ship to production")

app.complete(0)

print("=== My Todos ===")
app.display()
```

Run it:
```bash
pyths run todo.ps
```

```
=== My Todos ===
  1. [x] Learn PythScribe
  2. [ ] Build a web app
  3. [ ] Ship to production

2 of 3 items remaining
```

## Using Dataclasses

The same Todo can be written more concisely with `@dataclass`:

```python
from dataclasses import dataclass

@dataclass
class Todo:
    title: str
    done: bool = False

    def __str__(self):
        status = "x" if self.done else " "
        return f"[{status}] {self.title}"

    def toggle(self):
        self.done = not self.done

# @dataclass auto-generates:
# - Constructor with type validation
# - __eq__ method
# - toDict() / fromDict()
# - toString()

todo = Todo("Learn PythScribe")
print(todo)
print(todo.toDict())  # {"title": "Learn PythScribe", "done": false}
```

The `@dataclass` decorator generates a constructor with type validation, equality checking, and serialization — all from just the field annotations.

## Compiling the Output

```bash
pyths compile todo.ps -o todo.js
```

Inspect the compiled JavaScript:
```bash
pyths compile todo.ps --stdout
```

Notice how PythScribe translates:
- `self.x` → `this.x`
- `__init__` → `constructor`
- `__str__` → `toString()`
- `f"..."` → template literals
- `not x` → `!x`
- `len(x)` → `pyLen(x)` (runtime helper)

## Type Checking

Add type annotations and check them:

```python
class Todo:
    def __init__(self, title: str, done: bool = False):
        self.title = title
        self.done = done

    def toggle(self) -> None:
        self.done = not self.done
```

```bash
pyths check todo.ps
# ✓ todo.ps — no errors
```

## Next Steps

- [Tutorial 3: React Components](03-react-components.md) — Make this interactive in the browser
- [Tutorial 4: Advanced Features](04-advanced-features.md) — Pattern matching, generators, and more
