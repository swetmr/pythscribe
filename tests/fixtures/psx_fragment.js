"use client";
import { createElement, Fragment } from "react";
import { useState } from "react";
export default function TodoApp() {
    const [items, set_items] = useState([]);
    const [text, set_text] = useState("");
    return createElement(Fragment, null, createElement("div", {className: "todo-app"}, createElement("h1", null, "Todo List"), createElement("input", {value: text, onChange: (e) => set_text(e.target.value)}), createElement("ul", null, ...items.map((item) => createElement("li", null, item)))), createElement("footer", null, createElement("p", null, "Made with PythScribe")));
}
