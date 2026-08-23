import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import {
    makePrivateTempDir,
    removePrivateTempDir,
    resolvePythsCommand,
    runPyths,
    writeGeneratedSibling,
} from "./pyths-safe.js";

/**
 * Vite plugin for PythScribe (.ps and .psc files).
 *
 * Compiles .ps and .psc files to JavaScript using the `pyths` CLI.
 * `.psc` (compressed) files are expanded by the CLI before compilation;
 * `.ps` files pass through unchanged. The compiled output is then
 * processed by Vite as normal JS.
 *
 * Usage in vite.config.js:
 *   import pyths from "vite-plugin-pyths";
 *   export default { plugins: [pyths()] };
 *
 * Options:
 *   - pythsBin: Path to the pyths binary (default: auto-detect)
 *   - react: Enable React mode transforms (default: auto-detect from .ps content)
 *   - reactRefresh: Enable React Refresh (HMR with state preservation).
 *       `"auto"` (default): enabled in dev mode, disabled in build mode.
 *       `true`: always enabled. `false`: always disabled (falls back to
 *       full-reload HMR).
 *   - emitDts: Write a TypeScript declaration sibling for each compiled
 *       module so `.ts`/`.tsx` consumers get precise types instead of
 *       `any`. `Foo.ps` → `Foo.d.ps.ts`, `Foo.psc` → `Foo.d.psc.ts` (the
 *       TS 5.0 arbitrary-extension form; requires `allowArbitraryExtensions`
 *       + `bundler`/`node16`+ moduleResolution in the consumer's tsconfig).
 *       Default `true`. Files are (re)written only when content changes.
 *
 * @param {object} options
 * @returns {import("vite").Plugin}
 */
export default function pyths(options = {}) {
    let pythsCmd = null;
    const reactRefreshOpt = options.reactRefresh ?? "auto";
    const emitDts = options.emitDts ?? true;
    let isDev = false;

    return {
        name: "vite-plugin-pyths",
        enforce: "pre",

        configResolved(config) {
            // SECURITY (#1, CWE-426): resolve the compiler to an ABSOLUTE command
            // here. Never hand a bare name to execFileSync — the platform search
            // can include the current directory, letting a hostile repo's
            // `./pyths.exe` be selected. See pyths-safe.js.
            if (!pythsCmd) {
                pythsCmd = resolvePythsCommand({ pythsBin: options.pythsBin });
            }
            isDev = config.command === "serve";
        },

        /**
         * Importer-aware resolution for dual-track projects.
         *
         * When a `.ps`/`.psc` module imports a RELATIVE module that has a
         * `.ps`/`.psc` sibling on disk, resolve to that sibling — even when a
         * same-named `.tsx`/`.ts` also exists. Without this, Vite's global
         * `resolve.extensions` order (`.tsx` before `.ps`) makes a PythScribe
         * module silently import its React-reference sibling, mixing the two
         * tracks and crashing on prop-name mismatches (snake_case vs camelCase).
         *
         * Only relative specifiers from a `.ps`/`.psc` importer are touched;
         * everything else (npm packages, `.tsx` importers, specifiers with no
         * `.ps` sibling such as shared `.ts` data modules) defers to Vite.
         */
        resolveId(source, importer) {
            if (!importer) return null;
            const fromPs = importer.endsWith(".ps") || importer.endsWith(".psc");
            if (!fromPs) return null;
            if (!source.startsWith(".")) return null;

            const baseDir = dirname(importer.split("?")[0]);
            const target = resolve(baseDir, source.split("?")[0]);
            for (const ext of [".ps", ".psc"]) {
                if (existsSync(target + ext)) return target + ext;
            }
            for (const ext of ["index.ps", "index.psc"]) {
                const idx = resolve(target, ext);
                if (existsSync(idx)) return idx;
            }
            return null;
        },

        /**
         * Transform .ps files to JavaScript.
         * Vite calls this for every module that matches our filter.
         */
        transform(code, id) {
            if (!id.endsWith(".ps") && !id.endsWith(".psc")) return null;

            const refreshEnabled = reactRefreshOpt === true
                || (reactRefreshOpt === "auto" && isDev);

            try {
                const { code: jsCode, map } = compilePsFile(
                    pythsCmd || (pythsCmd = resolvePythsCommand({ pythsBin: options.pythsBin })),
                    id,
                    { reactRefresh: refreshEnabled, emitDts },
                );
                const finalCode = refreshEnabled
                    ? wrapWithRefreshShim(jsCode, id)
                    : jsCode;
                return { code: finalCode, map };
            } catch (err) {
                this.error(
                    `PythScribe compilation failed for ${id}:\n${err.message}`
                );
            }
        },

        /**
         * Handle HMR for .ps and .psc files. When React Refresh is on,
         * Vite's built-in @vitejs/plugin-react sees the compiled module
         * as plain JS with $RefreshReg$ calls and performs Fast Refresh.
         * When Refresh is off, fall back to full-reload.
         */
        handleHotUpdate({ file, server }) {
            if (file.endsWith(".ps") || file.endsWith(".psc")) {
                const refreshActive = reactRefreshOpt === true
                    || (reactRefreshOpt === "auto" && isDev);
                if (!refreshActive) {
                    server.ws.send({ type: "full-reload" });
                    return [];
                }
                // With Refresh: let the default Vite/React HMR handle it.
            }
        },
    };
}

