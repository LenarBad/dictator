# План: Dictator на Windows

Это **план**, не руководство по установке. Сейчас в Releases только Mac. Windows появится в этом же репозитории: один Tauri-бинарь, один pipeline, без форка приложения.

Цель первого релиза: **Windows 10 1809+ x64**. ARM64 и Linux — позже, как Intel Mac сейчас.

## Кто где работает

На тестовом Windows **не ставить** Visual Studio, Rust, Node, WebView2 SDK, `tauri dev`. ПК нужен только как пользователь: скачал установщик → поставил → прогнал сценарии.

| Машина | Роль |
|---|---|
| Mac | Весь код, проверка что macOS не сломалась |
| GitHub `windows-2022` | Единственный компилятор Windows (`x86_64-pc-windows-msvc`, NSIS + zip) |
| Windows 10 1809+ x64 | Установка артефакта и ручной прогон. Runtime: WebView2 Evergreen — его подтянет NSIS bootstrapper |

Кросс-сборка с Mac на MSVC не используется.

## Статус

| Шаг | Состояние |
|---|---|
| 1. Рефакторинг без поведения | **сделано** — `desktop/src-tauri/src/platform/` (`mod.rs`, `macos.rs`, заглушка `other.rs`) |
| 2. CI Windows (артефакт без релиза) | workflow добавлен — нужен push и Actions → Release Windows → Run workflow |
| 3. Windows компилируется | **в работе** — `platform/windows.rs`: буфер на main thread, Ctrl+V |
| 4. Паритет продукта | частично: HUD `NOACTIVATE`, тосты, `set_icon` |
| 5. Доки + тег в GitHub Release | не начато |

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

- **`stt.rs` `candidate_model_dirs`**: сейчас ищет `Contents/MacOS` / `Resources`. Добавить `<exe_dir>/gigaam` и `<exe_dir>/resources/gigaam` — путь установленного `.exe`, не `tauri dev`.
- **`settings_path`**: на Windows сейчас `%APPDATA%\dictator\dictator\settings.json` (двойной `push`). Оставить `%APPDATA%\dictator\settings.json`.
- **Микрофоны**: маркеры `microphone array`, `realtek`, `stereo mix`; `vb-audio` / `cable input` уже есть.
- **Хоткеи в UI**: `format.ts` рисует ⌃⇧. С `platform` в state: Windows → `Ctrl+Shift+D`.
- **`scripts/fetch-stt-model.sh`**: `shasum` непереносим. Для CI: `sha256sum` **или** `shasum` (Git Bash на `windows-2022`).

## Что сознательно не делать

- Не второй фронт и не второй pipeline.
- Не GPU (DirectML) в первом релизе — CPU, как на Mac.
- Не Linux.
- Не Windows ARM64. Цель: **Windows 10 1809+ x64**, WebView2 Evergreen (NSIS bootstrapper Tauri).
- Не Authenticode в том же заходе (SmartScreen ≈ Gatekeeper: «Подробнее» → «Выполнить в любом случае»).
- Не переносить vendored patch `tray-icon` — он только про macOS-меню.
- Не `tauri build` / `tauri dev` на тестовом ПК и не кросс-компиляция с Mac.

## Сборка: GitHub, не локальный Windows

Отдельный workflow, не ломая `.github/workflows/release-macos.yml`. Зеркало Mac: `workflow_dispatch` даёт zip/NSIS **без** GitHub Release; тег `v*` кладёт файлы в тот же Release.

| | Mac (как есть) | Windows |
|---|---|---|
| Workflow | `release-macos.yml` | `release-windows.yml` |
| Runner | `macos-14` | `windows-2022` |
| Target | `aarch64-apple-darwin` | `x86_64-pc-windows-msvc` |
| Артефакт | `Dictator-macos-aarch64.app.zip` | NSIS `Dictator-windows-x64.exe` (основной для теста) + zip папки (модель внутри, ~250 МБ) |
| Actions → Run workflow | артефакт, без релиза | то же |
| Tag `v*` | оба workflow кладут файлы в один GitHub Release (`softprops`, upsert) | |

