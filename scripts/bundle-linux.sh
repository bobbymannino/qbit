#!/bin/sh
# Build qBit for x86_64 Linux and package it as a tarball.
#
# Usage: scripts/bundle-linux.sh [version]
#
# Writes dist/qBit-<version>-linux-x86_64.tar.gz, holding the `qbit` binary and a
# `qbit.desktop` launcher entry. The version defaults to the one in Cargo.toml.
#
# Needs the development packages for Wayland, X11 (xcb, xkbcommon) and fontconfig;
# see the Linux job in .github/workflows/release.yml for the Debian/Ubuntu list.
set -eu

cd "$(dirname "$0")/.."

version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
version="${version#v}"
bin_name="qbit"
target="x86_64-unknown-linux-gnu"

name="qBit-$version-linux-x86_64"
dist="dist"
staging="$dist/$name"

echo "Building qBit $version for $target"
cargo build --release --locked --target "$target"

target_dir="$(cargo metadata --format-version 1 --no-deps \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"

rm -rf "$staging" "$dist/$name.tar.gz"
mkdir -p "$staging"
cp "$target_dir/$target/release/$bin_name" "$staging/"
cp packaging/linux/qbit.desktop "$staging/"

tar -C "$dist" -czf "$dist/$name.tar.gz" "$name"
rm -rf "$staging"

echo "Built:"
ls -lh "$dist/$name.tar.gz"
