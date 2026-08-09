<!-- exp=baseline-haiku task=macro_todo_list cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.026364899999999997 -->
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
    
    pending = sum(1 for t in todos if not t["done"])
    
    return div(cn="todo-app",
        h2("Todo List"),
        input(value=text, oh=lambda e: set_text(e.target.value), ph="Enter a new todo..."),
        button(oc=lambda: add(), "Add"),
        ul(cn="todo-list",
            *[li(cn=f"todo-item {'done' if t['done'] else ''}", key=i, oc=lambda i=i: toggle(i), t["text"])
              for i, t in enumerate(todos)]
        ),
        div(cn="footer", f"{pending} remaining"),
    )
```
