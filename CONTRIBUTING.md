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

- The **Rust toolchain** must be installed (via [rustup](https://rustup.rs/)) to compile the Tauri backend.
- Platform Tauri prerequisites are required (system libs on Linux, Xcode on macOS, WebView2 on Windows). See the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).

## Project notes

- Backend entry: `src-tauri/src/lib.rs`
- Renderer entry: `src/renderer/main.tsx`
- Front-end API bridge: `src/renderer/api.ts`
- Tauri commands live under: `src-tauri/src/commands/`
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
