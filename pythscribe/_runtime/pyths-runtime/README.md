# pyths-runtime

The runtime library for [PythScribe](https://github.com/swetmr/pythscribe) — small JavaScript shims that back the Python builtins, standard library, and web APIs that PythScribe-compiled code calls.

Required when your `.ps` or `.psc` source uses:

- **Builtins** that lower to runtime helpers (`len`, `range`, `enumerate`, `zip`, `sum`, `all`, `any`, etc.)
- **Standard-library modules** (`math`, `json`, `itertools`, `functools`, `collections`, `random`, `datetime`, `re`)
- **Web modules** (`pyths.dom`, `pyths.web.fetch`, `pyths.web.storage`, `pyths.web.router`)
- **React helpers** (`pyths.react.createElement`, hook re-exports)

## Install

```bash
npm install pyths-runtime
```

## Bundle size

~1 KB gzipped for typical apps after tree-shaking. The runtime is structured as one module per Python module (`pyths-runtime/stdlib/math` vs `/stdlib/json` etc.) so unused stdlib doesn't ship.

## How PythScribe references it

The compiler emits `import { ... } from "pyths-runtime/stdlib/<module>"` for stdlib uses and `import { ... } from "pyths-runtime/<web>/<api>"` for web-platform wrappers. As long as `pyths-runtime` is installed in your project, the imports resolve through the bundler the same way any npm dep would.

See [the main PythScribe repo](https://github.com/swetmr/pythscribe) for the compiler, plugins, and getting-started guides.

## License

MIT.
