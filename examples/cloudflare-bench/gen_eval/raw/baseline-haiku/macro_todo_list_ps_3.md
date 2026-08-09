<!-- exp=baseline-haiku task=macro_todo_list cond=ps sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.0165807 -->
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
        input(
            value=text,
            on_change=lambda e: set_text(e.target.value),
            placeholder="New todo..."
        ),
        button(on_click=add, "Add"),
        ul(*[li(
            key=i,
            on_click=lambda idx=i: toggle(idx),
            class_name="todo-item" + (" done" if todos[i]["done"] else ""),
            todos[i]["text"]
        ) for i in range(len(todos))]),
        p(f"{pending} remaining"),
    )
```
