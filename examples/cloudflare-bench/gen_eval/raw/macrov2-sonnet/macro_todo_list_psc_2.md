<!-- exp=macrov2-sonnet task=macro_todo_list cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.0168828 -->
```python
"use client"
R*

@c
def TodoApp():
    todos, set_todos = us([])
    text, set_text = us("")

    def add():
        if text.strip():
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    remaining = len([t for t in todos if not t["done"]])

    return div(cn="todo-app",
        h2("Todo List"),
        div(cn="todo-input",
            input(value=text, oh=lambda e: set_text(e.target.value),
                  ph="New todo...", type="text"),
            button(oc=lambda: add(), "Add"),
        ),
        ul(cn="todo-list",
            *[li(key=i, cn="todo-item done" if t["done"] else "todo-item",
                 oc=lambda: toggle(i), t["text"])
              for i, t in enumerate(todos)]),
        p(cn="todo-footer", f"{remaining} remaining"),
    )
```
