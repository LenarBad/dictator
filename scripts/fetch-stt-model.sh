#!/usr/bin/env bash
# Download GigaAM-v3 e2e RNNT ONNX files for the in-process sherpa-onnx engine.
# Also fetches diarization weights. That call stays above the early exit: if GigaAM
# is already on disk this script used to return before a child fetch could run.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
bash "$ROOT/scripts/fetch-diarize-model.sh"

DEST="$ROOT/desktop/src-tauri/resources/gigaam"
ANDROID_DEST="$ROOT/android/app/src/main/assets/gigaam"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-transducer-punct-giga-am-v3-russian-2025-12-16.tar.bz2"
ARCHIVE_NAME="sherpa-onnx-nemo-transducer-punct-giga-am-v3-russian-2025-12-16.tar.bz2"
# sha256 of the upstream tar.bz2 (recompute if you bump ARCHIVE_NAME / URL).
EXPECTED_SHA256="f9620a0099019c6afcee26525ef9ed3297fa50dd5691c1902af0c948fc1a470b"

file_sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    echo "fetch-stt-model: need shasum or sha256sum" >&2
    exit 1
  fi
}

sync_android_assets() {
  if [[ ! -d "$ROOT/android" ]]; then
    return 0
  fi
  mkdir -p "$ANDROID_DEST"
  for name in encoder.int8.onnx decoder.onnx joiner.onnx tokens.txt LICENSE; do
    if [[ -f "$DEST/$name" ]]; then
      cp "$DEST/$name" "$ANDROID_DEST/$name"
    fi
  done
  echo "fetch-stt-model: synced android assets $(du -sh "$ANDROID_DEST" 2>/dev/null | awk '{print $1}')"
}

need_fetch=0
for name in encoder.int8.onnx decoder.onnx joiner.onnx tokens.txt; do
  if [[ ! -f "$DEST/$name" ]]; then
    need_fetch=1
  fi
done
if [[ "$need_fetch" -eq 0 ]]; then
  echo "fetch-stt-model: already present $(du -sh "$DEST" | awk '{print $1}')"
  sync_android_assets
  exit 0
fi

mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetch-stt-model: downloading $ARCHIVE_NAME"
curl -L --fail --retry 3 -o "$TMP/$ARCHIVE_NAME" "$URL"

actual="$(file_sha256 "$TMP/$ARCHIVE_NAME")"
if [[ "$actual" != "$EXPECTED_SHA256" ]]; then
  echo "fetch-stt-model: SHA-256 mismatch" >&2
  echo "  expected: $EXPECTED_SHA256" >&2
  echo "  actual:   $actual" >&2
  exit 1
fi
echo "fetch-stt-model: SHA-256 ok"

tar -xjf "$TMP/$ARCHIVE_NAME" -C "$TMP"
INNER="$(find "$TMP" -name encoder.int8.onnx -print -quit)"
if [[ -z "$INNER" ]]; then
  echo "fetch-stt-model: encoder.int8.onnx not found in archive" >&2
  exit 1
fi
INNER="$(dirname "$INNER")"
cp "$INNER/encoder.int8.onnx" "$INNER/decoder.onnx" "$INNER/joiner.onnx" "$INNER/tokens.txt" "$DEST/"
if [[ -f "$INNER/LICENSE" ]]; then
  cp "$INNER/LICENSE" "$DEST/"
fi

echo "fetch-stt-model: done $(du -sh "$DEST" | awk '{print $1}')"
sync_android_assets
