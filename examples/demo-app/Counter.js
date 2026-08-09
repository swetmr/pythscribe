import { createElement } from "react";
import { component, use_state } from "pyths-runtime/react";
export default function Counter() {
    const [count, set_count] = useState(0);
    function click() {
        set_count((count + 1));
    }
    return createElement("div", null, createElement("h1", null, `Count: ${count}`), createElement("button", {onClick: click})("Increment"));
}
