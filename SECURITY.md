# Security

## Reporting a vulnerability

Email **lenar.rf@gmail.com** (private disclosure preferred). Please include OS version, Dictator version or commit, and steps to reproduce. Do not open a public issue for exploitable bugs until a fix or mitigation is available.

## Threat model (macOS)

Dictator is a local speech-to-text tray app. After install it does **not** contact the network. Recognition runs on-device (GigaAM via sherpa-onnx).

### Powerful permissions (by design)

| Capability | Why |
|---|---|
| Microphone | Capture audio for dictation |
| Accessibility | Focus restore and paste into the frontmost app (AX insert or synthesized Cmd+V) |
| Global hotkey | Start/stop recording from any app |
| Clipboard write | Deliver recognized text |

With Accessibility granted, Dictator can insert text into whatever is focused — messengers, browsers, password fields. That is the product; treat the binary like any other input-injection tool.

**Mitigations for users**

- Prefer the copy from `/Applications/Dictator.app` (not Downloads)
- Disable auto-paste in settings when working in sensitive fields (text stays on the clipboard only)
- Revoke Microphone / Accessibility when you uninstall

### Hardened Runtime entitlements

Release builds use Hardened Runtime with entitlements needed for the WebView/ONNX stack and audio (`allow-jit`, `allow-unsigned-executable-memory`, `disable-library-validation`, `device.audio-input`, `automation.apple-events`). App Sandbox is **not** enabled: it blocks the tray shortcut and Accessibility paste path.

### Ad-hoc code signature

GitHub Releases are signed with an ad-hoc identity (`signingIdentity: "-"`), not Apple Developer ID / notarization. Gatekeeper will warn on first open — see [docs/INSTALL.md](docs/INSTALL.md). Trust is: open source + your own build, or the zip attached to a GitHub Release you choose to trust.

### Data that can remain on the machine

- **Clipboard** — full recognized text until something else overwrites it
- **Notification Center** — may show a short preview of the phrase
- **Temp WAV** — written under the system temp directory and deleted after recognition; a crash mid-STT can leave an orphan file
- **`~/Library/Application Support/dictator/`** — settings and `last-paste.log` (paste diagnostics: trust flag, exe path, char counts — not the transcript text)

## Supply chain

- STT weights are fetched by `scripts/fetch-stt-model.sh` with a pinned SHA-256 of the upstream archive
- App code has no updater and no analytics SDK
