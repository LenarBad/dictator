#!/usr/bin/env bash
# Zip a built Dictator.app for GitHub Releases. Does not install to /Applications.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/desktop"
ENTITLEMENTS="$DESKTOP/src-tauri/Entitlements.plist"
OUT="${1:-$ROOT/Dictator-macos-aarch64.app.zip}"

find_app() {
  local candidate
  for candidate in \
    "$DESKTOP/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Dictator.app" \
    "$DESKTOP/src-tauri/target/release/bundle/macos/Dictator.app" \
    "${CARGO_TARGET_DIR:-/tmp/does-not-exist}/aarch64-apple-darwin/release/bundle/macos/Dictator.app" \
    "${CARGO_TARGET_DIR:-/tmp/does-not-exist}/release/bundle/macos/Dictator.app"
  do
    if [[ -d "$candidate" ]]; then
      echo "$candidate"
      return 0
    fi
  done
  local newest="" newest_m=0
  while IFS= read -r candidate; do
    local mtime
    mtime=$(stat -f %m "$candidate")
    if (( mtime >= newest_m )); then
      newest_m=$mtime
      newest=$candidate
    fi
  done < <(find "$DESKTOP/src-tauri/target" "${CARGO_TARGET_DIR:-/tmp/does-not-exist}" "$ROOT/target" \
    -path '*/bundle/macos/Dictator.app' -type d 2>/dev/null || true)
  if [[ -n "$newest" && -d "$newest" ]]; then
    echo "$newest"
    return 0
  fi
  return 1
}

APP="$(find_app)" || {
  echo "package-macos-zip: Dictator.app not found; run tauri build first" >&2
  exit 1
}

if [[ -f "$ENTITLEMENTS" ]]; then
  codesign --force --sign - --options runtime --entitlements "$ENTITLEMENTS" --timestamp=none "$APP"
else
  codesign --force --sign - --options runtime --timestamp=none "$APP"
fi

rm -f "$OUT"
ditto -c -k --keepParent "$APP" "$OUT"
echo "package-macos-zip: $(du -sh "$OUT" | awk '{print $1}') $OUT"
echo "package-macos-zip: from $APP"
