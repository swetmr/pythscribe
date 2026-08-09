<!-- exp=baseline-sonnet task=macro_todo_list cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.018460499999999998 -->
```python
"use client"
R*

@c
def TodoApp():
    todos, set_todos = us([
        {"id": 1, "text": "Learn PythScribe", "done": True},
        {"id": 2, "text": "Build a todo app", "done": False},
        {"id": 3, "text": "Ship it", "done": False},
    ])
    text, set_text = us("")

    def add():
        trimmed = text.strip()
        if trimmed:
            set_todos([*todos, {"id": len(todos) and max(t["id"] for t in todos) + 1 or 1, "text": trimmed, "done": False}])
            set_text("")

    def toggle(todo_id):
        set_todos([{**t, "done": not t["done"]} if t["id"] == todo_id else t for t in todos])

    remaining = len([t for t in todos if not t["done"]])

    return div(cn="todo-app",
        h2("Todo List"),
        div(cn="todo-input-row",
            input(value=text, oh=lambda e: set_text(e.target.value), ph="What needs doing?"),
            button(oc=add, "Add"),
        ),
        ul(cn="todo-list",
            *[li(key=t["id"], cn="todo-item done" if t["done"] else "todo-item",
                 oc=lambda t=t: toggle(t["id"]), t["text"])
              for t in todos],
        ),
        p(cn="todo-footer", f"{remaining} item{'s' if remaining != 1 else ''} remaining"),
    )
```
