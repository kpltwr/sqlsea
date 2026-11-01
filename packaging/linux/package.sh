#!/usr/bin/env bash
# Makes a Linux package of sqlsea: dist/sqlsea-<version>-linux-<arch>.tar.gz.
# The package holds the app, the icons, the launcher entry and install.sh.
# You can run this script from any folder.
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
name="sqlsea-$version-linux-$(uname -m)"
stage=dist/stage
dir="$stage/$name"

cargo build --release --locked

rm -rf "$stage"
mkdir -p "$dir/icons"
cp target/release/sqlsea "$dir/"
for size in 16 24 32 48 64 128 256 512; do
    cp "brand/png/icon-$size.png" "$dir/icons/"
done
cp brand/svg/icon.svg "$dir/icons/"
cp packaging/linux/sqlsea.desktop packaging/linux/install.sh README.md LICENSE-MIT LICENSE-APACHE "$dir/"
tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
rm -rf "$stage"
echo "dist/$name.tar.gz"
