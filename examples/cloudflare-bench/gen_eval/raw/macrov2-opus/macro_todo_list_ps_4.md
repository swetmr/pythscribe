<!-- exp=macrov2-opus task=macro_todo_list cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.025535000000000002 -->
```python
"use client"

from pyths.react import component, use_state

INITIAL_TODOS = [
    {"text": "Buy groceries", "done": False},
    {"text": "Read a book", "done": True},
    {"text": "Write PythScribe", "done": False},
]

@component
def TodoApp():
    todos, set_todos = use_state(INITIAL_TODOS)
    text, set_text = use_state("")

    def add():
        trimmed = text.strip()
        if trimmed:
            set_todos([*todos, {"text": trimmed, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    remaining = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h2(class_name="todo-title", "Todo List"),
        div(class_name="todo-input-row",
            input(
                class_name="todo-input",
                value=text,
                placeholder="New todo...",
                on_change=lambda e: set_text(e.target.value),
                on_key_down=lambda e: add() if e.key == "Enter" else None,
            ),
            button(class_name="todo-add", on_click=lambda: add(), "Add"),
        ),
        ul(class_name="todo-list",
            *[li(
                key=i,
                class_name="todo-item done" if t["done"] else "todo-item",
                on_click=lambda: toggle(i),
                t["text"],
            ) for i, t in enumerate(todos)],
        ),
        p(class_name="todo-footer", f"{remaining} remaining"),
    )
```
