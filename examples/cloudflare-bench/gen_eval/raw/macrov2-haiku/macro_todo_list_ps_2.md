<!-- exp=macrov2-haiku task=macro_todo_list cond=ps sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0080529 -->
I'll invoke the brainstorming skill first to establish the approach before implementing.

<details open>
<summary><strong>Using superpowers:brainstorming to outline TodoApp structure</strong></summary>

**Clarify the task**
- Build a React-style TodoApp component in PythScribe
- Core features: input field, Add button, todo list with toggle-done on click, footer showing pending count
- All in one file, no CSS imports, use class names for styling

**Brainstorm approach**
1. Component state: `todos` list (each todo has `text` and `done` fields), `text` input state
2. Input field binding: `value=text`, `on_change` updates text state
3. Add button: validates non-empty, appends to todos, clears input
4. Todo list: map over todos, render clickable items, toggle `done` on click
5. Footer: count `len([t for t in todos if not t["done"]])`
6. Styling: vanilla class names for layout/styling (no external CSS needed)

**Implementation plan**
- Use `@component` decorator, `use_state` hooks
- Straightforward if/elif flow
- Simple CSS class naming for styling

This is straightforward — no architecture decisions needed. Proceed with implementation.

</details>

```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        if text.strip():
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h1("Todo App"),
        div(class_name="input-section",
            input(
                value=text,
                on_change=lambda e: set_text(e.target.value),
                placeholder="Add a new todo...",
                class_name="todo-input",
            ),
            button(on_click=lambda: add(), class_name="add-btn", "Add"),
        ),
        ul(class_name="todo-list",
            *[li(
                key=i,
                on_click=lambda: toggle(i),
                class_name=f"todo-item {'done' if t['done'] else ''}",
                t["text"],
            ) for i, t in enumerate(todos)]
        ),
        div(class_name="footer",
            p(f"{pending} task{'s' if pending != 1 else ''} remaining"),
        ),
    )
```
