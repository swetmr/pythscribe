<!-- exp=baseline-haiku task=macro_todo_list cond=ps sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.027908299999999997 -->
```python
"use client"

from pyths.react import component, use_state

@component
def TodoApp():
    todos, set_todos = use_state([])
    text, set_text = use_state("")

    def add_todo():
        if text.strip():
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle_todo(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(class_name="todo-app",
        h1("Todo App"),
        div(class_name="input-container",
            input(value=text, on_change=lambda e: set_text(e.target.value),
                  placeholder="Enter a new todo...", class_name="todo-input"),
            button(on_click=lambda: add_todo(), class_name="add-btn", "Add"),
        ),
        ul(class_name="todo-list",
           *[li(key=i, on_click=lambda idx=i: toggle_todo(idx),
                class_name="todo-item" + (" done" if t["done"] else ""),
                t["text"])
             for i, t in enumerate(todos)]
        ),
        div(class_name="footer",
            p(f"{pending} todo{'s' if pending != 1 else ''} remaining")
        ),
    )
```
