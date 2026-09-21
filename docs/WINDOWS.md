# План: Dictator на Windows

Это **план**, не руководство по установке. Сейчас в Releases только Mac. Windows появится в этом же репозитории: один Tauri-бинарь, один pipeline, без форка приложения.

Цель первого релиза: **Windows 10 1809+ x64**. ARM64 и Linux — позже, как Intel Mac сейчас.

## Статус

| Шаг | Состояние |
|---|---|
| 1. Рефакторинг без поведения | **сделано** — `desktop/src-tauri/src/platform/` (`mod.rs`, `macos.rs`, заглушка `other.rs`) |
| 2. Windows компилируется | не начато |
| 3. Паритет продукта | не начато |
| 4. CI + zip/NSIS, доки | не начато |

## Принцип

Не копировать `macos.rs` «один в один». Вынести **общий контракт**, macOS оставить как есть, Windows реализовать те же шаги другими API.

```
UI (index.html, hud.html, main.ts)     ← почти без изменений
pipeline / recorder / stt / wav / hud  ← общие
hotkey, settings, tray-меню            ← общие
platform::{capture, paste, notify, chrome}
   platform/macos.rs   — AX + Cmd+V + TCC
   platform/windows.rs — HWND + Ctrl+V + Privacy  (ещё нет)
   platform/other.rs   — clipboard-only, пока нет windows.rs
```

`pipeline.rs`, `paste.rs`, `lib.rs`, `hud.rs` и `notify.rs` зовут `platform::*`. Поведение Mac не менялось: шаг 1 — перенос в `platform/macos.rs`.

Ядро уже кроссплатформенное (запись через cpal, GigaAM/sherpa-onnx на CPU, HUD/настройки на HTML, хоткей-плагин). Не хватает вставки, фокуса, тостов, пути к модели у `.exe` и CI.

Заготовки в коде:

- `windows_subsystem = "windows"` в `desktop/src-tauri/src/main.rs`
- `open_permission` → `ms-settings:privacy-microphone` в `platform/other.rs`
- clipboard-only вставка в `platform/other.rs`

## Что переиспользовать без переписывания

| Слой | Почему уже ок |
|---|---|
| `pipeline`, `wav`, STT stub | Логика диктовки не про OS |
| `recorder` + cpal 0.16 | На Windows это WASAPI |
| `stt` + sherpa-onnx 1.13, `provider = cpu` | Крейт сам тянет `win-x64-static-MT`; CUDA/DirectML не трогать |
| `hotkey.rs` | Уже понимает `win` / `super` |
| Tray-меню, команды, debounce 350 ms | Tauri tray на Windows есть; vendored `tray-icon` трогает только macOS-меню |
| HUD HTML/CSS, настройки, Vite | Один фронт |
| Модель GigaAM в `bundle.resources` | Tauri кладёт её рядом с exe |
| `main.rs` `windows_subsystem = "windows"` | Консоль в release уже скрыта |

## Windows-адаптер

Новый код: `desktop/src-tauri/src/platform/windows.rs` и crate `windows` (`Win32_UI_*`, clipboard, optional WinRT toast). Зависимости только под `cfg(windows)`, как сейчас objc2 только на Mac.

### 1. Вставка — тот же сценарий, что fallback на Mac

На Mac основной путь — AX, запасной — Cmd+V. Браузеры и Electron и так идут в Cmd+V. **В Windows v1 делать только этот запасной путь**, без UI Automation:

1. В момент старта записи: `GetForegroundWindow` + pid (пропуск своего процесса, `Shell_TrayWnd`, Progman).
2. После STT: текст в буфер (`tauri-plugin-clipboard-manager`, он уже в `platform/other.rs`; на Mac — NSPasteboard).
3. `SetForegroundWindow` на сохранённый HWND, пауза ~80–150 ms (как `PASTE_DELAY`).
4. `SendInput`: Ctrl down → V down/up → Ctrl up.

UIA `ValuePattern` — отдельный follow-up, аналог AX. Не блокирует первый релиз.

Ограничение (документировать в INSTALL): в **elevated** окна (админский терминал, некоторые установщики) из обычного процесса SendInput не дойдёт — текст останется в буфере, как без универсального доступа на Mac.

### 2. HUD не должен воровать фокус

Иначе Ctrl+V уйдёт в пилюлю. На Mac это `resignKeyWindow` + Accessory. На Windows в `hud.rs` рядом с `configure_panel`:

- уже есть `focus: false`, `skipTaskbar`, `alwaysOnTop`;
- дополнительно HWND: **OR** к текущему `GWL_EXSTYLE` флагов `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` (не затирать стиль — сломается прозрачность);
- `SetWindowPos(..., SWP_NOACTIVATE | SWP_FRAMECHANGED)`.

Клик по HUD должен останавливать запись **без** активации окна.

Прозрачность WebView2 капризная. Если acrylic / `windowEffects: hudWindow` даст серый прямоугольник — непрозрачная тёмная пилюля из текущего CSS, без отдельного UI.

### 3. Tray

`icon_as_template(true)` и `set_icon_with_as_template` — macOS. В vendored `tray-icon` метод `set_icon_with_as_template` на не-Mac — **no-op**, иконка статуса на Windows не сменится. Нужно:

