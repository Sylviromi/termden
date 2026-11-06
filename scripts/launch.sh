#!/usr/bin/env bash
# Launcher entry point (see assets/termden.desktop): focus the running
# termden if there is one, else start the dev build. Rebuilds hot-reload it.
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if "$ROOT/target/debug/termdenctl" ping >/dev/null 2>&1; then
    command -v hyprctl >/dev/null && hyprctl -q dispatch focuswindow class:termden
    exit 0
fi
[ -x "$ROOT/target/debug/termden" ] || cargo build -q -p termden-app -p termden-cli --manifest-path "$ROOT/Cargo.toml" || exit 1
cd "$HOME" && exec "$ROOT/target/debug/termden"
