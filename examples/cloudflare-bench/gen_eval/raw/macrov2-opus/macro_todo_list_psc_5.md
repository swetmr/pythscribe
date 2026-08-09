<!-- exp=macrov2-opus task=macro_todo_list cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.018446 -->
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
        input(value=text, oh=lambda e: set_text(e.target.value), ph="New todo..."),
        button(oc=lambda: add(), "Add"),
        ul(*[li(key=i, cn="done" if t["done"] else "pending", oc=lambda: toggle(i), t["text"])
             for i, t in enumerate(todos)]),
        p(cn="footer", f"{pending} remaining"),
    )
```