/**
 * Prepend the React Refresh runtime shim and append the HMR-accept
 * handshake. Together these match the prelude/postlude that
 * @vitejs/plugin-react emits for .jsx/.tsx files — the PythScribe-emitted
 * `$RefreshSig$()` / `$RefreshReg$(...)` calls in `code` then bind to
 * the runtime hooks installed by this wrapper.
 *
 * This follows the exact @vitejs/plugin-react recipe (`addRefreshWrapper`
 * in @vitejs/plugin-react/dist/index.js). The `/@react-refresh` virtual
 * module exported by @vitejs/plugin-react exposes:
 *   getRefreshReg(id)                          — module-scoped reg factory
 *   createSignatureFunctionForTransform        — hook-sig factory
 *   registerExportsForReactRefresh(id, exports) — register all exports
 *   validateRefreshBoundaryAndEnqueueUpdate(id, prev, next) — boundary check
 *   __hmr_import(url)                          — async self-import helper
 *
 * Idempotent: re-entrant transforms (Vite sometimes calls `transform`
 * multiple times on the same module) won't double-wrap because the
 * wrapper's marker comment is detected.
 */
function wrapWithRefreshShim(code, id) {
    // Idempotency guard
    if (code.includes("/* [pyths-refresh-shim] */")) return code;

    const refreshContentRE = /\$RefreshReg\$\(/;
    const hasRefresh = refreshContentRE.test(code);

    // Shared preamble: import RefreshRuntime + web-worker guard
    let newCode = `import * as RefreshRuntime from "/@react-refresh";
const inWebWorker = typeof WorkerGlobalScope !== 'undefined' && self instanceof WorkerGlobalScope;
/* [pyths-refresh-shim] */
`;

    if (hasRefresh) {
        // Per-module header: save current window reg/sig, install module-scoped ones
        newCode += `let prevRefreshReg;
let prevRefreshSig;

if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "vite-plugin-pyths can't detect preamble. Something is wrong. " +
      "See https://github.com/vitejs/vite-plugin-react/pull/11#discussion_r430879201"
    );
  }

  prevRefreshReg = window.$RefreshReg$;
  prevRefreshSig = window.$RefreshSig$;
  window.$RefreshReg$ = RefreshRuntime.getRefreshReg(${JSON.stringify(id)});
  window.$RefreshSig$ = RefreshRuntime.createSignatureFunctionForTransform;
}

`;
    }

    // Body: compiled PythScribe output (contains $RefreshReg$ / $RefreshSig$ calls)
    newCode += code;

    if (hasRefresh) {
        // Restore window reg/sig after the module body
        newCode += `

if (import.meta.hot && !inWebWorker) {
  window.$RefreshReg$ = prevRefreshReg;
  window.$RefreshSig$ = prevRefreshSig;
}
`;
    }

    // HMR boundary footer: register exports + hot-accept with boundary validation
    newCode += `

if (import.meta.hot && !inWebWorker) {
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh(${JSON.stringify(id)}, currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate(${JSON.stringify(id)}, currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
`;

    return newCode;
}

/**
 * Compile a .ps file by invoking the pyths CLI.
 * Compiles with --sourcemap to generate both JS and a source map,
 * reads both files, cleans up, and returns { code, map }.
 * Falls back to --stdout (no source map) if --sourcemap fails.
 *
 * When `emitDts` is set, also passes --dts and persists the emitted
 * declaration as the TS arbitrary-extension sibling (`Foo.d.ps.ts`),
 * so `.ts` consumers of `import './Foo.ps'` get precise types.
 *
 * SECURITY (#6, CWE-73): all transient output (`<stem>.js`, `.js.map`,
 * `<stem>.d.ts`) is directed into a PRIVATE per-invocation directory via
 * `-o`, and that whole directory is removed afterwards. Previously the CLI
 * wrote those three files beside the SOURCE and a `finally` block unlinked
 * them "if they exist" — which deleted a developer's hand-written
 * `Counter.js` (a file the CLI itself refuses to overwrite) on every build.
 * Nothing is created or deleted next to the user's source any more.
 *
 * SECURITY (#2, CWE-59): the one file that must land beside the source — the
 * `.d.ps.ts` declaration sibling — goes through `writeGeneratedSibling`,
 * which refuses to follow a symlink and refuses to clobber a file that lacks
 * the generated marker.
 */
function compilePsFile(cmd, filePath, { reactRefresh = false, emitDts = false } = {}) {
    const workDir = makePrivateTempDir("vite");
    // The CLI derives `.js.map` and `.d.ts` from `-o`, so one `-o` inside the
    // private directory relocates every transient artifact at once.
    const jsPath = join(workDir, "mod.js");
    const mapPath = jsPath + ".map";
    const dtsTmpPath = join(workDir, "mod.d.ts");
    // We relocate the declaration to the arbitrary-extension form TS resolves
    // for `import './Foo.ps'` — `Foo.d.ps.ts` (or `.d.psc.ts`).
    const declPath = filePath.endsWith(".psc")
        ? filePath.replace(/\.psc$/, ".d.psc.ts")
        : filePath.replace(/\.ps$/, ".d.ps.ts");
    // Pin `--target js`: the compiler's no-flag default is auto-routing
    // (js+wasm) as of 0.2.2, which emits .wasm/.glue.js sidecars for
    // numeric-kernel modules. The bundler plugin does not yet manage those
    // sidecars, so it stays explicitly JS-only. Teaching the plugin to carry
    // the WASM sidecars (keep auto-routing live in the browser build) is the
    // 0.2.3 follow-up (spec 17-07-26 §4.7.5).
    const compileArgs = ["compile", filePath, "-o", jsPath, "--sourcemap", "--target", "js"];
    const stdoutArgs = ["compile", "--stdout", filePath];
    if (reactRefresh) {
        compileArgs.push("--react-refresh");
        stdoutArgs.push("--react-refresh");
    }
    // `.d.ts` is default-ON in the compiler as of 0.2.2, so honoring
    // `emitDts: false` now requires an explicit `--no-dts` (a bare omission
    // would still emit the declaration). `--dts` when true is redundant but
    // kept for clarity / older compiler pins.
    if (emitDts) {
        compileArgs.push("--dts");
    } else {
        compileArgs.push("--no-dts");
    }

    let compiled;
    try {
        // Compile into the private dir with a source map
        runPyths(cmd, compileArgs);

        const code = readFileSync(jsPath, "utf-8");
        let map = null;
        if (existsSync(mapPath)) {
            map = JSON.parse(readFileSync(mapPath, "utf-8"));
        }
        compiled = { code, map, dts: null };
        if (emitDts && existsSync(dtsTmpPath)) {
            compiled.dts = readFileSync(dtsTmpPath, "utf-8");
        }
    } catch {
        // Fall back: --stdout without source map (no dts on this path).
        try {
            compiled = { code: runPyths(cmd, stdoutArgs), map: null, dts: null };
        } finally {
            removePrivateTempDir(workDir);
        }
        return { code: compiled.code, map: null };
    }
    // Only this invocation's private directory is removed — never a file
    // beside the user's source (#6).
    removePrivateTempDir(workDir);

    // Persist the declaration sibling through the no-follow, ownership-aware
    // writer (#2). A refusal must not fail the build: types are a nicety, and
    // the diagnostic tells the user which file is in the way.
    if (compiled.dts !== null) {
        try {
            writeGeneratedSibling(declPath, compiled.dts, { markerAware: true });
        } catch (err) {
            console.warn(`[vite-plugin-pyths] skipped .d.ts emission: ${err.message}`);
        }
    }

    return { code: compiled.code, map: compiled.map };
}

// Compiler resolution now lives in `pyths-safe.js::resolvePythsCommand`, which
// always yields an ABSOLUTE command (#1, CWE-426) and is shared byte-for-byte
// with next-plugin-pyths.
export { compilePsFile as __compilePsFileForTests };
