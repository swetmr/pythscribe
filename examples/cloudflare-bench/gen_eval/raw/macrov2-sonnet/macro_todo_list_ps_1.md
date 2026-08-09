<!-- exp=macrov2-sonnet task=macro_todo_list cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0366846 -->
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

    remaining = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h2("Todo List"),
        div(class_name="todo-input",
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder="New todo..."),
            button(on_click=lambda: add(), "Add"),
        ),
        ul(class_name="todo-list",
            *[li(key=i, class_name="done" if t["done"] else "",
                 on_click=lambda: toggle(i), t["text"])
              for i, t in enumerate(todos)]),
        p(class_name="todo-footer", f"{remaining} remaining"),
    )
```
