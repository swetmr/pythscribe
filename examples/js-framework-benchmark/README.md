# js-framework-benchmark — PythScribe `.ps`

The [official js-framework-benchmark](https://github.com/krausest/js-framework-benchmark)
row-table workload (**keyed** variant), written in PythScribe `.ps` and compiled to
React 19 — dual-tracked against a hand-written React reference.

It exercises the operations the benchmark's Chrome driver measures:

| Control (`id`) | Operation |
|---|---|
| `run` | create 1,000 rows |
| `runlots` | create 10,000 rows |
| `add` | append 1,000 rows |
| `update` | update every 10th row's label |
| `clear` | remove all rows |
| `swaprows` | swap rows 1 and 998 |
| (row `<a>`) | select a row (adds `danger`) / remove a row |

The DOM (`table.test-data`, keyed `<tr>` with four `<td>`, the select/remove `<a>`s)
matches the benchmark's `keyed` contract, so this drops into the official runner
unchanged.

## Two tracks

| File | Track |
|---|---|
| [`src/Main.ps`](src/Main.ps) | **PythScribe** — the system under test |
| [`src/react-reference/Main.tsx`](src/react-reference/Main.tsx) | **React** — the oracle |

Both render byte-identical DOM. Any divergence is a PythScribe bug, not a
benchmark artifact — the same dual-track methodology the project uses elsewhere.

## Run locally

```bash
# from this directory
npm install
export PYTHS_BIN=../../target/release/pyths        # or pyths on PATH
npm run dev            # PythScribe .ps track   → http://localhost:5173/
npm run dev:react      # React reference track  → /index-react.html
```

`cargo build --release -p pyths_cli` first if `PYTHS_BIN` doesn't exist.

## Register with the official harness (for real numbers)

The published perf chart comes from krausest's driver, not this repo:

1. Clone `krausest/js-framework-benchmark`.
2. Copy this app into `frameworks/keyed/pythscribe/` and add a
   `package.json` `js-framework-benchmark` block (name `pythscribe-vX`,
   `frameworkVersionFromPackage`, `issues`/`keyed: true`), following an existing
   `frameworks/keyed/react-hooks/` entry — the build output is a standard Vite
   `dist/` with the same `index.html` shape.
3. From the benchmark root: `npm ci`, `npm start` (results server), then
   `npm run bench keyed/pythscribe` and `npm run results`.

The React reference here corresponds to the harness's `react-hooks` entry, so a
side-by-side run is the honest comparison.

## Note — a codegen bug this example surfaced

Scaffolding this example surfaced a real compiler bug: the natural id counter
(`global next_id; next_id = next_id + 1`) mis-lowered — `global` emitted a
*shadowing local*, so the module variable was never mutated (ids came out
`NaN`). Tracked and fixed as **[#199](https://github.com/swetmr/pythscribe/issues/199)**
(PR #205): names declared `global`/`nonlocal` are now excluded from the
function's local-declaration set, so assignments rebind the outer binding.
`Main.ps` now uses the natural `global next_id` counter directly.
