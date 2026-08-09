"use client";
import { createElement } from "react";
import { useState } from "react";
export default function Counter() {
    const [count, set_count] = useState(0);
    return createElement("div", {className: "counter"}, createElement("h1", null, "Counter App"), createElement("p", null, `Count: ${count}`), createElement("button", {onClick: () => set_count((count + 1))}, "Increment"), createElement("button", {onClick: () => set_count((count - 1))}, "Decrement"));
}
