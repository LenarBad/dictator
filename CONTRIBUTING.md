# Contributing

Dictator is offline Russian dictation for the desktop tray (Tauri 2 + local GigaAM): macOS Apple Silicon and Windows x64.

## Before you open a PR

1. Read [docs/BUILD.md](docs/BUILD.md) (or the short command list in [desktop/README.md](desktop/README.md)).
2. Fetch models: `bash scripts/fetch-stt-model.sh` (GigaAM and diarization weights; SHA-256 is verified).
3. From `desktop/`: `npm ci` and run what you changed (`npm run tauri dev` or tests).

## Scope

- Supported targets: macOS 12+ Apple Silicon and Windows 10 1809+ x64. Code on a Mac; Windows artifacts come from GitHub Actions ([docs/WINDOWS.md](docs/WINDOWS.md))
- Keep recognition fully local — no cloud STT, analytics, or phone-home
- Prefer small, focused PRs with a short “why”

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reports and the threat model. Do not discuss exploitable issues in public issues until fixed.

## License

By contributing you agree your changes are licensed under the MIT License (same as this repository). Bundled third-party notices: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