NSIS — то, что ставить на ПК (bootstrapper WebView2). Zip — запасной портативный прогон.

`scripts/fetch-stt-model.sh` (+ `sha256sum` или `shasum`) и `scripts/package-windows.sh` (NSIS → `Dictator-windows-x64.exe`, папка exe+модель → `Dictator-windows-x64.zip`). sherpa при первой сборке качает static-MT; кэшировать cargo target. Job заведён **до** `windows.rs`: текущая заглушка `other.rs` должна собраться и дать установщик для дымового теста.

`.cursor/rules/release.mdc` знает оба workflow: тег `v*` — оба attach в один Release; без релиза — Run workflow.

### Цикл итерации (без dev-стека на ПК)

1. Правка на Mac, push ветки.
2. Actions → Release Windows → Run workflow (эта ветка). Ждать ~15–40 мин.
3. Скачать `Dictator-windows-x64.exe` с run, поставить на ПК, прогнать чеклист ниже.
4. Замечания → снова шаг 1.

Компилятор — лог Actions. Если `cfg(windows)` не собрался, править на Mac по ошибке job, не ставя MSVC дома.

## Документация (после рабочего бинаря)

README, [INSTALL.md](INSTALL.md), [SECURITY.md](../SECURITY.md), [CONTRIBUTING.md](../CONTRIBUTING.md): Windows 10+ x64, трей (не Dock), микрофон в Параметрах, авто-вставка = Ctrl+V, SmartScreen, путь `%APPDATA%\dictator\`. Linux по-прежнему «позже».

Это руководство (`docs/WINDOWS.md`) после релиза можно свернуть до короткой пометки «сделано» или заменить ссылкой на INSTALL.

## Порядок работ

Чтобы Mac не разъехался. CI — раньше адаптера: иначе нечем собрать установщик.

1. **Рефакторинг без поведения** — **сделано.** `platform/mod.rs`, перенос `macos.rs`, `pipeline` / `paste` / `lib` / `hud` / `notify` зовут `platform::*`.
2. **CI Windows** — `release-windows.yml` в репозитории. После push: Actions → Release Windows → Run workflow (эта ветка). Скачать NSIS, убедиться что ставится и открывается (clipboard-only ок).
3. **Windows компилируется** — на Mac заменить `other.rs` на `windows.rs`: tray, настройки, запись, STT, буфер. Push → дождаться зелёного job. Можно жить с «только копировать».
4. **Паритет продукта** — фокус + Ctrl+V, HUD `NOACTIVATE`, тосты, строка микрофона, иконки трея. Каждая порция — через артефакт на ПК, не через `tauri dev`.
5. **Доки** — README / INSTALL / SECURITY / CONTRIBUTING, когда бинарь уже ставится с Actions.

Шаг 1 закрыт; 2 — компилятор; 3–4 — логика; 5 — когда продукт уже гоняется с установщика.

## Риски, которые стоит прогнать руками на ПК

Ставить **NSIS с Actions**, не собранный локально. На ПК: микрофон в Параметрах, SmartScreen, трей (не панель задач для HUD).

1. HUD перехватывает фокус → вставка в себя.
2. Прозрачность WebView2.
3. Иконка трея не меняется из-за no-op `set_icon_with_as_template`.
4. Модель не находится рядом с exe.
5. Первый запуск микрофона (Privacy).
6. Chrome / Telegram / Notepad / VS Code / Word.
7. Окно «Запуск от имени администратора».

## Итог

На Windows тот же хоткей `ctrl+shift+d`, та же модель, тот же HUD, иконка в трее, текст в активное поле через буфер + Ctrl+V. Код пишется на Mac, собирает GitHub, на ПК только ставится NSIS. Новый код — в основном `platform/windows.rs` и CI, не вторая копия Dictator.
