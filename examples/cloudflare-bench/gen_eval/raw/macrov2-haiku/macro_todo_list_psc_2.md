<!-- exp=macrov2-haiku task=macro_todo_list cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0230974 -->
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
        set_todos([{**t, "done": not t["done"]} if j == i else t 
                   for j, t in enumerate(todos)])

    pending = len([t for t in todos if not t["done"]])

    return div(cn="todo-app",
        h1("Todo List"),
        input(value=text, oh=lambda e: set_text(e.target.value), ph="Add a new todo..."),
        button(oc=lambda: add(), "Add"),
        ul(cn="todo-list",
            *[li(key=i, oc=lambda: toggle(i), 
                 cn="todo-item" + (" done" if t["done"] else ""),
                 t["text"])
              for i, t in enumerate(todos)]
        ),
        p(cn="footer", f"{pending} remaining"),
    )
```
