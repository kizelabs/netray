#!/usr/bin/env bash
# Builds a universal NeTray.app and tars it up for a GitHub release.
#
#   ./scripts/bundle.sh 0.1.0
#
# Output: dist/NeTray.app and dist/NeTray-macos-universal.tar.gz
set -euo pipefail

VERSION="${1:?usage: bundle.sh <version>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/NeTray.app"

cd "$ROOT"

echo "==> Building universal binary (arm64 + x86_64)"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create -output "$APP/Contents/MacOS/netray" \
	"target/aarch64-apple-darwin/release/netray" \
	"target/x86_64-apple-darwin/release/netray"
chmod +x "$APP/Contents/MacOS/netray"

sed "s/__VERSION__/$VERSION/g" packaging/Info.plist > "$APP/Contents/Info.plist"
cp packaging/NeTray.icns "$APP/Contents/Resources/NeTray.icns"

# Ad-hoc signature. This is not Apple notarization -- it does not stop
# Gatekeeper warnings for browser downloads -- but it gives the bundle a
# stable identity, which macOS wants for a launchd-managed agent.
echo "==> Ad-hoc signing"
codesign --force --deep --sign - "$APP"
codesign --verify --strict "$APP"

echo "==> Packaging"
tar -czf "$DIST/NeTray-macos-universal.tar.gz" -C "$DIST" NeTray.app

echo
echo "Built $APP"
lipo -info "$APP/Contents/MacOS/netray"
ls -lh "$DIST/NeTray-macos-universal.tar.gz"
