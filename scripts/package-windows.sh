#!/usr/bin/env bash
# Copy the Tauri NSIS installer and a portable zip (exe + resources) to the repo root.
# Intended for GitHub Actions windows-2022 (Git Bash).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/desktop"
OUT_NSIS="${1:-$ROOT/Dictator-windows-x64.exe}"
OUT_ZIP="${2:-$ROOT/Dictator-windows-x64.zip}"

find_nsis() {
  local dir f
  for dir in \
    "$DESKTOP/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis" \
    "$DESKTOP/src-tauri/target/release/bundle/nsis"
  do
    [[ -d "$dir" ]] || continue
    for f in "$dir"/*setup.exe "$dir"/*.exe; do
      if [[ -f "$f" ]]; then
        echo "$f"
        return 0
      fi
    done
  done
  return 1
}

find_release_dir() {
  local dir
  for dir in \
    "$DESKTOP/src-tauri/target/x86_64-pc-windows-msvc/release" \
    "$DESKTOP/src-tauri/target/release"
  do
    if [[ -f "$dir/Dictator.exe" ]]; then
      echo "$dir"
      return 0
    fi
    local f
    for f in "$dir"/*.exe; do
      if [[ -f "$f" ]]; then
        echo "$dir"
        return 0
      fi
    done
  done
  return 1
}

NSIS="$(find_nsis)" || {
  echo "package-windows: NSIS installer not found; run tauri build --bundles nsis first" >&2
  exit 1
}

cp -f "$NSIS" "$OUT_NSIS"
echo "package-windows: nsis $(du -sh "$OUT_NSIS" | awk '{print $1}') $OUT_NSIS"
echo "package-windows: from $NSIS"

RELEASE_DIR="$(find_release_dir)" || {
  echo "package-windows: Dictator.exe not found next to the bundle" >&2
  exit 1
}

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

if [[ -f "$RELEASE_DIR/Dictator.exe" ]]; then
  cp -f "$RELEASE_DIR/Dictator.exe" "$STAGE/"
else
  shopt -s nullglob
  exe=("$RELEASE_DIR"/*.exe)
  cp -f "${exe[0]}" "$STAGE/"
fi

shopt -s nullglob
for dll in "$RELEASE_DIR"/*.dll; do
  cp -f "$dll" "$STAGE/"
done
if [[ -d "$RELEASE_DIR/resources" ]]; then
  cp -R "$RELEASE_DIR/resources" "$STAGE/resources"
fi

if [[ ! -f "$STAGE/resources/gigaam/encoder.int8.onnx" && ! -f "$STAGE/gigaam/encoder.int8.onnx" ]]; then
  echo "package-windows: GigaAM model missing from portable stage" >&2
  echo "package-windows: release dir listing:" >&2
  ls -la "$RELEASE_DIR" >&2 || true
  ls -la "$RELEASE_DIR/resources" >&2 || true
  exit 1
fi

if [[ ! -f "$STAGE/resources/diarize/segmentation.int8.onnx" && ! -f "$STAGE/diarize/segmentation.int8.onnx" ]]; then
  echo "package-windows: diarization segmentation model missing from portable stage" >&2
  ls -la "$RELEASE_DIR/resources" >&2 || true
  exit 1
fi
if [[ ! -f "$STAGE/resources/diarize/embedding.onnx" && ! -f "$STAGE/diarize/embedding.onnx" ]]; then
  echo "package-windows: diarization embedding model missing from portable stage" >&2
  ls -la "$RELEASE_DIR/resources" >&2 || true
  exit 1
fi

rm -f "$OUT_ZIP"
if command -v python >/dev/null 2>&1; then
  PY=python
elif command -v python3 >/dev/null 2>&1; then
  PY=python3
else
  echo "package-windows: need python to create the zip" >&2
  exit 1
fi

# Forward slashes (cygpath -m) so Git Bash does not eat \a in D:\a\...
# zipfile writes to OUT_ZIP in place — shutil.make_archive chdirs into STAGE
# and on this runner the zip landed in the temp dir, which the trap then deletes.
OUT_FOR_PY="$OUT_ZIP"
STAGE_FOR_PY="$STAGE"
if command -v cygpath >/dev/null 2>&1; then
  OUT_FOR_PY="$(cygpath -m "$OUT_ZIP")"
  STAGE_FOR_PY="$(cygpath -m "$STAGE")"
fi
DICTATOR_ZIP_OUT="$OUT_FOR_PY" DICTATOR_ZIP_SRC="$STAGE_FOR_PY" "$PY" -c "
import os, zipfile
out, root = os.environ['DICTATOR_ZIP_OUT'], os.environ['DICTATOR_ZIP_SRC']
with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED, allowZip64=True) as zf:
    for dirpath, _, files in os.walk(root):
        for name in files:
            full = os.path.join(dirpath, name)
            zf.write(full, os.path.relpath(full, root))
"

if [[ ! -f "$OUT_ZIP" ]]; then
  echo "package-windows: zip was not created at $OUT_ZIP" >&2
  ls -la "$ROOT" >&2 || true
  exit 1
fi

echo "package-windows: zip $(du -sh "$OUT_ZIP" | awk '{print $1}') $OUT_ZIP"
echo "package-windows: from $RELEASE_DIR"
