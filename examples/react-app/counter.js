import { useState, createElement as h } from "react";
function Counter() {
    const [count, set_count] = useState(0);
    function increment() {
        set_count((count + 1));
    }
    function decrement() {
        set_count((count - 1));
    }
    return h("div", null, h("h1", null, "PythScribe Counter"), h("p", null, `Count: ${count}`), h("button", ({"onClick": (e) => decrement()}), "-"), h("button", ({"onClick": (e) => increment()}), "+"));
}
