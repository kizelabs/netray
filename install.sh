#!/usr/bin/env bash
#
#   curl -fsSL https://raw.githubusercontent.com/kizelabs/netray/main/install.sh | bash
#
# Installs NeTray.app and registers a LaunchAgent so it starts at login.
# Everything it touches is listed at the end of a successful run, and
# uninstall.sh reverses all of it.
set -euo pipefail

REPO="kizelabs/netray"
LABEL="com.kizelabs.netray"
TARBALL="NeTray-macos-universal.tar.gz"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

red()  { printf '\033[31m%s\033[0m\n' "$*" >&2; }
bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }

[ "$(uname -s)" = "Darwin" ] || { red "NeTray is macOS-only (found $(uname -s))."; exit 1; }

# Prefer /Applications, fall back to ~/Applications when it isn't writable
# rather than escalating to sudo -- a menu bar agent has no need for root.
if [ -w /Applications ]; then
	APPDIR="/Applications"
else
	APPDIR="$HOME/Applications"
	mkdir -p "$APPDIR"
fi
APP="$APPDIR/NeTray.app"

bold "Installing NeTray"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

URL="https://github.com/$REPO/releases/latest/download/$TARBALL"
info "Downloading $URL"
if ! curl -fsSL --retry 3 -o "$TMP/$TARBALL" "$URL"; then
	red "Download failed. Does a published release exist at https://github.com/$REPO/releases ?"
	exit 1
fi

info "Extracting"
tar -xzf "$TMP/$TARBALL" -C "$TMP"
[ -d "$TMP/NeTray.app" ] || { red "Archive did not contain NeTray.app"; exit 1; }

# Stop any previously-installed copy before overwriting it on disk.
if launchctl print "gui/$UID/$LABEL" >/dev/null 2>&1; then
	info "Stopping running instance"
	launchctl bootout "gui/$UID/$LABEL" 2>/dev/null || true
fi
pkill -x netray 2>/dev/null || true

info "Installing to $APP"
rm -rf "$APP"
cp -R "$TMP/NeTray.app" "$APP"
# curl does not set the quarantine attribute, but a re-run over a
# browser-downloaded copy might have inherited one. Clear it either way.
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

info "Writing LaunchAgent $PLIST"
mkdir -p "$(dirname "$PLIST")"
cat > "$PLIST" <<PLISTEOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>$LABEL</string>
	<key>ProgramArguments</key>
	<array>
		<string>$APP/Contents/MacOS/netray</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<!-- Deliberately not KeepAlive: choosing Quit from the menu should
	     actually quit, not have launchd immediately restart it. -->
	<key>KeepAlive</key>
	<false/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>/tmp/netray.log</string>
	<key>StandardErrorPath</key>
	<string>/tmp/netray.log</string>
</dict>
</plist>
PLISTEOF

info "Loading agent"
launchctl bootstrap "gui/$UID" "$PLIST"

sleep 1
if pgrep -x netray >/dev/null; then
	bold "Done -- NeTray is running and will start at login."
else
	red "Agent loaded but netray is not running. Check /tmp/netray.log"
	exit 1
fi

echo
echo "Installed:"
echo "  $APP"
echo "  $PLIST"
echo
echo "Uninstall:"
echo "  curl -fsSL https://raw.githubusercontent.com/$REPO/main/uninstall.sh | bash"
