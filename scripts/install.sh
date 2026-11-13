#!/bin/sh
# Install the latest termden release into ~/.local (no root, no Rust needed):
#   curl -fsSL https://raw.githubusercontent.com/Sylviromi/termden/main/scripts/install.sh | sh
# Re-running updates; the app also offers new releases in its footer.
# `sh install.sh --uninstall` removes the files again (tmux sessions and
# ~/.config/termden are left alone).
set -eu

REPO=Sylviromi/termden
ASSET=termden-x86_64-linux.tar.gz
PREFIX="${TERMDEN_PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor/scalable/apps"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN/termden" "$BIN/termdenctl" "$APPS/termden.desktop" "$ICONS/termden.svg"
    echo "termden removed from $PREFIX"
    exit 0
fi

[ "$(uname -s)-$(uname -m)" = "Linux-x86_64" ] || { echo "termden: only Linux x86_64 builds are published" >&2; exit 1; }
for tool in curl tar; do
    command -v "$tool" >/dev/null || { echo "termden: needs $tool" >&2; exit 1; }
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading the latest termden release..."
curl -fSL --progress-bar -o "$tmp/$ASSET" "https://github.com/$REPO/releases/latest/download/$ASSET"
tar -xzf "$tmp/$ASSET" -C "$tmp"

mkdir -p "$BIN" "$APPS" "$ICONS"
# Rename into place so a running termden sees a whole new binary and reloads.
for bin in termdenctl termden; do
    install -m755 "$tmp/termden/$bin" "$BIN/.$bin.new"
    mv -f "$BIN/.$bin.new" "$BIN/$bin"
done
sed "s|^Exec=.*|Exec=$BIN/termden|" "$tmp/termden/termden.desktop" > "$APPS/termden.desktop"
install -m644 "$tmp/termden/termden.svg" "$ICONS/termden.svg"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" 2>/dev/null || true

echo "Installed $("$BIN/termden" --version 2>/dev/null || echo termden) into $BIN"

missing=""
command -v tmux >/dev/null || missing="$missing\n  - tmux (every tile runs in it)"
command -v xdg-open >/dev/null || missing="$missing\n  - xdg-open (opening links)"
ls /usr/share/vulkan/icd.d/*.json >/dev/null 2>&1 || ls /etc/vulkan/icd.d/*.json >/dev/null 2>&1 || missing="$missing\n  - a Vulkan driver (mesa-vulkan-drivers / vulkan-radeon / nvidia)"
[ -n "$missing" ] && printf "Still needed:%b\n" "$missing"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) echo "Add $BIN to your PATH to run termden from a shell." ;;
esac
echo "Start it from your app launcher or run: termden"