```
macOS   → template + set_icon_with_as_template
Windows → обычный PNG/ICO через set_icon
```

Те же `tray-idle.png` / `recording` / `transcribing`. Левый клик открывает меню (уже `show_menu_on_left_click`).

Окно настроек: без `ActivationPolicy`; на Windows `skipTaskbar: false`, чтобы его было видно на панели. Vibrancy — опционально (`window-vibrancy` acrylic), иначе сплошной тёмный фон.

### 4. Разрешения и тосты

На Windows нет аналога TCC Accessibility. `accessibility_trusted()` = `true`, строка «Универсальный доступ» скрывается (`platform` в `UiState`).

Микрофон: не заглушка `true`. Проверка через попытку открыть WASAPI / WinRT Microphone capability; кнопка уже открывает `ms-settings:privacy-microphone`.

Тосты: в `notify.rs` ветка Windows (WinRT toast или `tauri-plugin-notification`). macOS `osascript` не трогать в том же PR, если нет нужды.

### 5. Мелочи в общем коде

Не дублировать OS-файлы ради этого.

- **`stt.rs` `candidate_model_dirs`**: сейчас ищет `Contents/MacOS` / `Resources`. Добавить `<exe_dir>/gigaam` и `<exe_dir>/resources/gigaam` — нужно и для Windows, и для `tauri dev`.
- **`settings_path`**: на Windows сейчас `%APPDATA%\dictator\dictator\settings.json` (двойной `push`). Оставить `%APPDATA%\dictator\settings.json`.
- **Микрофоны**: маркеры `microphone array`, `realtek`, `stereo mix`; `vb-audio` / `cable input` уже есть.
- **Хоткеи в UI**: `format.ts` рисует ⌃⇧. С `platform` в state: Windows → `Ctrl+Shift+D`.
- **`scripts/fetch-stt-model.sh`**: `shasum` непереносим. Для CI: `sha256sum` **или** `shasum` (Git Bash на `windows-2022`).

## Что сознательно не делать

- Не второй фронт и не второй pipeline.
- Не GPU (DirectML) в первом релизе — CPU, как на Mac.
- Не Linux.
- Не Windows ARM64. Цель: **Windows 10 1809+ x64**, WebView2 Evergreen (NSIS bootstrapper Tauri).
- Не Authenticode в том же заходе (SmartScreen ≈ Gatekeeper).
- Не переносить vendored patch `tray-icon` — он только про macOS-меню.

## Сборка и релиз

Отдельный job, не ломая `.github/workflows/release-macos.yml`.

| | Mac (как есть) | Windows |
|---|---|---|
| Runner | `macos-14` | `windows-2022` |
| Target | `aarch64-apple-darwin` | `x86_64-pc-windows-msvc` |
| Артефакт | `Dictator-macos-aarch64.app.zip` | NSIS `Dictator-windows-x64.exe` + zip папки (модель внутри, ~250 МБ) |
| Tag `v*` | оба job кладут файлы в один GitHub Release (`softprops`, upsert) | |

Локально: `npm run tauri build -- --bundles nsis` (или zip), не `build:app` (он macOS-only).

`scripts/fetch-stt-model.sh` + кэш GigaAM — как на Mac. sherpa при первой сборке качает static-MT; кэшировать cargo target.

После появления Windows-артефакта обновить `.cursor/rules/release.mdc`: CI — не только `release-macos.yml`.

## Документация (после рабочего бинаря)

README, [INSTALL.md](INSTALL.md), [SECURITY.md](../SECURITY.md), [CONTRIBUTING.md](../CONTRIBUTING.md): Windows 10+ x64, трей (не Dock), микрофон в Параметрах, авто-вставка = Ctrl+V, SmartScreen, путь `%APPDATA%\dictator\`. Linux по-прежнему «позже».

Это руководство (`docs/WINDOWS.md`) после релиза можно свернуть до короткой пометки «сделано» или заменить ссылкой на INSTALL.

## Порядок работ

Чтобы Mac не разъехался.

1. **Рефакторинг без поведения** — **сделано.** `platform/mod.rs`, перенос `macos.rs`, `pipeline` / `paste` / `lib` / `hud` / `notify` зовут `platform::*`.
2. **Windows компилируется** — заменить `other.rs` на `windows.rs`: tray, настройки, запись, STT, буфер. Можно жить с «только копировать».
3. **Паритет продукта** — фокус + Ctrl+V, HUD `NOACTIVATE`, тосты, строка микрофона, иконки трея.
4. **CI + zip/NSIS** на тег, доки.

Шаг 1 закрыт; 2–3 — вся новая логика; 4 — зеркало macOS workflow.

## Риски, которые стоит прогнать руками на ПК

1. HUD перехватывает фокус → вставка в себя.
2. Прозрачность WebView2.
3. Иконка трея не меняется из-за no-op `set_icon_with_as_template`.
4. Модель не находится рядом с exe.
5. Первый запуск микрофона (Privacy).
6. Chrome / Telegram / Notepad / VS Code / Word.
7. Окно «Запуск от имени администратора».

## Итог

На Windows тот же хоткей `ctrl+shift+d`, та же модель, тот же HUD, иконка в трее, текст в активное поле через буфер + Ctrl+V. Новый код — в основном `platform/windows.rs` и CI, не вторая копия Dictator.
