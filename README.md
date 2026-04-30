# Sizzle

Sizzle is an IDE-style desktop app built for vibe coding first.

The core idea is not just opening one repo and staying there. It is managing tens to hundreds of active local projects at the same time, moving quickly between them, and delegating work to AI agents across all of them. You might ask an agent to implement a feature in one project, switch to another while it works, give a different agent a different task there, then keep rotating through projects as you describe features, fixes, and experiments.

Sizzle scans one or more folders for project roots, shows them in a browsable list, renders README files inside the app, and launches an AI coding agent plus a shell in split terminals for the selected project. The current built-in agent presets are Claude Code and Codex.

## Status

Sizzle is open source and usable now, but it is still early-stage and developer-oriented. The primary documented workflow today is running from source.

## Why use it

- You are juggling many repos, prototypes, tools, and half-finished ideas at once.
- You want an IDE-like workspace organized around AI agent delegation rather than manual window management.
- You want to bounce between projects quickly while multiple agents work in parallel.
- You want one place to browse projects, read context, inspect files, and launch agent-backed terminals.

## What it does

- Prompts for a projects root folder on first launch.
- Scans configured directories for likely project roots.
- Shows detected projects in a searchable list designed for hopping between many projects.
- Displays README and other markdown/text files in-app so you can recover context quickly.
- Opens project-rooted terminal sessions for an AI agent and shell workflow.
- Lets you keep multiple projects active while switching between them.
- Stores scan settings and project metadata in a local config directory.

## Requirements

- **Rust toolchain** — needed to compile the Tauri backend (install via [rustup](https://rustup.rs/))
- **Node.js** and **npm** — for the frontend build
- A working desktop environment (X11/Wayland on Linux, native on macOS/Windows)
- A supported shell on your system
- `claude` and/or `codex` installed on your `PATH` if you want to launch those agents from inside the app

Platform-specific Tauri prerequisites (system libraries, WebView2, etc.):
- Linux: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
- macOS: included with Xcode
- Windows: included with WebView2 (pre-installed on Windows 10 1803+)

See the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/) for details.

## Install

```bash
git clone https://github.com/NebuPookins/sizzle.git
cd sizzle
npm install
```

## Run In Development

```bash
npm run dev
```

This starts the Vite dev server and launches the Tauri desktop window. On first launch, Sizzle will ask you to choose the root directory it should scan for projects.

To run the frontend Vite server standalone (without the Tauri window):

```bash
npm run dev:renderer
```

## Build

```bash
npm run build
```

This produces the Tauri app bundles (`.deb`, `.AppImage`, `.dmg`, `.msi`, etc.) in `src-tauri/target/release/bundle/`.

## Config Storage

By default, Sizzle stores its local state under:

```text
~/.config/sizzle
```

That includes scan settings and project metadata.

## First-Run Flow

1. Launch the app.
2. Choose a root folder when prompted.
3. Wait for Sizzle to scan for projects.
4. Pick a project from the sidebar.
5. Read the README, inspect files, or launch a terminal/agent session.

## License

Sizzle is licensed under the GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`). See [LICENSE](LICENSE).

## Support

- Issues: <https://github.com/NebuPookins/sizzle/issues>
- Repository: <https://github.com/NebuPookins/sizzle>
- Contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
- Security: [SECURITY.md](SECURITY.md)
