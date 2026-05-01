# Sizzle — Project Memory

## What it is
Tauri v2 desktop app for "vibe coding" — manages tens to hundreds of local projects, browses them, reads READMEs, and launches AI coding agents (Claude Code, Codex) in project-rooted split terminals. Rust backend + OS-native WebView frontend.

## Key paths
- Rust entry: `src-tauri/src/main.rs` (calls `sizzle_lib::run()`)
- App init / commands: `src-tauri/src/lib.rs`
- Tauri commands: `src-tauri/src/commands/` (scanner, metadata, files, git, pty, etc.)
- Frontend entry: `src/renderer/main.tsx`
- Bridge layer (all invoke/listen calls): `src/renderer/api.ts`
- Zustand store: `src/renderer/store/appStore.ts`
- Frontend components: `src/renderer/components/` (LeftPane, MainPane, GitStatusPane)
- Metadata store: `~/.config/sizzle/db.json` by default, or `--sizzle-config-dir=/path/to/config`
- Tauri config: `src-tauri/tauri.conf.json`
- Capabilities/permissions: `src-tauri/capabilities/default.json`
- Build script (auto-generates API manifest): `src-tauri/build.rs`

## Stack
- **Backend**: Rust + Tauri 2, portable-pty 0.8, serde/serde_json, tokio (minimal), chrono
- **Frontend**: Vite 6 + React 19 + Zustand 5 + TypeScript 5
- **Terminal**: @xterm/xterm 5.5 + addon-canvas + addon-fit
- **Markdown**: react-markdown 10 + rehype-highlight + remark-gfm

## Architecture notes
- **Single binary**: No separate pty-host process (unlike Electron). PTY reader runs in a `std::thread::spawn`, emits `pty:data` Tauri events every ~16ms.
- **IPC**: All frontend-backend communication via `@tauri-apps/api` `invoke()` (commands) and `listen()` (events). No Electron-style preload/contextBridge.
- **API sync**: `build.rs` parses `lib.rs` and command files to auto-generate a JSON manifest. Frontend compares its manifest against this on startup to detect version mismatches.
- **State**: `MetadataStore` (JSON file backed, Rust managed state) + `Mutex<PtyRegistry>` (thread-safe PTY registry) on backend; Zustand store on frontend.

## Tags
Heuristics: `.git` directory, README, manifests (`Cargo.toml`, `package.json`, `go.mod`), source file counts. Tag detection scores languages by extension and frameworks by file patterns (React, Next.js, Vue, Angular, Svelte, Electron, Django, Flask, Rails, Godot, etc.).

## Dev command
```
npm run dev
```
(Runs `tauri dev` — requires DISPLAY for the WebView window)

## Build for production
```
npm run build
```
(Runs `tauri build` — bundles into a single platform-native installer)

## Critical notes
- After `npm install`, if the Tauri CLI is missing, run `npx @tauri-apps/cli` once or install system-wide.
- No Electron dependencies remain. Do not reference `electron-vite`, `node-pty`, preload scripts, or the old pty-host helper.
- Rust log level controlled via `RUST_LOG` env var (default "info" in env_logger setup).
