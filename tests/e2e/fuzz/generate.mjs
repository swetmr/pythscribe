// Spec → (.ps, .tsx) generator for the DOM-parity fuzzer.
//
// Reads JSON specs from tests/e2e/fuzz/specs/ and emits matching .ps
// (PythScribe) and .tsx (React reference) files into
// tests/e2e/fuzz/generated/. Each fixture should produce structurally
// identical DOM trees when rendered — the Playwright suite at
// tests/e2e/tests/fuzz.spec.ts then proves it.
//
// Each generator below is paired: pythscribeFor* and reactFor*
// produce semantically identical output. Text content uses single
// concatenated strings on both sides (so DOM text-node count matches).
//
// Run: node tests/e2e/fuzz/generate.mjs

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const SPECS_DIR = path.join(__dirname, "specs");
const OUT_DIR = path.join(__dirname, "generated");

// =====================================================================
// Per-shape generators. Each returns a `{ps, tsx, exportName}` triple.
// =====================================================================

function genList({ title, count }) {
    const ps = `from pyths.react import component

@component
def App():
    return div()(
        h1()("${title}"),
        ul()(
            ${Array.from({ length: count }).map((_, i) =>
                `li()(f"item ${i + 1}")`).join(",\n            ")}
        )
    )
`;
    const tsx = `export function App() {
  return (
    <div>
      <h1>${title}</h1>
      <ul>
        ${Array.from({ length: count }).map((_, i) =>
            `<li>{\`item ${i + 1}\`}</li>`).join("\n        ")}
      </ul>
    </div>
  );
}
`;
    return { ps, tsx, exportName: "App" };
}

function genTable({ rows, cols }) {
    const psRows = Array.from({ length: rows }).map((_, r) => {
        const cells = Array.from({ length: cols }).map((_, c) =>
            `td()(f"r${r}c${c}")`).join(",\n            ");
        return `tr()(\n            ${cells}\n        )`;
    }).join(",\n        ");
    const tsxRows = Array.from({ length: rows }).map((_, r) => {
        const cells = Array.from({ length: cols }).map((_, c) =>
            `<td>{\`r${r}c${c}\`}</td>`).join("\n          ");
        return `<tr>\n          ${cells}\n        </tr>`;
    }).join("\n        ");
    const ps = `from pyths.react import component

@component
def App():
    return table()(
        ${psRows}
    )
`;
    const tsx = `export function App() {
  return (
    <table>
        ${tsxRows}
    </table>
  );
}
`;
    return { ps, tsx, exportName: "App" };
}

function genGrid({ outer, inner }) {
    const psOuter = Array.from({ length: outer }).map((_, o) => {
        const psInner = Array.from({ length: inner }).map((_, i) =>
            `span()(f"o${o}i${i}")`).join(",\n            ");
        return `div(className="row")(\n            ${psInner}\n        )`;
    }).join(",\n        ");
    const tsxOuter = Array.from({ length: outer }).map((_, o) => {
        const tsxInner = Array.from({ length: inner }).map((_, i) =>
            `<span>{\`o${o}i${i}\`}</span>`).join("\n          ");
        return `<div className="row">\n          ${tsxInner}\n        </div>`;
    }).join("\n        ");
    const ps = `from pyths.react import component

@component
def App():
    return div(className="grid")(
        ${psOuter}
    )
`;
    const tsx = `export function App() {
  return (
    <div className="grid">
        ${tsxOuter}
    </div>
  );
}
`;
    return { ps, tsx, exportName: "App" };
}

function genForm({ fields }) {
    const psFields = fields.map(f =>
        `div()(label()(f"${f}"), input(name=f"${f}"))`).join(",\n        ");
    const tsxFields = fields.map(f =>
        `<div><label>{\`${f}\`}</label><input name={\`${f}\`} /></div>`).join("\n      ");
    const ps = `from pyths.react import component

@component
def App():
    return form()(
        ${psFields}
    )
`;
    const tsx = `export function App() {
  return (
    <form>
      ${tsxFields}
    </form>
  );
}
`;
    return { ps, tsx, exportName: "App" };
}

const GENERATORS = {
    list: genList,
    table: genTable,
    grid: genGrid,
    form: genForm,
};

// =====================================================================
// Driver
// =====================================================================

async function main() {
    await fs.mkdir(OUT_DIR, { recursive: true });
    const entries = await fs.readdir(SPECS_DIR);
    const specs = entries.filter(n => n.endsWith(".json"));
    if (specs.length === 0) {
        console.log("[fuzz] no specs found");
        return;
    }

    const manifest = [];
    for (const file of specs) {
        const raw = await fs.readFile(path.join(SPECS_DIR, file), "utf8");
        const spec = JSON.parse(raw);
        const gen = GENERATORS[spec.kind];
        if (!gen) {
            console.error(`[fuzz] unknown kind: ${spec.kind} (in ${file})`);
            process.exit(1);
        }
        const { ps, tsx, exportName } = gen(spec.params);
        const psPath = path.join(OUT_DIR, `${spec.id}.ps`);
        const tsxPath = path.join(OUT_DIR, `${spec.id}.tsx`);
        await fs.writeFile(psPath, ps, "utf8");
        await fs.writeFile(tsxPath, tsx, "utf8");
        manifest.push({ id: spec.id, exportName });
        console.log(`[fuzz] generated ${spec.id} (${spec.kind})`);
    }
    await fs.writeFile(
        path.join(OUT_DIR, "manifest.json"),
        JSON.stringify(manifest, null, 2),
    );
    console.log(`[fuzz] wrote manifest with ${manifest.length} fixtures`);
}

main().catch((e) => { console.error(e); process.exit(1); });
