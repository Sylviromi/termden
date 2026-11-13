#!/usr/bin/env bash
# Development loop: run termden under `dx serve --hot-patch` (Subsecond).
#
# Saving a source file patches the change into the running app in seconds;
# the window, state and tiles stay. Edits Subsecond cannot patch (struct
# fields, startup code, termden-core) make dx rebuild and relaunch termden,
# and every tile reattaches from tmux.
#
# Usage: scripts/dev.sh   start it in the background (also the launcher entry);
#                         focuses the window when termden already runs.
# Watch: tmux -L termden attach -t termden-dx (build errors show there).
#        `termdenctl reload` (or ctrl+shift+r) = full rebuild + relaunch;
#        needed after changing struct fields, which dx would patch unsafely.
# Needs: cargo install dioxus-cli --version 0.7.10 --locked
# Without dx: cargo build -p termden-app -p termden-cli && target/debug/termden
#   (a plain build execs into its rebuilt binary, closing the window briefly).

set -uo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if target/debug/termdenctl ping >/dev/null 2>&1; then
    command -v hyprctl >/dev/null && hyprctl -q dispatch focuswindow class:termden
    exit 0
fi
DX="$(command -v dx || echo "$HOME/.cargo/bin/dx")"
[ -x "$DX" ] || { echo "dev.sh: needs dx: cargo install dioxus-cli --version 0.7.10 --locked" >&2; exit 1; }
# Tiles get termdenctl from here; dx builds termden into its own directory.
cargo build -q -p termden-cli || exit 1

# dx runs interactively in a tmux session next to the tiles, so `termdenctl
# reload` can press its `r` (full rebuild + relaunch) and you can watch it:
#   tmux -L termden attach -t termden-dx
SOCK="${TERMDEN_TMUX_SOCKET:-termden}"
# Launched from a menu nothing else would show for the ~10s dx needs.
say() {
    echo "$1"
    command -v notify-send >/dev/null && notify-send -a termden -i termden "termden" "$1"
}
if tmux -L "$SOCK" has-session -t termden-dx 2>/dev/null; then
    say "Still starting; the window opens on the workspace you launched it from."
    exit 0
fi
ENV=(-e "TERMDEN_BIN_DIR=$ROOT/target/debug" -e "TERMDEN_TMUX_SOCKET=$SOCK")
for var in PATH WAYLAND_DISPLAY DISPLAY XDG_RUNTIME_DIR TERMDEN_CONTROL_SOCKET \
    TERMDEN_STATE_FILE VK_ICD_FILENAMES; do
    [ -n "${!var:-}" ] && ENV+=(-e "$var=${!var}")
done
tmux -L "$SOCK" new-session -d -s termden-dx -x 160 -y 48 -c "$ROOT" "${ENV[@]}" \
    "$DX serve --hot-patch --platform linux -p termden-app --bin termden --features hotpatch"
say "Starting under dx (about 10s, minutes after a dependency change)."
echo "Watch it: tmux -L $SOCK attach -t termden-dx"
