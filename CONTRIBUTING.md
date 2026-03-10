# Contributing to Sizzle

Thanks for contributing.

## Before you start

- Open an issue first for large changes, behavior changes, or significant UI changes.
- Keep pull requests focused. Small, reviewable changes move faster.

## Development setup

```bash
git clone https://github.com/NebuPookins/sizzle.git
cd sizzle
npm install
npm run dev
```

Notes:

- `node-pty` is a native dependency and is rebuilt during `npm install`.
- If Electron is missing after install, run `node node_modules/electron/install.js`.
- On Linux, `npm run dev` requires `DISPLAY` to be set.

## Project notes

- Main process entry: `src/main/index.ts`
- Preload entry: `src/preload/index.ts`
- Renderer entry: `src/renderer/main.tsx`
- Local metadata is stored under `~/.config/sizzle` by default.

This repo also includes agent-assistance files such as `AGENTS.md` and `CLAUDE.md`. They are repository guidance files, not a requirement for contributing.

## Pull request checklist

- The change builds cleanly with `npm run build`.
- New behavior is documented where needed.
- No secrets, local paths, or personal config were added.
- The diff does not include unrelated cleanup.

## Reporting bugs

Use the GitHub issue tracker:

<https://github.com/NebuPookins/sizzle/issues>
