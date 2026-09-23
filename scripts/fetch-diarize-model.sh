#!/usr/bin/env bash
# Download pyannote segmentation 3.0 (int8) and NeMo TitaNet Small for sherpa-onnx diarization.
# RevAI Reverb is intentionally not fetched (non-production license).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/desktop/src-tauri/resources/diarize"

SEG_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
SEG_NAME="sherpa-onnx-pyannote-segmentation-3-0.tar.bz2"
SEG_SHA256="24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488"

EMB_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx"
EMB_NAME="nemo_en_titanet_small.onnx"
EMB_SHA256="ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e"

file_sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    echo "fetch-diarize-model: need shasum or sha256sum" >&2
    exit 1
  fi
}

if [[ -f "$DEST/segmentation.int8.onnx" && -f "$DEST/embedding.onnx" ]]; then
  echo "fetch-diarize-model: already present $(du -sh "$DEST" | awk '{print $1}')"
  exit 0
fi

mkdir -p "$DEST/licenses"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetch-diarize-model: downloading $SEG_NAME"
curl -L --fail --retry 3 -o "$TMP/$SEG_NAME" "$SEG_URL"
actual="$(file_sha256 "$TMP/$SEG_NAME")"
if [[ "$actual" != "$SEG_SHA256" ]]; then
  echo "fetch-diarize-model: SHA-256 mismatch for $SEG_NAME" >&2
  echo "  expected: $SEG_SHA256" >&2
  echo "  actual:   $actual" >&2
  exit 1
fi

echo "fetch-diarize-model: downloading $EMB_NAME"
curl -L --fail --retry 3 -o "$TMP/$EMB_NAME" "$EMB_URL"
actual="$(file_sha256 "$TMP/$EMB_NAME")"
if [[ "$actual" != "$EMB_SHA256" ]]; then
  echo "fetch-diarize-model: SHA-256 mismatch for $EMB_NAME" >&2
  echo "  expected: $EMB_SHA256" >&2
  echo "  actual:   $actual" >&2
  exit 1
fi
echo "fetch-diarize-model: SHA-256 ok"

tar -xjf "$TMP/$SEG_NAME" -C "$TMP"
SEG_FILE="$(find "$TMP" -name model.int8.onnx -print -quit)"
if [[ -z "$SEG_FILE" ]]; then
  echo "fetch-diarize-model: model.int8.onnx not found in archive" >&2
  exit 1
fi
cp "$SEG_FILE" "$DEST/segmentation.int8.onnx"
cp "$TMP/$EMB_NAME" "$DEST/embedding.onnx"

SEG_LICENSE="$(find "$TMP" -path '*/sherpa-onnx-pyannote-segmentation-3-0/LICENSE' -print -quit)"
if [[ -n "$SEG_LICENSE" ]]; then
  cp "$SEG_LICENSE" "$DEST/licenses/pyannote-segmentation-3.0.LICENSE"
fi

echo "fetch-diarize-model: done $(du -sh "$DEST" | awk '{print $1}')"
