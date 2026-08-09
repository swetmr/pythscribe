import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import withPyths from "next-plugin-pyths";

const here = dirname(fileURLToPath(import.meta.url));
// Repo root (examples/next-app -> ../..). Turbopack scopes module
// resolution to `root`; the pyths-runtime / plugin packages are linked
// from there, so without this Turbopack can't follow the symlinks.
const repoRoot = resolve(here, "../..");

const pythsBin =
    process.env.PYTHS_BIN ??
    "pyths";

export default withPyths(
    {
        reactStrictMode: true,
        turbopack: { root: repoRoot },
    },
    { pythsBin, reactRefresh: "auto", emitDts: false },
);
