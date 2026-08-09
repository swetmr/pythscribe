import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

/**
 * Next.js plugin for PythScribe (.ps and .psc files).
 *
 * Registers a `.ps`/`.psc` loader on BOTH bundlers: Turbopack (Next.js 16's
 * default, via `turbopack.rules`) and webpack (the `--webpack` opt-out and
 * Next < 16, via `config.module.rules`). The loader compiles sources to
 * JavaScript using the `pyths` CLI before Next.js processes them; `.psc`
 * (compressed) files are expanded by the CLI first.
 *
 * Usage in next.config.mjs:
 *   import withPyths from "next-plugin-pyths";
 *   export default withPyths({
 *     // your Next.js config
 *   });
 *
 * Options:
 *   - pythsBin: Path to the pyths binary (default: auto-detect)
 *   - reactRefresh: React Refresh / Fast Refresh policy.
 *       `"auto"` (default): on in `next dev`, off in `next build`.
 *       `true`: always on. `false`: always off (HMR falls back to
 *       webpack's default module-reload).
 *
 * @param {object} nextConfig
 * @param {object} pluginOptions
 * @returns {object} Modified Next.js config
 */
export default function withPyths(nextConfig = {}, pluginOptions = {}) {
    const pythsBin = pluginOptions.pythsBin || findPythsBinary();
    const reactRefresh = pluginOptions.reactRefresh ?? "auto";
    const loaderPath = resolve(import.meta.dirname || process.cwd(), "loader.js");
    const loaderOptions = { pythsBin, reactRefresh };

    return {
        ...nextConfig,

        // Turbopack path — Next.js 16's default bundler. Turbopack ignores
        // the `webpack()` hook below, so `.ps`/`.psc` must be registered as
        // Turbopack loader rules here. `loader.js` is webpack-loader-compatible
        // (resourcePath / getOptions / callback), which Turbopack supports;
        // `as: "*.js"` tells Turbopack the loader output is JavaScript.
        // (Under Turbopack the react-refresh shim stays off — Turbopack
        // instruments React Fast Refresh on the compiled output natively.)
        turbopack: {
            ...(nextConfig.turbopack || {}),
            // Let Turbopack resolve extensionless relative imports to
            // `.ps`/`.psc` siblings (the webpack path does this via
            // `config.resolve.extensions`). Without it, a server `.ps` that
            // imports `./Counter` can't find `Counter.ps`/`.psc`. Preserve
            // the standard set so normal .ts(x)/.js(x)/.json still resolve.
            resolveExtensions: [
                ...(nextConfig.turbopack && nextConfig.turbopack.resolveExtensions
                    ? nextConfig.turbopack.resolveExtensions
                    : [".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".json"]),
                ".psc",  // prefer compressed over canonical when both exist
                ".ps",
            ],
            rules: {
                ...((nextConfig.turbopack && nextConfig.turbopack.rules) || {}),
                "*.ps": {
                    loaders: [{ loader: loaderPath, options: loaderOptions }],
                    as: "*.js",
                },
                "*.psc": {
                    loaders: [{ loader: loaderPath, options: loaderOptions }],
                    as: "*.js",
                },
            },
        },

        // Webpack path — retained for `next dev/build --webpack` (the
        // opt-out bundler) and Next < 16.
        webpack(config, options) {
            // Add .psc before .ps so compressed variant is preferred when both exist
            config.resolve.extensions.push(".psc", ".ps");

            // Add webpack loader for .ps and .psc files
            config.module.rules.push({
                test: /\.psc?$/,
                use: [
                    {
                        loader: loaderPath,
                        options: loaderOptions,
                    },
                ],
            });

            // Chain with existing webpack config if provided
            if (typeof nextConfig.webpack === "function") {
                return nextConfig.webpack(config, options);
            }
            return config;
        },

        // Add .psc before .ps to pageExtensions so page.psc is preferred over page.ps
        pageExtensions: [
            ...(nextConfig.pageExtensions || ["tsx", "ts", "jsx", "js"]),
            "psc",  // prefer compressed over canonical when both exist
            "ps",
        ],
    };
}

/**
 * Find the pyths binary.
 */
function findPythsBinary() {
    // SECURITY (CWE-426 untrusted search path): do NOT probe cwd-relative `target/`
    // paths by default, and NEVER execute a candidate just to test it — a hostile
    // project could ship `./target/release/pyths` and gain code execution the moment
    // a developer runs the build. Resolution order:
    //   1. explicit PYTHS_BIN env (trusted, user-set)
    //   2. opt-in local dev build via PYTHS_DEV_BIN (existence check only, no exec)
    //   3. the `pyths` command on PATH / node_modules/.bin (npm puts .bin on PATH)
    if (process.env.PYTHS_BIN) return process.env.PYTHS_BIN;
    if (process.env.PYTHS_DEV_BIN) {
        for (const rel of [
            "target/release/pyths", "target/release/pyths.exe",
            "target/debug/pyths", "target/debug/pyths.exe",
        ]) {
            const p = resolve(process.cwd(), rel);
            if (existsSync(p)) return p;
        }
    }
    return "pyths";
}
