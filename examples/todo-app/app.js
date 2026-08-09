import { pyLen } from "pyths-runtime";

class Todo {
    constructor(title, done) {
        this.title = title;
        this.done = done;
    }
    toString() {
        let status = (this.done ? "x" : " ");
        return `[${status}] ${this.title}`;
    }
    toggle() {
        this.done = (!this.done);
    }
}
let todos = [new Todo("Learn PythScribe", false), new Todo("Build a web app", false), new Todo("Ship to production", false)];
todos[0].toggle();
for (const todo of todos) {
    console.log(todo);
}
let pending = todos.filter((t) => (!t.done)).map((t) => t);
console.log(`\\n${pyLen(pending)} items remaining`);
