#!/usr/bin/env node

import { mkdirSync, writeFileSync } from "node:fs";
import { resolve, join, isAbsolute, relative } from "node:path";

const projectName = process.argv[2] || "my-pyths-app";

// A15 hardening: confine the scaffold target to a subdirectory of the CWD.
// Reject absolute paths and any `..` escape (e.g. `../../../tmp/x`) so the
// scaffolder cannot write files outside the directory it was invoked in.
if (isAbsolute(projectName)) {
    console.error(
        `\nError: project name must be a relative path inside the current directory, not an absolute path: ${projectName}\n`
    );
    process.exit(1);
}
const projectDir = resolve(process.cwd(), projectName);
const rel = relative(process.cwd(), projectDir);
if (rel === "" || rel.startsWith("..") || isAbsolute(rel)) {
    console.error(
        `\nError: project directory must be inside the current directory. "${projectName}" resolves outside it.\n`
    );
    process.exit(1);
}

console.log(`\nCreating PythScribe app in ${projectDir}...\n`);

// Create directory structure
const dirs = [
    "",
    "app",
    "components",
    "public",
];

for (const dir of dirs) {
    mkdirSync(join(projectDir, dir), { recursive: true });
}

// package.json
writeFileSync(
    join(projectDir, "package.json"),
    JSON.stringify(
        {
            name: projectName,
            version: "0.1.0",
            private: true,
            type: "module",
            scripts: {
                dev: "next dev",
                build: "next build",
                start: "next start",
            },
            dependencies: {
                next: "^16.2.9",
                react: "^19.2.0",
                "react-dom": "^19.2.0",
                pythscribe: "^0.2.1",
                "pyths-runtime": "^0.2.1",
                "next-plugin-pyths": "^0.2.1",
            },
        },
        null,
        2
    ) + "\n"
);

// next.config.mjs
writeFileSync(
    join(projectDir, "next.config.mjs"),
    `import withPyths from "next-plugin-pyths";

/** @type {import('next').NextConfig} */
const nextConfig = {};

export default withPyths(nextConfig);
`
);

// app/layout.ps
writeFileSync(
    join(projectDir, "app", "layout.ps"),
    `def metadata():
    return {"title": "PythScribe App", "description": "Built with PythScribe"}

@component
def RootLayout(children):
    return html(lang="en",
        body(children),
    )
`
);

// app/page.ps
writeFileSync(
    join(projectDir, "app", "page.ps"),
    `"use client"

from react import use_state

@component
def Home():
    count, set_count = use_state(0)

    return main(class_name="container",
        h1("Welcome to PythScribe!"),
        p("Write Python. Ship to the Web."),
        div(class_name="counter",
            p(f"Count: {count}"),
            button(on_click=lambda: set_count(count + 1), "Increment"),
            button(on_click=lambda: set_count(count - 1), "Decrement"),
        ),
    )
`
);

// components/header.ps
writeFileSync(
    join(projectDir, "components", "header.ps"),
    `"use client"

@component
def Header(title):
    return header(class_name="header",
        nav(
            h1(title),
            ul(
                li(a(href="/", "Home")),
                li(a(href="/about", "About")),
            ),
        ),
    )
`
);

console.log("Created project files:");
console.log("  package.json");
console.log("  next.config.mjs");
console.log("  app/layout.ps");
console.log("  app/page.ps");
console.log("  components/header.ps");
console.log("");
console.log("Get started:");
console.log(`  cd ${projectName}`);
console.log("  npm install");
console.log("  npm run dev");
console.log("");
