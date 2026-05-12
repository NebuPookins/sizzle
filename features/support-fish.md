# Behavior

As a user, when I open a new terminal and I'm using Fish as my shell, everything
should just work.

# Edge cases

## Primary Device Attribute query

Fish sends a "Primary Device Attribute query" `\e[c` when it starts up. Whatever
terminal emulation system Sizzle uses, it should not hang when this happens.