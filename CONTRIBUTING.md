# Contributing to Sizzle

Thanks for contributing.

## Before you start

- Open an issue first for large changes, behavior changes, or significant UI changes.
- Keep pull requests focused. Small, reviewable changes move faster.

## Development setup

```bash
git clone https://github.com/NebuPookins/sizzle.git
cd sizzle
cargo run -p sizzle-gtk
```

Notes:

- The **Rust toolchain** must be installed (via [rustup](https://rustup.rs/)).
- **GTK4 development libraries** are required on Linux. See the [README](README.md) for platform-specific install commands.

## Project notes

- Binary entry: `crates/sizzle-gtk/src/main.rs`
- Core library: `crates/sizzle-core/` (scanner, metadata, files, git)
- GTK4 UI components live in `crates/sizzle-gtk/src/`
- Local metadata is stored under `~/.config/sizzle` by default.

This repo also includes agent-assistance files such as `AGENTS.md` and `CLAUDE.md`. They are repository guidance files, not a requirement for contributing.

## Pull request checklist

- The change builds cleanly with `cargo build --release -p sizzle-gtk`.
- New behavior is documented where needed.
- No secrets, local paths, or personal config were added.
- The diff does not include unrelated cleanup.

## Reporting bugs

Use the GitHub issue tracker:

<https://github.com/NebuPookins/sizzle/issues>
