#!/bin/sh

#
# Cyclic build script triggered by file changes. Useful when editing with an ordinary
# text editor.
#
# yaourt -S inotify-tools
#

# Some statistics
find src -name "*.rs" | xargs wc -l

# Cargo update
if test "`find Cargo.lock -mtime +7`" ; then
  cargo update
fi

# The initial build
cargo test

# Build on demand without warnings
while inotifywait -e close_write -r . &>/dev/null ; do sync ; reset ; cargo test ; done

