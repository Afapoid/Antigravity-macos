#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if [ "$(uname -s)" != Darwin ]; then
    echo "Собирайте на macOS с Xcode Command Line Tools и Rust." >&2
    exit 1
fi
VERSION=${AG_FULL_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1).1}
case "$VERSION" in "" | *[!0-9._]*) echo "Некорректная версия" >&2; exit 1 ;; esac
export AG_FULL_VERSION="$VERSION"
export MACOSX_DEPLOYMENT_TARGET=11.0
# The upstream fat-LTO/one-unit profile targets minimum Windows download size.
# Keep native builds within the memory budget of the 7 GB Apple Silicon runner.
export CARGO_PROFILE_RELEASE_OPT_LEVEL=2
export CARGO_PROFILE_RELEASE_LTO=thin
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
rustup target add aarch64-apple-darwin
cargo build --locked --release --no-default-features --target aarch64-apple-darwin
mkdir -p release/staging
cp target/aarch64-apple-darwin/release/ag_unlocker release/staging/ag_unlocker
codesign --force --sign - --timestamp=none release/staging/ag_unlocker
codesign --verify --strict release/staging/ag_unlocker
file release/staging/ag_unlocker
ASSET="AG_${VERSION}_macos_arm64.tar.gz"
tar czf "release/$ASSET" -C release/staging ag_unlocker
(cd release && shasum -a 256 "$ASSET" > "$ASSET.sha256")
printf 'Готово: release/%s\n' "$ASSET"
