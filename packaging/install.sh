#!/bin/sh
# Installs sqlsea for the current user: the binary, its icons and a launcher entry.
# Run after `cargo build --release`, from any folder. Undo with `install.sh --remove`.
set -eu

# Work from the repository root, wherever the script was started.
cd "$(dirname "$0")/.."

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

[ -x target/release/sqlsea ] || { echo "Build first: cargo build --release" >&2; exit 1; }

install -Dm755 target/release/sqlsea "$BIN/sqlsea"
for s in $SIZES; do
    install -Dm644 "brand/png/icon-$s.png" "$ICONS/${s}x${s}/apps/sqlsea.png"
done
install -Dm644 brand/svg/icon.svg "$ICONS/scalable/apps/sqlsea.svg"
# The desktop session's PATH often lacks ~/.local/bin, and launchers hide
# entries whose program they cannot find, so point at the binary directly.
mkdir -p "$APPS"
sed -e "s|^Exec=sqlsea|Exec=$BIN/sqlsea|" -e "s|^TryExec=.*|TryExec=$BIN/sqlsea|" \
    packaging/sqlsea.desktop > "$APPS/sqlsea.desktop"
chmod 644 "$APPS/sqlsea.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
echo "Installed sqlsea to $BIN. It is in your app launcher now."
