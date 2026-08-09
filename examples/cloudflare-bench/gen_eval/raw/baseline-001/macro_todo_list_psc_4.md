<!-- exp=baseline-001 task=macro_todo_list cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.022488 -->
```python
"use client"
R*

@c
def TodoApp():
    todos, set_todos = us([
        {"text": "Learn PythScribe", "done": True},
        {"text": "Write a todo app", "done": False},
    ])
    text, set_text = us("")

    def add():
        if text.strip():
            set_todos([*todos, {"text": text.strip(), "done": False}])
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
                  ph="New todo..."),
            button(oc=lambda: add(), "Add"),
        ),
        ul(cn="todo-list",
            *[li(key=i, cn="done" if t["done"] else "pending",
                 oc=lambda i=i: toggle(i), t["text"])
              for i, t in enumerate(todos)]),
        footer(cn="todo-footer", f"{remaining} remaining"),
    )
```
