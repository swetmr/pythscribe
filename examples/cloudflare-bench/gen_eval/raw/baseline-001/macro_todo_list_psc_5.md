<!-- exp=baseline-001 task=macro_todo_list cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.022988 -->
```python
"use client"
R*

def make_todo(text):
    return {"text": text, "done": False}

FIXTURES = [
    {"text": "Buy milk", "done": False},
    {"text": "Walk the dog", "done": True},
    {"text": "Write PythScribe", "done": False},
]

@c
def TodoApp():
    todos, set_todos = us(FIXTURES)
    text, set_text = us("")

    def add():
        trimmed = text.strip()
        if trimmed:
            set_todos([*todos, make_todo(trimmed)])
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
            *[li(key=i, cn="todo-item done" if t["done"] else "todo-item",
                 oc=lambda i=i: toggle(i), t["text"])
              for i, t in enumerate(todos)]),
        p(cn="todo-footer", f"{remaining} remaining"),
    )
```
