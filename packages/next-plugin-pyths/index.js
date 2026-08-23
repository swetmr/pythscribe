import { resolve } from "node:path";
import { resolvePythsCommand } from "./pyths-safe.js";

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
    // SECURITY (#1, CWE-426): resolve the compiler to an ABSOLUTE command here,
    // once, and pass it down to the loader. A bare name like "pyths" is resolved
    // by the platform at spawn time and that search can include the CURRENT
    // DIRECTORY, so a hostile repo's `./pyths.exe` would be selected. Loader
    // options must stay JSON-serializable for Turbopack, so the command is
    // carried as a string plus a string[] of leading args.
    const { command, prefixArgs } = resolvePythsCommand({
        pythsBin: pluginOptions.pythsBin,
    });
    const reactRefresh = pluginOptions.reactRefresh ?? "auto";
    const loaderPath = resolve(import.meta.dirname || process.cwd(), "loader.js");
    // P7: propagate emitDts so `withPyths({}, { emitDts: false })` actually
    // suppresses the `.d.ps.ts` write (the loader defaults it to true otherwise).
    const loaderOptions = {
        pythsBin: command,
        pythsPrefixArgs: prefixArgs,
        reactRefresh,
        emitDts: pluginOptions.emitDts,
    };

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

// Compiler resolution now lives in `pyths-safe.js::resolvePythsCommand`, which
// always yields an ABSOLUTE command (#1, CWE-426) and is shared byte-for-byte
// with vite-plugin-pyths.
