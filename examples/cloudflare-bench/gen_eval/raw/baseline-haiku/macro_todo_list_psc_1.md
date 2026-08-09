<!-- exp=baseline-haiku task=macro_todo_list cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.031632299999999995 -->
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
        updated = [*todos]
        updated[i] = {**updated[i], "done": not updated[i]["done"]}
        set_todos(updated)
    
    remaining = len([t for t in todos if not t["done"]])
    
    return div(cn="todo-app",
        h2("Todo List"),
        input(value=text, oh=lambda e: set_text(e.target.value), ph="New todo..."),
        button(oc=lambda: add(), "Add"),
        ul(*[li(key=i, oc=lambda idx=i: toggle(idx), t["text"])
             for i, t in enumerate(todos)]),
        p(f"{remaining} remaining"),
    )
```
