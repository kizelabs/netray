# NeTray

A lightweight macOS menu bar network monitor. Live upload and download
throughput in your status bar, with a per-app breakdown, session totals,
peak speeds, and per-interface stats in the dropdown.

```
↑ 4.0K/s
↓ 6.0K/s
```

The throughput value is colored by magnitude for an at-a-glance read —
green for K/s, amber for M/s, red for G/s (and a muted gray when idle).
Colors adapt to a light or dark menu bar.

Clicking the menu bar item shows a compact table of the apps currently
using the network (top 10 by throughput) — app name on the left,
download and upload right-aligned in their own columns, each colored by
magnitude. Sampled from macOS's built-in `nettop`, no elevated
privileges required.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/kizelabs/netray/main/install.sh | bash
```

Installs `NeTray.app` and registers a LaunchAgent so it starts at login.
Universal binary — Apple Silicon and Intel. Requires macOS 11+.

## Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/kizelabs/netray/main/uninstall.sh | bash
```

## What the installer touches

| Path | Purpose |
| --- | --- |
| `/Applications/NeTray.app` | the app (falls back to `~/Applications` if `/Applications` isn't writable) |
| `~/Library/LaunchAgents/com.kizelabs.netray.plist` | login agent |
| `/tmp/netray.log` | stdout/stderr |

Nothing runs as root, and `uninstall.sh` reverses all of it.

## A note on code signing

Releases are **ad-hoc signed, not Apple-notarized.** Installing via `curl`
works without any Gatekeeper prompt, because `curl` doesn't attach the
`com.apple.quarantine` attribute that triggers it. If you instead download
the tarball through a browser, macOS *will* block it and you'll need to
right-click → Open, or run `xattr -dr com.apple.quarantine NeTray.app`.

Proper notarization needs a paid Apple Developer account.

## Build from source

```sh
cargo build --release          # plain binary at target/release/netray
./scripts/bundle.sh 0.1.0      # universal NeTray.app + tarball in dist/
```

## Releasing

Push a tag; the `release` workflow builds the universal bundle and
publishes it, which is what `install.sh` pulls from.

```sh
git tag v0.1.0 && git push origin v0.1.0
```

## Implementation notes

The two-line menu bar title is an `NSAttributedString` set on the status
item's button, which takes some care to lay out:

- The two lines are joined with `\n`, making them separate *paragraphs* —
  so `paragraphSpacing`, not `lineSpacing`, controls the gap.
- The system font's natural line height at 9pt is far too tall for the menu
  bar, so min/max line height are pinned to collapse each line box.
- Collapsing the line box leaves the block sitting high; an explicit
  baseline offset re-centers it. **This is tuned to a 30pt notched menu
  bar** (`BASELINE_OFFSET` in `src/tray_title.rs`) and will sit low on a
  ~24pt bar.
- A monospaced font plus U+2007 figure-space padding keeps the title a
  constant width, so it never shoves neighbouring menu bar icons around.
  AppKit trims trailing *ASCII* whitespace when measuring a button title,
  which is why the padding uses figure spaces.
