# Dictator на Windows

Установка для пользователя: [INSTALL.md](INSTALL.md). Ниже — как устроен адаптер, не план порта.

Windows — тот же продукт, что macOS: один Tauri-бинарь, один `pipeline` / `stt` / HUD / настройки. Адаптер — `desktop/src-tauri/src/platform/windows.rs` (HWND, буфер, Ctrl+V, HUD `NOACTIVATE`, тосты). `other.rs` — только не-Mac/не-Windows.

**Windows 10 1809+ x64.** ARM64 и Linux — как Intel Mac: позже.

## Скачать

[Releases](https://github.com/LenarBad/dictator/releases/latest):

- `Dictator-windows-x64.exe` — NSIS, то, что ставить (подтянет WebView2 Evergreen)
- `Dictator-windows-x64.zip` — портативная папка, модель внутри

На тестовом ПК не ставить Visual Studio, Rust, Node. Сборщик — GitHub `windows-2022` (`.github/workflows/release-windows.yml`). Тег `v*` кладёт файлы в тот же Release, что Mac. Android в этот Release не входит: у него тег `android-v*`. `workflow_dispatch` — только артефакты.

## Тот же сценарий, другие API

| | macOS | Windows |
|---|---|---|
| Иконка | строка меню | трей |
| Вставка | AX, запасной Cmd+V | буфер + Ctrl+V (UIA нет) |
| Разрешения | Микрофон + Универсальный доступ | Микрофон в Параметрах; Accessibility нет |
| Первый запуск | Gatekeeper, ad-hoc | SmartScreen: «Подробнее» → «Выполнить в любом случае»; Authenticode нет |
| Настройки | `~/Library/Application Support/dictator/` | `%APPDATA%\dictator\` |

Хоткей тот же: `ctrl+shift+d`. Elevated-окна (админский терминал) SendInput не примут — текст останется в буфере.

## Цикл правки

Код на Mac, проверку macOS не ломать. Push → Actions → Release Windows → Run workflow → NSIS с run на ПК. Кросс-сборка с Mac на MSVC не используется.

## Не переписывать ради Windows

GigaAM, sherpa-onnx CPU, recorder/cpal, общий фронт. GPU (DirectML) не включать. Vendored `tray-icon` — патч только про macOS-меню.
