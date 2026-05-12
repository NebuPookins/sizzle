# Behavior

In the primary workflow, the UI is split into 3 sections: a left pane, a main
center pane, and a right pane.

The left pane shows a list of projects, and clicking on one of those projects
changes the contents of the main pane to shwo that project. The right pane
also changes to show the git state of that project, namely what branch we are
currently on, how many commits have not yet been pushed up stream, what files
are modified, staged, untracked, etc.

By default, projects are not "active", and so the main pane will show a list
of tabs, one for each markdown file found in the project root, with the
README.md file shown first (if present), and then with all subsequent
markdown files sorted alphabetically. Finally, after all those markdown files,
the last tab is "Explorer".

Clicking on the tab for any markdown file shows the rendered contents for that
markdown file.

Clicking on the "Explorer" tab shows a directory tree explorer.
