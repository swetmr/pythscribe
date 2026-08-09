<!-- exp=macrov2-sonnet task=macro_todo_list cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.017086200000000003 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([
        {"text": "Learn PythScribe", "done": True},
        {"text": "Build a todo app", "done": False},
    ])
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
        h2("Todo List"),
        div(class_name="todo-input",
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder="New todo..."),
            button(on_click=lambda: add(), "Add"),
        ),
        ul(class_name="todo-list",
            *[li(key=i, on_click=lambda: toggle(i),
                 class_name="done" if t["done"] else "pending",
                 t["text"])
              for i, t in enumerate(todos)]),
        p(class_name="todo-footer", f"{remaining} remaining"),
    )
```
