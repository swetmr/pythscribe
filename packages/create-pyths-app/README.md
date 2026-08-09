# create-pyths-app

Scaffold a new PythScribe project in seconds.

## Usage

```bash
npm create pyths-app@latest my-app
cd my-app
npm install
npm run dev
```

Or with `npx`:

```bash
npx create-pyths-app my-app
```

## What you get

- A Vite + React project pre-wired with `vite-plugin-pyths`.
- An example `.ps` component to copy from.
- `pyths.toml` with sensible defaults.
- Standard scripts: `dev`, `build`, `preview`, `lint`, `check`.

## Project structure

```
my-app/
├── app/                  # .ps source files
│   └── App.ps
├── components/           # shared @component / @psx helpers
├── public/               # static assets
├── pyths.toml
├── vite.config.js
├── package.json
└── index.html
```

## Next steps after scaffolding

- Read [getting-started-with-vite.md](https://github.com/your-org/pythscribe/blob/main/docs/getting-started-with-vite.md) to understand the plugin layer.
- Check `examples/` in the main repo for full app fixtures (dashboard, CRM).
- Run `pyths check **/*.ps` before committing — catches type errors at compile time.

## License

MIT.
