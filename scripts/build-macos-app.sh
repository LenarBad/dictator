#!/usr/bin/env bash
# Release Dictator.app with the GigaAM ONNX model inside Contents/Resources.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/desktop"
ENTITLEMENTS="$DESKTOP/src-tauri/Entitlements.plist"
LOCAL_APP="$DESKTOP/src-tauri/target/release/bundle/macos/Dictator.app"
INSTALL="/Applications/Dictator.app"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

# Spotlight indexes leftover Dictator.app under bundle/macos and shows it next
# to /Applications. Opening that copy breaks Accessibility (different CDHash).
hide_build_artifacts_from_spotlight() {
  local dir="$1"
  [[ -n "$dir" ]] || return 0
  mkdir -p "$dir"
  touch "$dir/.metadata_never_index"
}
unregister_and_remove_app() {
  local app="$1"
  [[ -n "$app" && -d "$app" ]] || return 0
  if [[ "$app" == "$INSTALL" || "$app" == /Applications/* ]]; then
    echo "build-macos-app: refusing to delete $app" >&2
    return 1
  fi
  "$LSREGISTER" -u "$app" >/dev/null 2>&1 || true
  rm -rf "$app"
}

hide_build_artifacts_from_spotlight "$DESKTOP/src-tauri/target"
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  hide_build_artifacts_from_spotlight "$CARGO_TARGET_DIR"
fi

echo "build-macos-app: fetching STT model…"
bash "$ROOT/scripts/fetch-stt-model.sh"

cd "$DESKTOP"
if [[ ! -d node_modules ]]; then
  npm install
fi

BUILD_LOG="$(mktemp)"
trap 'rm -f "$BUILD_LOG"' EXIT
npm run tauri build -- --bundles app | tee "$BUILD_LOG"

APP=""
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  candidate="$CARGO_TARGET_DIR/release/bundle/macos/Dictator.app"
  if [[ -d "$candidate" ]]; then
    APP="$candidate"
  fi
fi
if [[ -z "$APP" ]]; then
  APP="$(awk '/bundle\/macos\/Dictator\.app/{path=$NF} END{print path}' "$BUILD_LOG")"
fi
if [[ -z "$APP" || ! -d "$APP" ]]; then
  newest=""
  newest_m=0
  while IFS= read -r candidate; do
    mtime=$(stat -f %m "$candidate")
    if (( mtime >= newest_m )); then
      newest_m=$mtime
      newest=$candidate
    fi
  done < <(find "$DESKTOP/src-tauri/target" "${CARGO_TARGET_DIR:-/tmp/does-not-exist}" "$ROOT/target" \
    -path '*/bundle/macos/Dictator.app' -type d 2>/dev/null || true)
  APP="$newest"
fi
if [[ -z "$APP" || ! -d "$APP" ]]; then
  echo "build-macos-app: Dictator.app not found after tauri build" >&2
  exit 1
fi

if [[ -f "$ENTITLEMENTS" ]]; then
  codesign --force --sign - --options runtime --entitlements "$ENTITLEMENTS" --timestamp=none "$APP"
else
  codesign --force --sign - --options runtime --timestamp=none "$APP"
fi

echo "build-macos-app: installing $INSTALL"
rm -rf "$INSTALL"
ditto "$APP" "$INSTALL"
if [[ -f "$ENTITLEMENTS" ]]; then
  codesign --force --sign - --options runtime --entitlements "$ENTITLEMENTS" --timestamp=none "$INSTALL"
else
  codesign --force --sign - --options runtime --timestamp=none "$INSTALL"
fi

unregister_and_remove_app "$APP"
if [[ "$LOCAL_APP" != "$APP" ]]; then
  unregister_and_remove_app "$LOCAL_APP"
fi

echo "build-macos-app: $(du -sh "$INSTALL" | awk '{print $1}')"
echo "build-macos-app: $INSTALL"
