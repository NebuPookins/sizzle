# Behavior

## Overview

The kanban board is a project management view that shows work items ("cards")
organized as a matrix. The columns represent workflow stages, and the rows
(lanes) are the user's projects. The user can move cards between columns, assign
them to specific LLM agents, and Sizzle will track each agent's capacity so work
gets distributed automatically.

## The Board View

There is a single global board, not one per project. It lives in the main
content area — the user reaches it by clicking "Kanban Board" in the left
sidebar, alongside their project list.

The board is a horizontally scrollable set of columns. The leftmost column
contains the project names (the row headers). Each remaining column represents a
stage in the workflow. By default, the board has these columns:

- Feature design
- LLM Coding
- Human verification
- Commit building and rebasing

The user can rename, reorder, add, or delete columns at any time (deleting a
column that still has cards in it will prompt the user to choose where the cards
should go).

Within each column, cards are grouped by project. Each card shows the project
name (small, muted), the card title, and the assigned agent badge (if any), and
whether that agent has spare capacity to perform work. Cards are stacked
vertically within their project group, sorted by their position.

## Cards

A card represents a unit of work. Each card has:

- A **title** (required)
- A **description** (optional, free text)
- The **project** it belongs to (selected from the list of scanned projects)
- An **assigned agent** (optional — selected from the list of configured agents:
  Claude, Codex, any Agent Preset the user has defined)
- An optional **git worktree** — the card can be created with an isolated
  branch checked out in a worktree directory

Cards are created by clicking the "+" button at the top of any column. This
opens a dialog where the user fills in the fields above. There is also a
checkbox labeled "Create git worktree". If the project does not have an
associated git repository, the checkbox is greyed out. If the project does have
an associated git repository, it is checked by default. When checked, Sizzle
creates a new git worktree directory, then records the worktree path on the card.

Cards can be edited (right-click → Edit), copied (right-click → Duplicate), or
deleted (right-click → Delete, with a confirmation prompt).

## Moving Cards

The user drags a card from one column to another to move it to the next stage.
They can also drag within a column to reorder cards relative to each other.

Optionally, a column can have a **WIP (work-in-progress) limit** — a maximum
number of cards allowed in that column at once. If a column has a WIP limit and
it's reached, dropping a new card into it will still work, but the column's
header counter is highlighted in red as a warning.

## Card Actions

Right-clicking a card opens a context menu with these options:

- **Edit** — opens the card dialog with current values pre-filled
- **Duplicate** — creates a copy of the card in the same column
- **Delete** — removes the card (and optionally removes its git worktree if it
  has one)
- **Run with {Agent}** — launches the dual terminal layout as used in the main
  Sizzle UI, with the current directory set to the git worktree directory, and
  the top terminal running the agent associated with the card, and the lower
  terminal running the user's default shell.

## Agent Capacity

Each configured agent (Claude, Codex, any Agent Preset) conceptually has an
internal **token capacity**, but this capacity is not currently directly
observable by sizzle. As such, Sizzle will rely on the user manually reporting
when an agent has exceeded its capacity and when that capacity will be
refreshed.

Typically, when an LLM agent exceeds capacity, it will display a message like
"You have used all your usage for the week. Usage resets at Tuesday 3PM".
Sizzle provides a timestamp picker where the user can select "Tuersday 3PM".
This signals to Sizzle that the Agent associated with the card has exceeded
capacity, and is blocked until the selected time. This will then update any
cards assigned to that agent to show that those cards are blocked until the
specified time.

Sizzle doesn't do anything else beyond show that "block" indicator. The user,
for example, is free to launch the agent anyway (though the agent will probably
just refuse to do any work), and the user can update the timestamp to any value
(past or future or null), and the "block" indicator will update accordingly.

## Agent Configuration

The agents available for card assignment come from the existing Agent Presets in
the Settings window.

## Edge Cases

- **No git repo**: If the project doesn't have a git repository, the "Create git
  worktree" checkbox is disabled with a tooltip explaining why.
- **Worktree branch name collision**: If the slugified branch name already
  exists, Sizzle appends a numeric suffix.
- **Project deleted**: If a project is removed from Sizzle's scan roots, its
  cards remain in the board but show "(project not found)" instead of the
  project name.
- **Empty board**: If there are no scanned projects, the board shows a message
  prompting the user to add scan roots.
- **Removing columns safely**: The user cannot delete the last remaining column.
  When deleting a column that contains cards, Sizzle prompts the user to choose
  a destination column for the cards.
