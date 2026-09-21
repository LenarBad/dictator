# Third-party notices

Dictator itself is MIT-licensed ([LICENSE](LICENSE)). The desktop app redistributes or links the following components. This file is a summary; full license texts live in the linked projects or in-tree copies.

## GigaAM speech model (ONNX weights)

- Project: [salute-developers/GigaAM](https://github.com/salute-developers/GigaAM)
- Packaged via: [sherpa-onnx asr-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)
- License: MIT (Copyright (c) 2024 GigaChat Team)
- In-tree copy bundled with the app: `desktop/src-tauri/resources/gigaam/LICENSE`

## sherpa-onnx

- Crate: [`sherpa-onnx`](https://crates.io/crates/sherpa-onnx) (Rust wrapper) and the underlying [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) toolkit / ONNX Runtime
- License: Apache-2.0 (wrapper and core project)

## Tauri and tray-icon

- [Tauri](https://tauri.app) and related crates — typically Apache-2.0 OR MIT
- Vendored patch of [`tray-icon`](https://github.com/tauri-apps/tray-icon): `desktop/src-tauri/vendor/tray-icon/`
  - Licenses: MIT and Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`)
  - Patch notes: `PATCH.md`

## Other Rust / npm dependencies

The full dependency graph is declared in:

- `desktop/src-tauri/Cargo.lock`
- `desktop/package-lock.json`

Each package carries its own license (commonly MIT, Apache-2.0, or BSD). Run `cargo license` / npm license tooling if you need a machine-generated inventory for a compliance audit.
