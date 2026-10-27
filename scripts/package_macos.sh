#!/usr/bin/env bash
# Build the release binary and assemble Narra.app (M7, D-014: macOS only).
#
# The bundle is unsigned and unnotarized: it is meant for local builds and
# the reference machine. A copy downloaded from elsewhere carries the
# quarantine attribute; open it once with right-click → Open, or clear it
# with `xattr -d com.apple.quarantine Narra.app` (see docs/INSTALL.md).
#
# Usage: scripts/package_macos.sh [--no-smoke]
#   --no-smoke  skip the launch/dump smoke test (for CI-less hosts)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SMOKE=1
if [[ "${1:-}" == "--no-smoke" ]]; then
    SMOKE=0
fi

APP="$ROOT/target/release/Narra.app"
BUNDLE_ID="io.github.olivaresrafael.narra"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
VERSION="${VERSION:-0.0.0}"

echo "==> building the release binary"
cargo build --release -p narra-app

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/narra-app" "$APP/Contents/MacOS/narra-app"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Narra</string>
    <key>CFBundleDisplayName</key>
    <string>Narra</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>
    <string>narra-app</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

plutil -lint "$APP/Contents/Info.plist" >/dev/null
echo "==> bundle: $APP (version $VERSION, id $BUNDLE_ID)"

if [[ "$SMOKE" -eq 1 ]]; then
    echo "==> smoke test: launch the bundled binary and read its dump"
    DUMP_FILE="$(mktemp "${TMPDIR:-/tmp}/narra-smoke.XXXXXX")"
    trap 'rm -f "$DUMP_FILE"' EXIT
    # No credentials in the environment: export and editing paths must not
    # need any (M7 acceptance).
    env -u OPENAI_API_KEY -u GITHUB_TOKEN \
        "$APP/Contents/MacOS/narra-app" \
        --fixture "$ROOT/fixtures/screenplay-1000.fountain" \
        --dump-after-ms 2500 > "$DUMP_FILE"
    if ! grep -q '"kind":"spike-a-buffer-dump"' "$DUMP_FILE"; then
        echo "smoke test failed: no dump produced" >&2
        exit 1
    fi
    LINES="$(grep -o '"lines":[0-9]*' "$DUMP_FILE" | head -1 | cut -d: -f2)"
    echo "==> smoke ok: dump parsed (lines=$LINES)"
fi

echo "done: $APP"
