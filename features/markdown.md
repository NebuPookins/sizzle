# Behavior

In the main UI panel, we list all the markdown files found in the top level
directory as separate tabs (alongside the tab to actually see the terminal for
the LLM coding agent). Click on these tabs lets you view the relevant markdown
file.

When viewing a markdown file, there is also an "Edit" button visible. The
button should constantly be visible as the user scrolls up and down the
contents of the file.

Clicking the "Edit" button switches to edit mode, where the user can make
changes to the contents to the markdown file. The "Edit" button switches to a
"Save" button. The user can either click the "Save" button when they're done
or press CTRL-S to save the file, at which point the contents of the file are
written to disk, and the "Save" button switches back to an "Edit" button.

The markdown parser we use should support syntax highlighting in multiple
different programming languages.