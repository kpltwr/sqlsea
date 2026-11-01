#!/usr/bin/env bash
# Makes a macOS package of sqlsea: dist/sqlsea-<version>-macos.dmg.
# The app in it runs on Apple silicon and on Intel.
# Run this script on a Mac that has the Xcode command line tools.
#
# Without a signing identity, the app gets an ad-hoc signature. It runs on the
# Mac that built it. On other Macs, macOS asks before the first start.
# To sign and notarize the app, set these variables:
#   SQLSEA_SIGN_ID         a "Developer ID Application: ..." identity in your keychain
#   SQLSEA_NOTARY_PROFILE  a profile from `xcrun notarytool store-credentials`
#   SQLSEA_BUNDLE_ID       the bundle identifier (the default is dev.sqlsea.app)
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
bundle_id=${SQLSEA_BUNDLE_ID:-dev.sqlsea.app}
stage=dist/stage
app="$stage/sqlsea.app"
out="dist/sqlsea-$version-macos.dmg"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

rm -rf "$stage"
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
hdiutil create -volname sqlsea -srcfolder "$stage" -ov -format UDZO "$out"
rm -rf "$stage"

if [ -n "${SQLSEA_SIGN_ID:-}" ]; then
    codesign --force --sign "$SQLSEA_SIGN_ID" "$out"
fi
if [ -n "${SQLSEA_NOTARY_PROFILE:-}" ]; then
    xcrun notarytool submit "$out" --keychain-profile "$SQLSEA_NOTARY_PROFILE" --wait
    xcrun stapler staple "$out"
fi
echo "$out"
