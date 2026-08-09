<!-- exp=macrov2-opus task=macro_todo_list cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.02321 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([
        {"text": "Read the manual", "done": True},
        {"text": "Write a todo app", "done": False},
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
                  on_key_down=lambda e: add() if e.key == "Enter" else None,
                  placeholder="New todo..."),
            button(on_click=lambda: add(), "Add"),
        ),
        ul(class_name="todo-list",
            *[li(key=i, class_name="done" if t["done"] else "pending",
                 on_click=lambda: toggle(i), t["text"])
              for i, t in enumerate(todos)]),
        p(class_name="footer", f"{remaining} remaining"),
    )
```
