import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import withPyths from "next-plugin-pyths";

const here = dirname(fileURLToPath(import.meta.url));
// next/ -> ../.. = repo root (examples/clones/next -> pythscribe).
// Turbopack scopes module resolution to `root`; without this it can't
// follow the file: symlinks into runtime / packages, nor resolve relative
// imports that reach up into ../shared/<clone>/.
const repoRoot = resolve(here, "../../..");

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
