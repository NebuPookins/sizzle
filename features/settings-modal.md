# Behavior

Near the bottom of the left panel is a button to open the settings or options
modal.

In that modal, we can configure the following things:

- Scan roots
- Paths to ignore
- Manually added project roots
- Custom Agent Presets.

A scan root is a list of zero or more absolute paths. When the app starts up,
if there are zero scan roots, then a first-time wizard pops up asking the
user to specify a scan root.

A sizzle will monitor all files under all provided scan roots and use a
heuristic to detect the presence of projects. If the underlying OS provides
some sort of "recursive file watch" API, we can use that. Otherwise, we might
just poll the directory tree e.g. every 5 seconds.

The heuristic will be documented elsewhere, but it includes things like "If
there is a git root, i.e. the existence of a `.git` folder, then this is
probably a project."

Any paths under "Paths to ignore" will be ignored. We will not scan under
these paths, and even if a folder passes our "project detection heuristic",
if it lies under that path, we will not add it to the list of projects.

Every path in "Manually added project root" is considered a project, even
if that folder fails the "project detection heuristic".

"Custom Agent Presets" is a list of pairs consisting of a label and a
command. A label might be .e.g "DeepSeek" and a command might be
"/usr/bin/deepseek".

In the main UI, in the top right corner, we have buttons for launching
LLM Coding agents. The built in defaults are like "Launch Claude",
"Launch Codex", and "Shell". For every pair in "Custom Agent Presets",
we add an addition button, whose label is the provided label. When that
button is clicked on, we create a new launch the specific command as
if it were a coding LLM agent, following the same logic was
"Launch Claude" or "Launch Codex": i.e. we create a pair of new
terminals and set the current directory of both of them to be the
project root. In the top terminal, we launch the coding agent, and in
the bottom terminal, we launch the user's default shell.


