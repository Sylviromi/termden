#!/usr/bin/env bash
# Render termden into a PNG without touching the real desktop.
#
# GPUI's own `Application::headless()` cannot open windows, so this starts a
# private Weston compositor using its headless backend (1440x900 memory
# surface), runs termden inside it with the kiosk shell (no panel, no
# decorations), and captures the output with weston-screenshooter.
#
# Usage:
#   scripts/headless-shot.sh [output.png] [width] [height]
#
# Env:
#   TERMDEN_WARMUP   seconds to let the app settle (default 3)
#   TERMDEN_SHOT_MODE
#       demo   (default) the mock fleet; only shell tiles are live
#       fresh  a real first-run instance with throwaway state, tmux server and
#              control socket (exported as TERMDEN_CONTROL_SOCKET for SETUP)
#       mirror read-only twin of the running termden: same state file, same
#              tmux sessions (what `termdenctl screenshot` uses)
#   TERMDEN_SHOT_SETUP  shell snippet run after warmup, before capture, e.g.
#              'termdenctl add claude; sleep 2' (fresh mode)
#   TERMDEN_SHOT_NOBUILD=1  skip the rebuild (a rebuild reloads a live app)
#   VK_ICD_FILENAMES override the Vulkan driver (default: lavapipe if present)
#
# The script never connects to $WAYLAND_DISPLAY / $DISPLAY.

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/target/termden-headless.png}"
WIDTH="${2:-1440}"
HEIGHT="${3:-900}"

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

TMP="$(mktemp -d)"

# Consume SETUP so nested runs (`termdenctl screenshot` inside SETUP) do not
# inherit it: that recursed forever, each level adding a claude harness.
SETUP="${TERMDEN_SHOT_SETUP:-}"
unset TERMDEN_SHOT_SETUP

# GPUI and Weston both draw a cursor. The host's "default" Xcursor theme can be
# a custom-painted sprite, which would leak into every capture. Point both at a
# fully transparent 24x24 theme generated here.
ICONS="$TMP/icons"
mkdir -p "$ICONS/default/cursors"
python3 - "$ICONS" <<'PY'
import os, struct, sys

root = sys.argv[1]
pixels = b"\x00\x00\x00\x00" * (24 * 24)
image = struct.pack(
    "<IIIIIIIII", 36, 0xFFFD0002, 24, 1, 24, 24, 0, 0, 0
) + pixels
blob = (
    struct.pack("<IIII", 0x72756358, 16, 0x10000, 1)
    + struct.pack("<III", 0xFFFD0002, 24, 16 + 12)
    + image
)
names = [
    "left_ptr", "default", "pointer", "text", "crosshair", "move", "wait",
    "progress", "help", "not-allowed", "ns-resize", "ew-resize",
    "nwse-resize", "nesw-resize", "grab", "grabbing", "zoom-in", "zoom-out",
]
for name in names:
    with open(os.path.join(root, "default", "cursors", name), "wb") as f:
        f.write(blob)
with open(os.path.join(root, "default", "index.theme"), "w") as f:
    f.write("[Icon Theme]\nName=termden blank\nInherits=Adwaita\n")
PY

export XCURSOR_PATH="$ICONS"
export XCURSOR_THEME="default"

for tool in weston weston-screenshooter python3; do
    command -v "$tool" >/dev/null || {
        echo "headless-shot: missing $tool" >&2
        exit 1
    }
done

SOCKET="termden-headless-$$"
MODE="${TERMDEN_SHOT_MODE:-demo}"
APP_ARGS=()
OWN_TMUX=1
case "$MODE" in
    demo) APP_ARGS=(--demo) ;;
    fresh)
        export TERMDEN_STATE_FILE="$TMP/state.json"
        export TERMDEN_CONTROL_SOCKET="$TMP/app.sock"
        ;;
    mirror)
        export TERMDEN_MIRROR=1
        OWN_TMUX=0
        ;;
    *) echo "headless-shot: unknown TERMDEN_SHOT_MODE=$MODE" >&2; exit 2 ;;
