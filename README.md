# Sizzle

Sizzle is an IDE-style desktop app built for vibe coding first.

The core idea is not just opening one repo and staying there. It is managing tens to hundreds of active local projects at the same time, moving quickly between them, and delegating work to AI agents across all of them. You might ask an agent to implement a feature in one project, switch to another while it works, give a different agent a different task there, then keep rotating through projects as you describe features, fixes, and experiments.

Sizzle scans one or more folders for project roots, shows them in a browsable list, renders README files inside the app, and launches an AI coding agent plus a shell in split terminals for the selected project. The current built-in agent presets are Claude Code and Codex.

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
- Supports relaunching the app core while preserving terminal state.
- Stores scan settings and project metadata in a local config directory.

## Requirements

- Node.js
- npm
- A working desktop environment for Electron
- A supported shell on your system
- `claude` and/or `codex` installed on your `PATH` if you want to launch those agents from inside the app

Notes:

- `node-pty` is a native dependency and is rebuilt during `npm install`.
- If Electron is missing after install, run `node node_modules/electron/install.js`.
- On Linux, `npm run dev` needs `DISPLAY` set so Electron can open a window.

## Install

```bash
npm install
```

The `postinstall` script rebuilds the native PTY dependency automatically.

## Run In Development

```bash
npm run dev
```

On first launch, Sizzle will ask you to choose the root directory it should scan for projects.

## Build

```bash
npm run build
```

This produces the Electron app bundles under `out/`.

## Preview The Built App

```bash
npm run preview
```

## Config Storage

By default, Sizzle stores its local state under:

```text
~/.config/sizzle
```

That includes scan settings, project metadata, PTY host state, and reload state.

To run Sizzle against a different config directory, pass:

```bash
npm run dev -- --sizzle-config-dir=/tmp/sizzle-test
```

This is useful for testing first-run behavior without touching your normal config.

## First-Run Flow

1. Launch the app.
2. Choose a root folder when prompted.
3. Wait for Sizzle to scan for projects.
4. Pick a project from the sidebar.
5. Read the README, inspect files, or launch a terminal/agent session.
