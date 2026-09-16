"""The compiler pin (pin, don't float).

The build step refuses to build against a `pyths` whose reported version differs from
COMPILER_VERSION, and every manifest records the pin it was built with. Bump these two
constants (one-line commit) when the compiler moves.
"""

COMPILER_VERSION = "0.2.5"
COMPILER_COMMIT = "7aac7cf16919e7f9c49e4971102e84b67ad84dd2"

# M2.1 (spec 13-09-26 §5.6, Layer 2): the range of compiler versions whose PREBUILT artifacts
# this runtime LOADS (`artifacts.resolve()` classifies an out-of-range `manifest.compiler.version`
# as unusable -> Python fallback + warning; explicit mode/artifact= raises). COMPILER_VERSION above
# stays the exact BUILD pin. The in-module `pyths.abi` section (Layer 1, `runtime/abi.py`) is the
# authority on layout compatibility within the range.
ACCEPTED_COMPILER_RANGE = ">=0.2.4,<0.3"

# Artifact manifest schema version. Bump on any layout change; older manifests are then
# reported as unusable (-> Python fallback), never half-read.
MANIFEST_SCHEMA = 1