esac
# Keep captures off the user's real tmux server, except when mirroring it.
if [ "$OWN_TMUX" = 1 ]; then
    export TERMDEN_TMUX_SOCKET="$SOCKET"
fi
WESTON_PID=""
APP_PID=""

cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
    [ -n "$WESTON_PID" ] && kill "$WESTON_PID" 2>/dev/null || true
    if [ "$OWN_TMUX" = 1 ]; then
        tmux -L "$TERMDEN_TMUX_SOCKET" kill-server 2>/dev/null || true
    fi
    wait 2>/dev/null || true
    cp "$TMP/app.log" "$ROOT/target/headless-app.log" 2>/dev/null || true
    cp "$TMP/weston.log" "$ROOT/target/headless-weston.log" 2>/dev/null || true
    rm -rf "$TMP"
}
trap cleanup EXIT

# Always rebuild: the script is also the quick visual check during development.
if [ -z "${TERMDEN_SHOT_NOBUILD:-}" ]; then
    cargo build -q -p termden -p termdenctl --manifest-path "$ROOT/Cargo.toml"
fi
export PATH="$ROOT/target/debug:$PATH"

# Software Vulkan keeps the capture independent of the host GPU. Without this,
# blade can pick a driver whose Wayland surface does not survive the virtual
# output (ERROR_SURFACE_LOST_KHR).
LVP="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -1 || true)"
if [ -z "${VK_ICD_FILENAMES:-}" ] && [ -n "$LVP" ]; then
    VK_ICD_FILENAMES="$LVP"
fi

weston \
    --backend=headless \
    --width="$WIDTH" \
    --height="$HEIGHT" \
    --socket="$SOCKET" \
    --no-config \
    --shell=kiosk \
    --idle-time=0 \
    --renderer=pixman \
    --fake-seat \
    --debug \
    --log="$TMP/weston.log" >>"$TMP/weston.log" 2>&1 &
WESTON_PID=$!

# Wait for the socket instead of sleeping blindly.
for _ in $(seq 1 50); do
    [ -S "$XDG_RUNTIME_DIR/$SOCKET" ] && break
    sleep 0.1
done

if [ -n "${VK_ICD_FILENAMES:-}" ]; then
    WAYLAND_DISPLAY="$SOCKET" VK_ICD_FILENAMES="$VK_ICD_FILENAMES" \
        "$ROOT/target/debug/termden" "${APP_ARGS[@]}" >"$TMP/app.log" 2>&1 &
else
    WAYLAND_DISPLAY="$SOCKET" "$ROOT/target/debug/termden" "${APP_ARGS[@]}" >"$TMP/app.log" 2>&1 &
fi
APP_PID=$!

sleep "${TERMDEN_WARMUP:-3}"

if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "headless-shot: termden exited before capture" >&2
    tail -20 "$TMP/app.log" >&2
    exit 1
fi

if [ -n "$SETUP" ]; then
    bash -c "$SETUP" || echo "headless-shot: setup failed" >&2
fi

mkdir -p "$(dirname "$OUT")"
# Headless Weston does not answer a frame callback attached to an empty
# commit (real compositors do, every vblank), so GPUI stalls on its last frame
# once it goes idle. A capture forces a repaint, which releases the callback
# and lets the app draw its current state; the second capture records that.
shoot() {
    ( cd "$TMP" && WAYLAND_DISPLAY="$SOCKET" timeout 15 weston-screenshooter >/dev/null 2>&1 ) || true
}
shoot
sleep 0.6
rm -f "$TMP"/wayland-screenshot-*.png
shoot

SHOT="$(ls -t "$TMP"/wayland-screenshot-*.png 2>/dev/null | head -1 || true)"
if [ -z "$SHOT" ]; then
    echo "headless-shot: capture failed" >&2
    tail -20 "$TMP/weston.log" >&2
    exit 1
fi

cp "$SHOT" "$OUT"
echo "$OUT"
