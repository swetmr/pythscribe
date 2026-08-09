// Stable JSON projection of the rendered DOM tree under #root.
// Used by parity-dom.spec.ts to compare PythScribe output against the
// React-equivalent output deterministically. Strips React-internal
// attributes and normalizes whitespace.

export function serializeRootDom() {
  const root = document.getElementById("root");
  if (!root || !root.firstChild) return null;
  return serializeNode(root.firstChild);
}

function serializeNode(node) {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = (node.textContent || "").replace(/\s+/g, " ").trim();
    return text ? { type: "text", value: text } : null;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return null;

  const tag = node.tagName.toLowerCase();

  // Pull a deterministic, sorted attribute map. Drop React/Hydration
  // bookkeeping attributes that don't affect rendering.
  const attrs = {};
  for (const a of Array.from(node.attributes)) {
    const name = a.name;
    if (name.startsWith("data-react")) continue;
    if (name === "data-reactroot") continue;
    attrs[name] = a.value;
  }
  const sortedAttrs = {};
  for (const k of Object.keys(attrs).sort()) sortedAttrs[k] = attrs[k];

  const children = [];
  for (const ch of Array.from(node.childNodes)) {
    const s = serializeNode(ch);
    if (s) children.push(s);
  }

  return { type: "element", tag, attrs: sortedAttrs, children };
}

// Concatenate visible text content for a quick text-only comparison.
export function visibleText() {
  const root = document.getElementById("root");
  if (!root) return "";
  return (root.textContent || "").replace(/\s+/g, " ").trim();
}
