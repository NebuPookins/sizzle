# Sizzle — Project Memory

## What it is
GTK4 desktop app for "vibe coding" — manages tens to hundreds of local projects, browses them, reads READMEs, and launches AI coding agents (Claude Code, Codex) in project-rooted split terminals. Rust backend + GTK4 native UI.

## Key paths
- Rust entry: `crates/sizzle-gtk/src/main.rs`
- Core library: `crates/sizzle-core/` (scanner, metadata, files, git)
- Terminal emulation: `crates/sizzle-gtk/src/terminal.rs`
- Markdown rendering: `crates/sizzle-gtk/src/markdown.rs`
- Metadata store: `~/.config/sizzle/db.json` by default, or `--sizzle-config-dir=/path/to/config`
- Workspace manifest: `Cargo.toml` (members: `sizzle-core`, `sizzle-gtk`, `term-proto`)

## Stack
- **Backend**: Rust + GTK4 0.9, alacritty_terminal 0.26, pulldown-cmark 0.12, portable-pty 0.8, serde/serde_json, chrono
- **UI**: GTK4 native widgets (no WebView/HTML)
- **Terminal**: alacritty_terminal based terminal emulation
- **Markdown**: pulldown-cmark parser + PangoCairo rendering

## Architecture notes
- **Single binary**: `sizzle-gtk` binary built via Cargo workspace. No separate renderer process.
- **IPC**: All UI-backend communication is in-process via direct Rust function calls. No RPC/IPC layer.
- **State**: `MetadataStore` (JSON file backed) for project metadata; in-memory state for terminal sessions.
- **Terminal**: Uses alacritty_terminal for terminal emulation with PTY reader thread for I/O.
- **Markdown**: pulldown-cmark for parsing, custom PangoCairo rendering in the UI.

## Tags
Heuristics: `.git` directory, README, manifests (`Cargo.toml`, `package.json`, `go.mod`), source file counts. Tag detection scores languages by extension and frameworks by file patterns (React, Next.js, Vue, Angular, Svelte, Electron, Django, Flask, Rails, Godot, etc.).

## Dev command
```
cargo run -p sizzle-gtk
```
(Runs the GTK4 UI — requires DISPLAY on Linux)

## Build for production
```
cargo build --release -p sizzle-gtk
```
The binary will be at `target/release/sizzle-gtk`.

## Critical notes
- GTK4 development libraries are required on the system (libgtk-4-dev on Debian/Ubuntu, gtk4-devel on Fedora, gtk4 on Arch).
- Rust log level controlled via `RUST_LOG` env var (default "info" in env_logger setup).
