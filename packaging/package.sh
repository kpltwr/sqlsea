#!/usr/bin/env bash
# Builds a release package of sqlsea for the platform it runs on, into dist/.
#
#   Linux  dist/sqlsea-<version>-linux-<arch>.tar.gz   binary, icons, launcher entry, install.sh
#   macOS  dist/sqlsea-<version>-macos.dmg             sqlsea.app for Apple silicon and Intel
#
# On Windows, run packaging/windows/package.ps1 instead.
#
# macOS signing is optional. By default the app gets an ad-hoc signature, which
# runs on the Mac that built it; elsewhere Gatekeeper asks before first launch.
# To sign and notarize for distribution, set:
#   SQLSEA_SIGN_ID         a "Developer ID Application: ..." identity in your keychain
#   SQLSEA_NOTARY_PROFILE  a profile saved with `xcrun notarytool store-credentials`
#   SQLSEA_BUNDLE_ID       the bundle identifier (default dev.sqlsea.app)
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
stage=dist/stage
rm -rf "$stage"
mkdir -p "$stage"

case "$(uname -s)" in
Linux)
    cargo build --release --locked
    name="sqlsea-$version-linux-$(uname -m)"
    dir="$stage/$name"
    mkdir -p "$dir/icons"
    cp target/release/sqlsea "$dir/"
    for size in 16 24 32 48 64 128 256 512; do
        cp "brand/png/icon-$size.png" "$dir/icons/"
    done
    cp brand/svg/icon.svg "$dir/icons/"
    cp packaging/sqlsea.desktop packaging/install.sh README.md LICENSE-MIT LICENSE-APACHE "$dir/"
    tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
    out="dist/$name.tar.gz"
    ;;
Darwin)
    bundle_id=${SQLSEA_BUNDLE_ID:-dev.sqlsea.app}
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    cargo build --release --locked --target aarch64-apple-darwin
    cargo build --release --locked --target x86_64-apple-darwin

    app="$stage/sqlsea.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    lipo -create -output "$app/Contents/MacOS/sqlsea" \
        target/aarch64-apple-darwin/release/sqlsea \
        target/x86_64-apple-darwin/release/sqlsea
    cp brand/icon.icns "$app/Contents/Resources/sqlsea.icns"
    sed -e "s/@VERSION@/$version/g" -e "s/@BUNDLE_ID@/$bundle_id/g" \
        packaging/macos/Info.plist > "$app/Contents/Info.plist"

    if [ -n "${SQLSEA_SIGN_ID:-}" ]; then
        codesign --force --options runtime --timestamp --sign "$SQLSEA_SIGN_ID" "$app"
    else
        codesign --force --sign - "$app"
    fi

    ln -s /Applications "$stage/Applications"
    out="dist/sqlsea-$version-macos.dmg"
    hdiutil create -volname sqlsea -srcfolder "$stage" -ov -format UDZO "$out"

    if [ -n "${SQLSEA_SIGN_ID:-}" ]; then
        codesign --force --sign "$SQLSEA_SIGN_ID" "$out"
    fi
    if [ -n "${SQLSEA_NOTARY_PROFILE:-}" ]; then
        xcrun notarytool submit "$out" --keychain-profile "$SQLSEA_NOTARY_PROFILE" --wait
        xcrun stapler staple "$out"
    fi
    ;;
*)
    echo "On Windows, run packaging/windows/package.ps1." >&2
    exit 1
    ;;
esac

rm -rf "$stage"
echo "$out"
