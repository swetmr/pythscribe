# tests/libinterop — third-party React library behavioral parity

Track-B sweep. Dual-track methodology at library scale: for each library a
**TSX reference component** (`references/*.tsx`, the oracle) and a **`.ps`
twin** (`components/*.ps`, the system under test) are driven by **one vitest
spec** (`specs/*.behavior.test.ts`) through identical behavioral assertions
(mount → interact via `@testing-library/user-event` → assert). Any divergence
between the tracks is a compiler finding.

## Run

```bash
cargo build --release -p pyths_cli   # or set PYTHS_BIN
node tests/libinterop/run.mjs        # npm-installs on first use, then vitest
```

## Coverage (10 packages / 8 spec pairs)

| Spec | Package(s) | Load-bearing patterns probed |
|---|---|---|
| `dialog` | `@radix-ui/react-dialog` | module-alias import (`import at_radix_ui.react_dialog as Dialog`), portal render, `on_open_change`/`as_child` snake props, `ref=` on an asChild Trigger, `**props` spread into Content |
| `dropdown` | `@radix-ui/react-dropdown-menu` | menu opens, `on_select` fires per item, event object consumed Pythonically (`e.preventDefault()`) |
| `checkbox` | `@radix-ui/react-checkbox` | controlled `checked` + `on_checked_change` boolean flow, Indicator child |
| `button` | `class-variance-authority` + `clsx` + `tailwind-merge` | shadcn Button: cva config dicts as plain objects, `cn = twMerge(clsx(...))`, `**rest` passthrough on a user @component, exact final `className` parity |
| `icons` | `lucide-react` | icon renders as `<svg>`, `size`/`stroke_width` props, dynamic icon selection from a dict of component factories |
| `form` | `react-hook-form` | `use_form` + `**register(...)` spread, `handleSubmit` validation flow, error message render, `Controller` render-prop (children-as-function) |
| `query` | `@tanstack/react-query` | `QueryClientProvider` + `use_query` with async fn, loading → data state transition |
| `motion` | `framer-motion` | `motion.div` member components, `animate`/`initial` dict props, `AnimatePresence` mount/unmount |

Package versions are pinned exactly in `package.json`.

## Conventions pinned here

- Library-component props are written in **snake_case** (`on_open_change=`,
  `as_child=True`) — the compiler converts them like HTML props. Verbatim
  camelCase also works.
- **Dynamically-selected** components (`Icon = ICONS[name]`) get NO prop
  conversion — write final JS names (`strokeWidth=`, `**{"data-testid": ...}`).
- `**spread` dict keys pass through verbatim — use final JS names in spreads.
- Library **config dicts** (cva variants, useQuery options) are library API —
  write their keys verbatim (`queryKey`, `defaultVariants`).
- Member calls into library objects are verbatim: `rhf.handleSubmit`,
  `result.isLoading`, `form.register`.
