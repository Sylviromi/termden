#!/usr/bin/env bash
# Start the dev build of termden (also the launcher entry), or focus it.
#
# A running termden notices when `cargo build -p termden` replaces its binary
# and offers a restart in the sidebar; it never restarts unasked. Restarting
# saves state, detaches from tmux and execs the new build, so every harness
# (including the agent editing the code) survives.

set -uo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if target/debug/termdenctl ping >/dev/null 2>&1; then
    command -v hyprctl >/dev/null && hyprctl -q dispatch focuswindow class:termden
    exit 0
fi
cargo build -q -p termden || {
    command -v notify-send >/dev/null && notify-send -a termden -i termden "termden" "termden build failed; see cargo build -p termden"
    exit 1
}
cd "$HOME" && setsid "$ROOT/target/debug/termden" >/dev/null 2>&1 &
