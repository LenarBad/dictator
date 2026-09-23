# Security

## Reporting a vulnerability

Email **lenar.rf@gmail.com** (private disclosure preferred). Please include OS version, Dictator version or commit, and steps to reproduce. Do not open a public issue for exploitable bugs until a fix or mitigation is available.

## Threat model (desktop)

Dictator is a local speech-to-text tray app for **macOS** (Apple Silicon) and **Windows** (x64). After install it does **not** contact the network. Recognition runs on-device (GigaAM via sherpa-onnx).

### Powerful permissions (by design)

| Capability | Why |
|---|---|
| Microphone | Capture audio for dictation |
| Accessibility (macOS) | Focus restore and paste into the frontmost app (synthesized Cmd+V) |
| SendInput Ctrl+V (Windows) | Paste into the previously focused window; no Accessibility analog |
| Global hotkey | Start/stop recording from any app |
| Clipboard write | Deliver recognized text |

Dictator can insert text into whatever is focused — messengers, browsers, password fields. That is the product; treat the binary like any other input-injection tool.

On Windows, elevated (Run as administrator) windows will not accept paste from a normal Dictator process; text stays on the clipboard.

**Mitigations for users**

- Mac: prefer the copy from `/Applications/Dictator.app` (not Downloads)
- Windows: install the NSIS exe from a GitHub Release you choose to trust
- Disable auto-paste in settings when working in sensitive fields (text stays on the clipboard only)
- Revoke Microphone (and on Mac, Accessibility) when you uninstall

### Hardened Runtime (macOS)

Release builds use Hardened Runtime with entitlements needed for the WebView/ONNX stack and audio (`allow-jit`, `allow-unsigned-executable-memory`, `disable-library-validation`, `device.audio-input`, `automation.apple-events`). App Sandbox is **not** enabled: it blocks the tray shortcut and Accessibility paste path.

### Ad-hoc / unsigned installers

- **macOS:** GitHub Releases are signed with an ad-hoc identity (`signingIdentity: "-"`), not Apple Developer ID / notarization. Gatekeeper will warn on first open.
- **Windows:** no Authenticode. SmartScreen will warn; choose «More info» → «Run anyway».

See [docs/INSTALL.md](docs/INSTALL.md). Trust is: open source + your own build, or the zip/exe attached to a GitHub Release you choose to trust.

### Data that can remain on the machine

- **Clipboard** — full recognized text until something else overwrites it
- **Notification Center / Windows toasts** — may show a short preview of the phrase
- **Temp WAV** — written under the system temp directory and deleted after recognition; a crash mid-STT can leave an orphan file
- **Settings** — macOS `~/Library/Application Support/dictator/`; Windows `%APPDATA%\dictator\` — settings and `last-paste.log` (paste diagnostics: exe path, char counts — not the transcript text; macOS also logs the Accessibility trust flag)

## Supply chain

- STT weights are fetched by `scripts/fetch-stt-model.sh` with a pinned SHA-256 of the upstream archive
- App code has no updater and no analytics SDK
