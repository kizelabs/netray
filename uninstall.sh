#!/usr/bin/env bash
#
#   curl -fsSL https://raw.githubusercontent.com/kizelabs/netray/main/uninstall.sh | bash
#
# Reverses install.sh: unloads the LaunchAgent, removes the plist and the app.
set -euo pipefail

LABEL="com.kizelabs.netray"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }

bold "Uninstalling NeTray"

if launchctl print "gui/$UID/$LABEL" >/dev/null 2>&1; then
	info "Unloading agent"
	launchctl bootout "gui/$UID/$LABEL" 2>/dev/null || true
fi

pkill -x netray 2>/dev/null || true

if [ -f "$PLIST" ]; then
	info "Removing $PLIST"
	rm -f "$PLIST"
fi

for APP in "/Applications/NeTray.app" "$HOME/Applications/NeTray.app"; do
	if [ -d "$APP" ]; then
		info "Removing $APP"
		rm -rf "$APP"
	fi
done

rm -f /tmp/netray.log

bold "Done -- NeTray removed."
