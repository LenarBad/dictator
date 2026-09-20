# Dictator desktop (Tauri 2)

Только macOS. Если вы не разработчик — установка в корневом [README](../README.md) и в [docs/INSTALL.md](../docs/INSTALL.md).

Ежедневный путь — **`/Applications/Dictator.app`**: tray, глобальный хоткей, запись WAV, вставка.
GigaAM-v3 e2e RNNT работает внутри того же бинаря через sherpa-onnx.

## Сборка .app (self-contained)

```bash
cd desktop
npm install
npm run build:app
```

Скрипт скачивает ONNX (~221 MB) в `src-tauri/resources/gigaam`, собирает `.app`,
подписывает ad-hoc и копирует в `/Applications/Dictator.app`.

Dev без установки в `/Applications`:

```bash
bash ../scripts/fetch-stt-model.sh
npm install
npm run tauri dev
```

Иконка в строке меню. Клик открывает меню (Начать запись / Настройки / Выход).
Хоткей по умолчанию `ctrl+shift+d`.

Запись: 16 кГц mono PCM16. Настройки:
`~/Library/Application Support/dictator/settings.json`.

Для проверки без GigaAM:

```bash
DICTATOR_STT_STUB=1 npm run tauri dev
```

## macOS .app и разрешения

Локально стоит ad-hoc подпись (`signingIdentity: "-"`), чтобы TCC (микрофон и
универсальный доступ) вешался на **Dictator.app** / `com.dictator.desktop`.

Запускайте **только** `/Applications/Dictator.app`. После установки скрипт
удаляет копию в `target/.../bundle/macos`, иначе Spotlight показывает два Dictator.
Копия из `target/` — другой CDHash, галочка в Accessibility на неё не действует.

После пересборки ad-hoc подпись меняется: удалите Dictator из Универсального
доступа, добавьте `/Applications/Dictator.app` заново.

- `Info.plist` — тексты для микрофона и Apple Events, `LSUIElement` (без Dock)
- `Entitlements.plist` — Hardened Runtime: audio-input, Apple Events, JIT для WebView.
  **Без App Sandbox**
- Универсальный доступ в entitlements не кодируется: пользователь добавляет Dictator
  в Системных настройках

Диагностика вставки: `~/Library/Application Support/dictator/last-paste.log`
(`trusted=true`, `exe=.../Applications/Dictator.app/...`).

Нотаризация и Developer ID — когда появится сертификат: замените `signingIdentity`
на `Developer ID Application: …` и прогоните `notarytool`. Без сертификата Apple
Store/Gatekeeper «неизвестный разработчик» останется.
