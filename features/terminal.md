# Behavior

When a project is active, in addition to showing a panel where we can interact
with the the LLM coding agent, we also provide a terminal so that the user can
navigate around and explore the project tree. The user may, for example, want to
inspect specific file contents (source files, or data files edited by the
program), inspect the state of the git repository, run various utilities
(linters, try to compile the program, run the unit tests, etc.)

The user should be able to select text in the terminal and right click to show
a context menu containing the "Copy" command to copy the selected text into the
clipboard.

The user should be able to right click into the terminal to show a context menu
containing the "Paste" command to paste text from the clipboard into the
terminal.

This pop up context menu should appear near where the cursor was at the time
of the right click.