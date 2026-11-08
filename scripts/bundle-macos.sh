#!/bin/sh
# Build qBit as an Apple Silicon macOS app and disk image.
#
# Usage: scripts/bundle-macos.sh [version]
#
# Writes dist/qBit.app and dist/qBit-<version>-macos-arm64.dmg. The version defaults to the one in Cargo.toml.
#
# The app is ad-hoc signed, so macOS will warn that it's from an unidentified developer.
# Set QBIT_SIGN_IDENTITY to sign with a real identity instead.
set -eu

cd "$(dirname "$0")/.."

version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
version="${version#v}"
app_name="qBit"
bin_name="qbit"
target="aarch64-apple-darwin"

dist="dist"
app="$dist/$app_name.app"
artifact="$dist/$app_name-$version-macos-arm64"

echo "Building $app_name $version for $target"
cargo build --release --locked --target "$target"

target_dir="$(cargo metadata --format-version 1 --no-deps \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

# Only clear this script's own output, so other platforms' builds in dist/ survive.
rm -rf "$app" "$artifact.dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$target_dir/$target/release/$bin_name" "$app/Contents/MacOS/$bin_name"

sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" > /dev/null

codesign --force --deep --sign "${QBIT_SIGN_IDENTITY:--}" "$app"
codesign --verify --deep --strict "$app"

# A disk image with an Applications shortcut to drag the app onto.
staging="$dist/dmg"
mkdir -p "$staging"
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
rm -f "$artifact.dmg"
# `hdiutil create` is deprecated in favour of `diskutil image`, which older macOS
# releases (and so some CI runners) don't have yet.
if diskutil image create from --help > /dev/null 2>&1; then
    diskutil image create from --format UDZO --volumeName "$app_name" \
        "$staging" "$artifact.dmg" > /dev/null
else
    hdiutil create -volname "$app_name" -srcfolder "$staging" -fs HFS+ -format UDZO \
        "$artifact.dmg" > /dev/null
fi
rm -rf "$staging"

echo "Built:"
lipo -archs "$app/Contents/MacOS/$bin_name"
ls -lh "$artifact.dmg"
