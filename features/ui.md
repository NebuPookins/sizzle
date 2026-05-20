# Behavior

In the primary workflow, the UI is split into 3 sections: a left pane, a main
center pane, and a right pane.

The left pane shows a list of projects, and clicking on one of those projects
changes the contents of the main pane to shwo that project. The right pane
also changes to show the git state of that project, namely what branch we are
currently on, how many commits have not yet been pushed up stream, what files
are modified, staged, untracked, etc.

Note that the git panel should be associated no with the project, but with a
editor tab of the project. It's possible for different editor tabs within a
project to be associated with different git worktrees, for example, and each
will have its own git panel shown its own git status.

By default, projects are not "active", and so the main pane will show a list
of tabs, one for each markdown file found in the project root, with the
README.md file shown first (if present), and then with all subsequent
markdown files sorted alphabetically. Finally, after all those markdown files,
the last tab is "Explorer".

Clicking on the tab for any markdown file shows the rendered contents for that
markdown file.

Clicking on the "Explorer" tab shows a directory tree explorer.

In the top right corner of the main panel are a bunch of buttons that let you
launch the LLM coding agents. The built in options are "Launch Claude",
"Launch Codex" and "Shell", but the user can add more in the settings menu.

When the user clicks on one of these buttons, we do a couple of things:
- We set the project to "active".
- We set the "last active" time of the project to now.
- We create a new tab at the beginning of the tab list (before the README.md
  tab), and then select that tab.
- In that tab, the main panel is split vertically into three.
  - The top section shows a terminal running the selected LLM Coding agent
    with the current directory set to the project root.
  - The middle section is a thin dividing bar.
  - The bottom section shows a terminal running the user's default shell
    with the current directory set to the project root.
- We set the focus to the top panel. I.e. if the user starts typing, their
  keypresses are sent to the LLM coding agent.

Sizzle should dim or highlight terminal panels to indicate which terminal
has focus. For example, if the user clicks on the top terminal, then the
top panel should be lit and the bottom panel should become dim. If the user
clicks on the bottom terminal, the bottom terminal should be lit and the
top terminal should become dim.

As the user switches between different projects, Sizzle should remember
which terminal had focus, and restore the focus as appropriate when they
switch back to the previous project. For example, if the user clicks on
project A, then clicks on the top panel and starts typing something,
whatever they type should be sent to the top terminal panel of project A.
Then, if the user clicks on project B, and then clicks on the bottom
panel and starts typing something, whatever they type should be sent to
the bottom terminal panel. If the user then clicks on project A, and
starts typing again, whatever they type should be sent to the top
terminal of project A, even though the user didn't click on the top
terminal.

The middle dividing bar should itself contain tabs. At first, there will be
only one tab, labeled "Shell 1". There is also a button to create new
shells, which creates a new tab and a corresponding terminal and shell
process. The user can click on the different tabs to switch between the
different shells. The currently active tab should be highlighted.

The user can also drag the middle dividing bar up and down to resize the
top and bottom panels. Hovering over this middle bar should change the mouse
cursor to indicate that resizing is possible.

If there are multiple shell tabs, then the tabs have a small "Close" button
which the user can click on to close those tabs. The user can also press
CTRL-W to close the currently focused tab. If there is only 1 shell left,
the user cannot close that shell.

The processes associated with the two terminals can terminate. If all
processes associated with a project have terminated then:
- We start a 5 second timer.
- After 5 seconds, we mark the project as no longer active, which also
  updates the UI so that the two terminals view is hidden and we're
  back to displaying the markdown files for the project. The idea is
  that we're giving time for the user to read any final output from the
  process (shell commands or LLM coding agent) before closing them.
- If the user navigates to another project, then we cancel the 5 second
  timer, and we immediately mark the project as no longer active.
