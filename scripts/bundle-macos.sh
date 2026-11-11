#!/bin/sh
# Build qBit as a macOS app and disk image.
#
# Usage: scripts/bundle-macos.sh [version]
#
# Builds for the machine it runs on (Apple Silicon or Intel); set QBIT_TARGET to
# aarch64-apple-darwin or x86_64-apple-darwin to override (either can be built from
# either kind of Mac). Writes dist/qBit.app, dist/qBit-<version>-macos-<arch>.dmg, and
# dist/qBit-<version>-macos-<arch>.app.tar.gz (the bare app, for in-app updates).
# The version defaults to the one in Cargo.toml.
#
# The app is ad-hoc signed, so macOS will warn that it's from an unidentified developer.
# Set QBIT_SIGN_IDENTITY to sign with a real identity instead.
set -eu

cd "$(dirname "$0")/.."

version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
version="${version#v}"
app_name="qBit"
bin_name="qbit"

if [ -z "${QBIT_TARGET:-}" ]; then
    case "$(uname -m)" in
        arm64 | aarch64) QBIT_TARGET="aarch64-apple-darwin" ;;
        x86_64) QBIT_TARGET="x86_64-apple-darwin" ;;
        *) echo "Unsupported architecture: $(uname -m); set QBIT_TARGET" >&2; exit 1 ;;
    esac
fi
target="$QBIT_TARGET"

case "$target" in
    aarch64-*) arch="arm64" ;;
    x86_64-*) arch="x86_64" ;;
    *) echo "Unsupported target: $target" >&2; exit 1 ;;
esac

dist="dist"
app="$dist/$app_name.app"
artifact="$dist/$app_name-$version-macos-$arch"

echo "Building $app_name $version for $target"
cargo build --release --locked --target "$target"

target_dir="$(cargo metadata --format-version 1 --no-deps \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

# Only clear this script's own output, so other platforms' builds in dist/ survive.
rm -rf "$app" "$artifact.dmg" "$artifact.app.tar.gz"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$target_dir/$target/release/$bin_name" "$app/Contents/MacOS/$bin_name"
cp packaging/macos/qBit.icns "$app/Contents/Resources/"

sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" > /dev/null

if [ -n "${QBIT_SIGN_IDENTITY:-}" ]; then
    codesign --force --deep --sign "$QBIT_SIGN_IDENTITY" "$app"
else
    # An ad-hoc signature's designated requirement defaults to the build's cdhash, which
    # changes every release. The keychain trusts the app that saved the login by that
    # requirement, so each update would ask for the user's password to read it again.
    # Pinning it to the bundle ID keeps every release trusted.
    bundle_id="$(plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist")"
    codesign --force --deep --sign - --identifier "$bundle_id" \
        -r="designated => identifier \"$bundle_id\"" "$app"
fi
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

# The bare app as a tarball, which keeps the binary executable and the signature intact.
# COPYFILE_DISABLE stops macOS tar adding AppleDouble (._*) files, which would break it.
COPYFILE_DISABLE=1 tar -czf "$artifact.app.tar.gz" -C "$dist" "$app_name.app"

echo "Built:"
lipo -archs "$app/Contents/MacOS/$bin_name"
ls -lh "$artifact.dmg" "$artifact.app.tar.gz"
