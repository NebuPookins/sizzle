# Sizzle

Sizzle is an IDE-style desktop app built for vibe coding first.

The core idea is not just opening one repo and staying there. It is managing tens to hundreds of active local projects at the same time, moving quickly between them, and delegating work to AI agents across all of them. You might ask an agent to implement a feature in one project, switch to another while it works, give a different agent a different task there, then keep rotating through projects as you describe features, fixes, and experiments.

Sizzle scans one or more folders for project roots, shows them in a browsable list, renders README files inside the app, and launches an AI coding agent plus a shell in split terminals for the selected project. The current built-in agent presets are Claude Code and Codex.

## Status

Sizzle is open source and usable now, but it is still early-stage and developer-oriented. The primary documented workflow today is running from source.

## Why use it

- You are juggling many repos, prototypes, tools, and half-finished ideas at once.
- You want an IDE-like workspace organized around AI agent delegation rather than direct coding.
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

- **Rust toolchain** — install via [rustup](https://rustup.rs/)
- **GTK4 development libraries** (Linux only for now)
- A supported shell on your system
- `claude` and/or `codex` installed on your `PATH` if you want to launch those agents from inside the app

On Debian/Ubuntu:

```bash
sudo apt install libgtk-4-dev
```

On Fedora:

```bash
sudo dnf install gtk4-devel
```

On Arch:

```bash
sudo pacman -S gtk4
```

## Install

```bash
git clone https://github.com/NebuPookins/sizzle.git
cd sizzle
```

## Run In Development

```bash
cargo run -p sizzle-gtk
```

On first launch, Sizzle will ask you to choose the root directory it should scan for projects.

## Build

```bash
cargo build --release -p sizzle-gtk
```

The binary will be at `target/release/sizzle-gtk`.

## Debugging

### Memory Leaks

A helper script runs the app under Valgrind with GTK4 and GLib suppression files to filter out false positives:

```bash
./scripts/valgrind.sh
```

The script builds with debug symbols (set in `Cargo.toml`'s dev profile) then launches Valgrind. Options passed to the script are forwarded to the binary.

Valgrind settings are read from `.valgrindrc` in the project root — override any flag there or pass `--show-leak-kinds=all` to see still-reachable allocations as well.

## Config Storage

By default, Sizzle stores its local state under:

```text
~/.config/sizzle
```

That includes scan settings and project metadata.

## License

Sizzle is licensed under the GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`). See [LICENSE](LICENSE).

## Support

- Issues: <https://github.com/NebuPookins/sizzle/issues>
- Repository: <https://github.com/NebuPookins/sizzle>
- Contributing: [CONTRIBUTING.md](CONTRIBUTING.md)
- Security: [SECURITY.md](SECURITY.md)
