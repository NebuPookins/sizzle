# Behavior

In the primary worflow, the left pane of the UI shows:

- A filter search box
- A list of projects as a vertically oriented list of project widgets.
- A button to open the options/settings menu.
- A "Status" box that shows the currently amount of memory consumed by Sizzle.

The filter search box lets the user type in some short text, and then filters
the list of projects to only show those where the project name contains the
provided text as a substring (smart case sensitivity, meaning if the user's
input is all lower case, then the filter is case insensitive, but if the
user's input contains at least one upper case character, the filter is case
sensitive).

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
