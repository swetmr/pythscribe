import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync, unlinkSync } from "node:fs";
import { dirname, resolve as resolvePath } from "node:path";

/**
 * Rewrite extensionless RELATIVE import specifiers in compiled output to
 * explicit `./X.psc` / `./X.ps` when a PythScribe sibling exists on disk.
 *
 * Why: bundler `resolve.extensions` order is GLOBAL and puts `.tsx`/`.ts`
 * before `.psc`/`.ps` (so plain `.tsx` imports keep working). But a module
 * *compiled from PythScribe* that imports `./Counter` means the PythScribe
 * `Counter.ps(c)` — in a dual-track app where a `Counter.tsx` oracle sits
 * beside it, global order silently resolves the `.tsx` instead. An explicit
 * extension bypasses resolve order on BOTH webpack and Turbopack and routes
 * the import through this loader. (The Vite plugin solves the same problem
 * with an importer-aware `resolveId` hook; webpack/Turbopack have no
 * importer-conditional resolution, so we rewrite the emitted specifier.)
 *
 * Only touches static `import`/`export ... from` specifiers that are
 * relative and have no extension; `.psc` is preferred over `.ps` (matching
 * the deploy-compressed convention).
 *
 * A14 hardening: the regex is anchored to line starts (`^\s*(?:import|export)`
 * with the `m` flag) so it matches only real top-level import/export
 * STATEMENTS, never an `import ... from "./x"` substring living inside a user
 * string literal (which previously had its data corrupted). Line-anchoring
 * also caps match-start positions at line beginnings, so the lazy `[^\n]*?`
 * no longer rescans from every `import`/`export` occurrence — removing the
 * O(L²) blowup on a long single line of compiled output.
 *
 * NOTE (follow-up): the ideal fix is to rewrite specifiers on the COMPILER
 * side (where the AST gives real lexical context), not by regex over emitted
 * text. That is out of scope here; tracked as a codegen enhancement.
 */
export function rewritePsImports(code, importerPath) {
    const dir = dirname(importerPath);
    return code.replace(
        /^(\s*(?:import|export)\b[^\n]*?\bfrom\s*["'])(\.[^"'\n]*?)(["'])/gm,
        (whole, pre, spec, post) => {
            if (/\.[a-zA-Z0-9]+$/.test(spec)) return whole; // already has an extension
            // `.client.js` first: a PRE-COMPILED "use client" island (Turbopack's
            // client-reference proxy can't handle custom-extension module ids, so
            // islands are precompiled to real JS under a name the loader's own
            // transient `X.js` output can never collide with). Then the live
            // loader-compiled PythScribe sources, compressed first.
            for (const ext of [".client.js", ".psc", ".ps"]) {
                if (existsSync(resolvePath(dir, spec + ext))) {
                    return pre + spec + ext + post;
                }
            }
            return whole;
        },
    );
}

/**
 * Webpack loader for PythScribe .ps and .psc files.
 * Compiles source to JavaScript using the pyths CLI.
 * `.psc` (compressed) sources are expanded by the CLI before compilation.
 * Returns source maps when available for browser DevTools debugging.
 *
 * In dev mode, passes `--react-refresh` to the CLI so `@component`
 * functions are emitted with `$RefreshSig$` / `$RefreshReg$` calls, then
 * wraps the compiled module with a small prelude/postlude that installs
 * Next.js's `react-refresh/runtime` globals and registers the module
 * with webpack HMR. Result: state-preserving Fast Refresh on `.ps`/`.psc`
 * edits, matching the experience for `.tsx`.
 */
