# Behavior

## Overview 

In the primary worflow, the left pane of the UI shows:

- A filter search box
- A list of projects as a vertically oriented list of project widgets.
- A button to open the options/settings menu.
- A "Status" box that shows the currently amount of memory consumed by Sizzle.

## Filter Search Box

The filter search box lets the user type in some short text, and then filters
the list of projects to only show those where the project name contains the
provided text as a substring (smart case sensitivity, meaning if the user's
input is all lower case, then the filter is case insensitive, but if the
user's input contains at least one upper case character, the filter is case
sensitive).

## List of Projects

Each project widget shows the following components:
- Active status.
- The name of the project (derived from the name of the project root folder).
- When the project was last active as a relative date (e.g. "5 minutes ago").
- A "favorite" status/button.
- The primary "tag" of the project (essentially, what is the primary
  programming language or framework associated with the project, e.g. "Rust",
  "React", etc.).

The "Active" status shows whether the project is active (i.e. are there
active terminals running LLM coding agents associated with the project.) If
the project is active, a dot is shown. If the project is not active, no dot is
shown.

The dot should be green if the LLM coding agent is idle or waiting for user
input, and yellow if the LLM coding agent is working or busy. There does not
seem to be a standard API for getting the status of the LLM coding agent, so
we use heuristics here: If the LLM coding agent is sending text to the
terminal, we presume it is busy, and if it has not sent any text to the
terminal for the last, say, 5 seconds, we assume it is idle and waiting for
input from the user.

The "favorite" status button can have 1 of three states:
- default
- favorite
- trash

Every project starts in the "default" state, and clicking on the button cycles
between the three states.

The list of projects are sorted in the following order:

- Active projects are shown before inactive projects.
- In case of tie, favorite projects are shown first, then default projects, then
  trash projects.
- In case of tie, projects are sorted by date with most-recently-active projects
  shown first.
- In case of tie, the projects are then sorted alphabetically by name.

### List of Projects - Context Menu

Right clicking on a project window should show a context menu with the
following options:

- "Move/Rename Project"
- "Add to Ignored Roots"

### List of Projects - Context Menu - Move/Rename Project

"Move/Rename projects" lets the user enter in a new path. If the new path
is under any of the "ignore roots", a warning is displayed.

The project will then be moved to that path, and additionally the following
things will happen:

- We create an empty list of "changes" which we will gradually populate and
  then display to the user for them to review.
- We actually move or rename the project root directory to reflect the new
  path. We also record this as one of the changes in our list of changes.
- If the project was under a scan root and the destination would no longer be
  under any scan root, then the project's path is added to the "manually added
  project roots".
- If the project was not under a scan root, that means the project must have
  been a manually added project root. That "manually added project root" entry
  is modified to reflect the new chosen path.
- If there is a directory "~/.claude/projects", then we presume Claude is
  installed on the user's system. There will be a folder for every project
  managed by Claude code, mangled so that the full path can be represented as
  as a single directory. We will rename that folder to reflect the new path,
  and also add this as one of the changes e.g. `changes.push("Moved Claude project data:\n ${oldClaudeDir}\n -> ${newClaudeDir}")`.
- Similarly, if there is a file "~/.codex/config.toml", we presume that the
  user has Codex installed, and we update the file to reflect the new path.
  We also add this to the list of changes.
- Finally, we show the list of changes we performed to the user.

### List of Projects - Context Menu - Add to Ignored Roots

This appends the project's path to our list of ignored project roots.

## Memory Consumed

It is a high priority for the Sizzle project to consume as little amount of memory
as possible. The status widget which shows memory consumption is designed to make
it very apparent when Sizzle is consuming too much memory so that corrective
action can be taken.

The exact contents of the memory widget will depend on the implementation, but
should err towards providing detailed information. For example, if Sizzle is using
a Tauri based implementation, the memory widget should show memory usaged broken
down into:

- How much RAM the Rust core is using.
- How much RAM the WebKit process is using.
- How much RAM the LLM Agent processes are using.

This should be implemented by walking the process tree, categorizing the process
children and grandchildren into buckets, then summing up the RAM usage in each
bucket.
