#!/usr/bin/env bash
# Download GigaAM-v3 e2e RNNT ONNX files for the in-process sherpa-onnx engine.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/desktop/src-tauri/resources/gigaam"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-transducer-punct-giga-am-v3-russian-2025-12-16.tar.bz2"
ARCHIVE_NAME="sherpa-onnx-nemo-transducer-punct-giga-am-v3-russian-2025-12-16.tar.bz2"

need_fetch=0
for name in encoder.int8.onnx decoder.onnx joiner.onnx tokens.txt; do
  if [[ ! -f "$DEST/$name" ]]; then
    need_fetch=1
  fi
done
if [[ "$need_fetch" -eq 0 ]]; then
  echo "fetch-stt-model: already present $(du -sh "$DEST" | awk '{print $1}')"
  exit 0
fi

mkdir -p "$DEST"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetch-stt-model: downloading $ARCHIVE_NAME"
curl -L --fail --retry 3 -o "$TMP/$ARCHIVE_NAME" "$URL"
tar -xjf "$TMP/$ARCHIVE_NAME" -C "$TMP"
INNER="$(find "$TMP" -name encoder.int8.onnx -print -quit | xargs dirname)"
cp "$INNER/encoder.int8.onnx" "$INNER/decoder.onnx" "$INNER/joiner.onnx" "$INNER/tokens.txt" "$DEST/"
if [[ -f "$INNER/LICENSE" ]]; then
  cp "$INNER/LICENSE" "$DEST/"
fi

echo "fetch-stt-model: done $(du -sh "$DEST" | awk '{print $1}')"
