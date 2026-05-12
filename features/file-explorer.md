# Behavior

The file explorer widget is divided into 2 panes. On the left is the directory
tree, where the user can click on directory nodes to expand or collapse them.
If the user clicks on a file, then the pane on the right shows a preview of
the contents of that file.

If the file is text, we show the text (ideally, we detect the syntax of the
text file and perform syntax highlighting where possible).

If the file is a basic supported image type, we show the image in the right
panel.

If the file is a zip or other supported archive file, we show the list of
files in the archive, but we do not need to recursively show the contents
of those files.
