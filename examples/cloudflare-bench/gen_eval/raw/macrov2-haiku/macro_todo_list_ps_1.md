<!-- exp=macrov2-haiku task=macro_todo_list cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.014863600000000001 -->
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
        h2("Todo List"),
        input(value=text, on_change=lambda e: set_text(e.target.value),
              placeholder="Add a new todo..."),
        button(on_click=lambda: add(), "Add"),
        ul(*[li(key=i,
                class_name="todo-item" + (" done" if t["done"] else ""),
                on_click=lambda: toggle(i),
                t["text"])
             for i, t in enumerate(todos)]),
        p(f"{pending} todo{'s' if pending != 1 else ''} remaining"),
    )
```
