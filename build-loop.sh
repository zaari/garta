#!/bin/sh

#
# Cyclic build script triggered by file changes. Useful when editing with an ordinary
# text editor.
#
# yaourt -S inotify-tools
#

# Some statistics
find src -name "*.rs" | xargs wc -l

# Let's use incremental build if available
export CARGO_INCREMENTAL=1

# Configure for testing
./configure.sh --prefix . >/dev/null

# The initial build
cargo test

# Build on demand
while inotifywait -e close_write -r . &>/dev/null ; do sync ; reset ; cargo test ; done

