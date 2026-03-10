# Sizzle — Project Memory

## What it is
Steam-like Electron + React/TypeScript desktop app. Prompts for a projects root on first launch, then scans configurable directories, shows README, and launches Claude Code + shell split terminal.

## Key paths
- Main process: `src/main/index.ts`
- Preload: `src/preload/index.ts`
- Renderer entry: `src/renderer/main.tsx`
- Metadata store: `~/.config/sizzle/db.json` by default, or `--sizzle-config-dir=/path/to/config`

## Stack
- electron-vite 5.0.0 (supports Vite 6), Electron 33, React 19, Zustand 5
- node-pty (native, needs rebuild), @xterm/xterm 5.5

## Critical notes
- `electron-rebuild` CLI fails on Node 25 (ESM/CJS yargs conflict). Use JS API via `scripts/rebuild-pty.cjs` instead.
- `postinstall` calls `node scripts/rebuild-pty.cjs` (not electron-rebuild CLI)
- Electron binary: run `node node_modules/electron/install.js` if missing (path.txt absent)
- electron-vite 2.x requires vite ^4||^5 — must use electron-vite 5.x for Vite 6 support

## Dev command
```
npm run dev
```
(Requires DISPLAY to be set for the Electron window)
