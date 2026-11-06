#!/usr/bin/env bash
# Development loop for working on termden from inside termden.
#
# Rebuilds on every source change. A running termden notices its binary was
# replaced and reloads itself: it saves state, detaches from tmux and execs the
# new build, so every harness (including the agent editing the code) survives.
# A failed build changes nothing; the old UI keeps running.
#
# Usage: scripts/dev.sh          (run it in a termden shell tile)
#        scripts/dev.sh --launch (also start termden if it is not running)

set -uo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

build() {
    printf '\n\033[2m── %s rebuilding ──\033[0m\n' "$(date +%T)"
    local log
    log="$(mktemp)"
    cargo build -p termden-app -p termden-cli 2>&1 | grep -Ev '^\s*(Compiling|Checking)' | tee "$log"
    if [ "${PIPESTATUS[0]}" -eq 0 ]; then
        printf '\033[32mok\033[0m · running termden reloads itself\n'
    else
        printf '\033[31mbuild failed\033[0m · termden keeps the previous build\n'
        # Surface it in the app's activity feed too (no-op if it is not running).
        "$ROOT/target/debug/termdenctl" log "build failed: $(grep -m1 '^error' "$log" | cut -c1-120)" 2>/dev/null || true
    fi
    rm -f "$log"
}

build
if [ "${1:-}" = "--launch" ] && ! "$ROOT/target/debug/termdenctl" ping >/dev/null 2>&1; then
    setsid "$ROOT/target/debug/termden" >/dev/null 2>&1 &
fi

command -v inotifywait >/dev/null || { echo "dev.sh: needs inotifywait (inotify-tools)" >&2; exit 1; }
while inotifywait -qq -r -e close_write,move,create,delete \
    --exclude '(/target/|\.git/|~$|\.swp$)' crates Cargo.toml; do
    # Let editors finish multi-file writes before building.
    sleep 0.3
    build
done
