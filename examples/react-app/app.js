import { pyLen, pyRange } from "pyths-runtime";

import { useState, createElement as h } from "react";
function TodoItem(props) {
    let todo = props.todo;
    let on_toggle = props.onToggle;
    return h("li", ({"onClick": on_toggle, "style": ({"cursor": "pointer"})}), h("span", null, (todo.done ? "[x] " : "[ ] ")), h("span", ({"style": ({"textDecoration": (todo.done ? "line-through" : "none")})}), todo.text));
}
function TodoApp() {
    const [todos, set_todos] = useState([({"text": "Learn PythScribe", "done": false}), ({"text": "Build with React", "done": false}), ({"text": "Ship to production", "done": false})]);
    const [new_text, set_new_text] = useState("");
    function toggle(index) {
        let updated = todos.map((t) => t);
        let item = updated[index];
        updated[index] = ({"text": item.text, "done": (!item.done)});
        set_todos(updated);
    }
    function add_todo() {
        if (new_text) {
            set_todos([...todos, ({"text": new_text, "done": false})]);
            set_new_text("");
        }
    }
    let pending = pyLen(todos.filter((t) => (!t.done)).map((t) => t));
    return h("div", ({"className": "app"}), h("h1", null, "PythScribe + React Todo"), h("div", null, h("input", ({"value": new_text, "onChange": (e) => set_new_text(e.target.value), "placeholder": "Add a todo..."})), h("button", ({"onClick": (e) => add_todo()}), "Add")), h("ul", null, ...pyRange(pyLen(todos)).map((i) => h(TodoItem, ({"key": i, "todo": todos[i], "onToggle": (e) => toggle(i)})))), h("p", null, `${pending} items remaining`));
}
let export = TodoApp;
