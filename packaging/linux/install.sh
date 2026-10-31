#!/bin/sh
# Installs sqlsea for the current user: the binary, its icons and a launcher entry.
# Works from an unpacked release (sqlsea beside this script) or from the source
# tree after `cargo build --release`, started from any folder.
# Undo with `install.sh --remove`.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
if [ -f "$here/sqlsea" ]; then
    # An unpacked release.
    app="$here/sqlsea"
    png="$here/icons/icon-"
    svg="$here/icons/icon.svg"
    desktop="$here/sqlsea.desktop"
else
    # The source tree, two levels up from packaging/linux.
    root=$(cd "$here/../.." && pwd)
    app="$root/target/release/sqlsea"
    png="$root/brand/png/icon-"
    svg="$root/brand/svg/icon.svg"
    desktop="$here/sqlsea.desktop"
fi

BIN="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor"
SIZES="16 24 32 48 64 128 256 512"

if [ "${1:-}" = "--remove" ]; then
    rm -f "$BIN/sqlsea" "$APPS/sqlsea.desktop"
    for s in $SIZES; do rm -f "$ICONS/${s}x${s}/apps/sqlsea.png"; done
    rm -f "$ICONS/scalable/apps/sqlsea.svg"
    echo "Removed sqlsea."
    exit 0
fi

[ -x "$app" ] || { echo "Build first: cargo build --release" >&2; exit 1; }

install -Dm755 "$app" "$BIN/sqlsea"
for s in $SIZES; do
    install -Dm644 "$png$s.png" "$ICONS/${s}x${s}/apps/sqlsea.png"
done
install -Dm644 "$svg" "$ICONS/scalable/apps/sqlsea.svg"
# The desktop session's PATH often lacks ~/.local/bin, and launchers hide
# entries whose program they cannot find, so point at the binary directly.
mkdir -p "$APPS"
sed -e "s|^Exec=sqlsea|Exec=$BIN/sqlsea|" -e "s|^TryExec=.*|TryExec=$BIN/sqlsea|" \
    "$desktop" > "$APPS/sqlsea.desktop"
chmod 644 "$APPS/sqlsea.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
echo "Installed sqlsea to $BIN. It is in your app launcher now."
