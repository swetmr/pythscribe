<!-- exp=macrov2-haiku task=macro_todo_list cond=ps sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0314379 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add():
        if text:
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
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder="Enter a new todo...", class_name="todo-input"),
            button(on_click=lambda: add(), class_name="add-button", "Add Todo"),
        ),
        ul(class_name="todo-list",
            *[li(key=i, on_click=lambda: toggle(i),
                  class_name="todo-item" + (" completed" if t["done"] else ""),
                  t["text"])
              for i, t in enumerate(todos)],
        ),
        div(class_name="footer",
            p(f"{pending} todo{'s' if pending != 1 else ''} remaining"),
        ),
    )
```
