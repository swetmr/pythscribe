<!-- exp=macrov2-sonnet task=macro_todo_list cond=psc sample=1 model=claude-sonnet-5 cost_usd=0.0455616 -->
```python
"use client"
R*

@c
def TodoApp():
    todos, set_todos = us([])
    text, set_text = us("")

    def add():
        if text:
            set_todos([*todos, {"text": text, "done": False}])
            set_text("")

    def toggle(i):
        updated = [t for t in todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)

    pending = len([t for t in todos if not t["done"]])

    return div(cn="todo-app",
        h2("Todo List"),
        div(cn="todo-input",
            input(value=text, oh=lambda e: set_text(e.target.value),
                  ph="New todo...", on_key_down=lambda e: add() if e.key == "Enter" else None),
            button(oc=lambda: add(), "Add"),
        ),
        ul(cn="todo-list", *[
            li(key=i, cn="todo-item done" if t["done"] else "todo-item",
               oc=lambda: toggle(i), t["text"])
            for i, t in enumerate(todos)
        ]),
        p(cn="todo-footer", f"{pending} remaining"),
    )
```
