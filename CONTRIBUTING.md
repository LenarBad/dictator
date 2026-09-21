# Contributing

Dictator is offline Russian dictation for the Mac menu bar (Tauri 2 + local GigaAM).

## Before you open a PR

1. Read [docs/INSTALL.md](docs/INSTALL.md) (build-from-source section).
2. Fetch the STT model: `bash scripts/fetch-stt-model.sh` (SHA-256 is verified).
3. From `desktop/`: `npm ci` and run what you changed (`npm run tauri dev` or tests).

## Scope

- macOS Apple Silicon is the supported target today
- Keep recognition fully local — no cloud STT, analytics, or phone-home
- Prefer small, focused PRs with a short “why”

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reports and the threat model. Do not discuss exploitable issues in public issues until fixed.

## License

By contributing you agree your changes are licensed under the MIT License (same as this repository). Bundled third-party notices: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