export default function pythsLoader(source) {
    const options = this.getOptions();
    const bin = options.pythsBin || "pyths";
    const refreshOpt = options.reactRefresh ?? "auto";
    const filePath = this.resourcePath;
    const jsPath = filePath.replace(/\.psc?$/, ".js");
    const mapPath = jsPath + ".map";
    // Emit a TS declaration sibling (default on) so .ts consumers of
    // `import './Foo.ps'` get precise types instead of `any`. Opt out with
    // `emitDts: false` in the plugin options.
    const emitDts = options.emitDts ?? true;
    const dtsTmpPath = filePath.replace(/\.psc?$/, ".d.ts");
    const declPath = filePath.endsWith(".psc")
        ? filePath.replace(/\.psc$/, ".d.psc.ts")
        : filePath.replace(/\.ps$/, ".d.ps.ts");

    // `this.hot` is webpack's HMR signal; `this.mode === "development"`
    // is the more direct switch. Either resolves to "this is a dev
    // build, emit Refresh-aware code".
    const isDev = this.mode === "development" || this.hot === true;
    const refreshEnabled = refreshOpt === true
        || (refreshOpt === "auto" && isDev);

    const compileArgs = ["compile", filePath, "--sourcemap"];
    const stdoutArgs = ["compile", "--stdout", filePath];
    if (refreshEnabled) {
        compileArgs.push("--react-refresh");
        stdoutArgs.push("--react-refresh");
    }
    if (emitDts) {
        compileArgs.push("--dts");
    }

    try {
        // Compile with source map
        execFileSync(bin, compileArgs, {
            encoding: "utf-8",
            timeout: 30000,
        });

        // Persist the declaration sibling (write-if-changed → no rebuild loop).
        if (emitDts && existsSync(dtsTmpPath)) {
            const dts = readFileSync(dtsTmpPath, "utf-8");
            const prev = existsSync(declPath) ? readFileSync(declPath, "utf-8") : null;
            if (prev !== dts) writeFileSync(declPath, dts);
        }

        const rawCode = rewritePsImports(readFileSync(jsPath, "utf-8"), filePath);
        const jsCode = refreshEnabled
            ? wrapWithRefreshShim(rawCode, filePath)
            : rawCode;
        let sourceMap = null;
        if (existsSync(mapPath)) {
            sourceMap = JSON.parse(readFileSync(mapPath, "utf-8"));
        }

        // Return code + source map via callback
        this.callback(null, jsCode, sourceMap);
    } catch (err) {
        // Fall back: --stdout without source map
        try {
            const rawCode = rewritePsImports(execFileSync(bin, stdoutArgs, {
                encoding: "utf-8",
                timeout: 30000,
            }), filePath);
            return refreshEnabled
                ? wrapWithRefreshShim(rawCode, filePath)
                : rawCode;
        } catch {
            this.emitError(
                new Error(`PythScribe compilation failed for ${filePath}:\n${err.stderr || err.message}`)
            );
            return "";
        }
    } finally {
        // Clean up transient generated files; keep the `.d.ps.ts` sibling.
        try { if (existsSync(jsPath)) unlinkSync(jsPath); } catch {}
        try { if (existsSync(mapPath)) unlinkSync(mapPath); } catch {}
        try { if (existsSync(dtsTmpPath)) unlinkSync(dtsTmpPath); } catch {}
    }
}

/**
 * Wrap the compiled module with Next.js's react-refresh handshake.
 *
 * The PythScribe codegen emitted `const _s_X = $RefreshSig$()` and
 * `$RefreshReg$(X, "X")` calls — they look up the globals installed
 * here. The prelude swaps in runtime-backed implementations; the
 * postlude restores the previous values (so concurrent modules don't
 * collide) and accepts the HMR update.
 *
 * Idempotent — if `code` already starts with the marker comment, this
 * function returns it unchanged. Webpack sometimes invokes loaders
 * twice on the same source in eager-recompile cycles.
 */
function wrapWithRefreshShim(code, id) {
    if (code.includes("/*pyths-refresh-prelude*/")) {
        return code;
    }
    // App Router evaluates modules in three contexts: the RSC (server)
    // layer, the SSR pass of client components (also Node), and the
    // browser. `self` does not exist in Node, and Fast Refresh is a
    // browser-only mechanism — so the shim must (a) never touch `self`
    // unguarded, (b) install NO-OP $RefreshReg$/$RefreshSig$ on the
    // server (the compiled code calls them unconditionally when built
    // with --react-refresh), and (c) only register with the refresh
    // runtime + accept HMR in the browser. `import.meta.webpackHot` is
    // the ESM-safe HMR handle (`module` is not defined in ESM output).
    const prelude = `/*pyths-refresh-prelude*/
import RefreshRuntime from "next/dist/compiled/react-refresh/runtime";
const __pyths_g = typeof self !== "undefined" ? self : globalThis;
const __pyths_isBrowser = typeof window !== "undefined";
const __pyths_prev_RefreshReg = __pyths_g.$RefreshReg$;
const __pyths_prev_RefreshSig = __pyths_g.$RefreshSig$;
__pyths_g.$RefreshReg$ = __pyths_isBrowser
  ? (type, id) => { RefreshRuntime.register(type, ${JSON.stringify(id)} + " " + id); }
  : () => {};
__pyths_g.$RefreshSig$ = __pyths_isBrowser
  ? RefreshRuntime.createSignatureFunctionForTransform
  : () => (type) => type;
`;
    const postlude = `
__pyths_g.$RefreshReg$ = __pyths_prev_RefreshReg;
__pyths_g.$RefreshSig$ = __pyths_prev_RefreshSig;
const __pyths_hot = typeof import.meta.webpackHot !== "undefined"
  ? import.meta.webpackHot
  : (typeof module !== "undefined" && module.hot ? module.hot : undefined);
if (__pyths_isBrowser && __pyths_hot) {
  __pyths_hot.accept();
  RefreshRuntime.performReactRefresh();
}
`;
    return prelude + code + postlude;
}
