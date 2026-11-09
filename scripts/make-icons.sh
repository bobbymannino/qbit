#!/bin/sh
# Render every platform's app icon from packaging/icons/qbit.svg.
#
# Usage: scripts/make-icons.sh
#
# Writes packaging/macos/qBit.icns, packaging/windows/qbit.ico and
# packaging/linux/qbit.png. The results are committed, so this only needs running after
# the SVG changes. Needs macOS (for iconutil), rsvg-convert (librsvg) and ImageMagick.
set -eu

cd "$(dirname "$0")/.."

svg="packaging/icons/qbit.svg"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# macOS draws its own margin into the icon; Windows and Linux icons fill the canvas, so
# crop the SVG to the tile for them.
sed 's/viewBox="0 0 1024 1024"/viewBox="100 100 824 824"/' "$svg" > "$tmp/tile.svg"

render() { rsvg-convert -w "$2" -h "$2" "$1" -o "$3"; }

# macOS: an .icns from the standard iconset sizes.
iconset="$tmp/qBit.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    render "$svg" "$size" "$iconset/icon_${size}x${size}.png"
    render "$svg" "$((size * 2))" "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o packaging/macos/qBit.icns

# Windows: a multi-size .ico.
for size in 16 24 32 48 64 128 256; do
    render "$tmp/tile.svg" "$size" "$tmp/win-$size.png"
done
magick "$tmp"/win-16.png "$tmp"/win-24.png "$tmp"/win-32.png "$tmp"/win-48.png \
    "$tmp"/win-64.png "$tmp"/win-128.png "$tmp"/win-256.png packaging/windows/qbit.ico

# Linux: a 512px PNG for the hicolor icon theme.
render "$tmp/tile.svg" 512 packaging/linux/qbit.png

echo "Wrote packaging/macos/qBit.icns, packaging/windows/qbit.ico, packaging/linux/qbit.png"
