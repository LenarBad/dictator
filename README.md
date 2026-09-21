# Dictator

<img src="desktop/public/icon.png" width="88" alt="Dictator">

**Говорите — текст появляется.** Модель на Mac. Голос никуда не уходит.

Speak — the text appears. On-device Russian STT. No cloud, no account.

**Офлайн** · **Русский** · **Без аккаунта** · **macOS 12+ · Apple Silicon**

<p align="center">
  <img src="docs/hero.svg" alt="Запись: волна, таймер, хоткей. Текст появляется в Заметках." width="720">
</p>

**[Скачать для Mac](https://github.com/LenarBad/dictator/releases/latest)** — `Dictator-macos-aarch64.app.zip`, около 250 МБ.

`⌃⇧D` начинает запись. Сверху экрана бежит волна. Ещё раз — текст в активном поле.

Сейчас только **macOS 12+ на Apple Silicon**. Windows и Linux будут в этом репозитории позже.

Распознавание: [GigaAM-v3](https://github.com/salute-developers/GigaAM) (Sber, MIT), полностью на компьютере. Оболочка — [Tauri 2](https://tauri.app), Python не нужен.

## Как пользоваться

Поставьте курсор в поле и нажмите хоткей. Говорите. Нажмите ещё раз — или кликните по индикатору сверху.

| Действие | Как |
|---|---|
| Старт / стоп | `ctrl+shift+d`, пункт меню иконки или клик по индикатору |
| Настройки | Меню иконки → «Настройки…» |
| Только копировать | В настройках выключите авто-вставку |
| Выход | Меню иконки → «Выход» |

Иконки в Dock нет — Dictator в **строке меню**, справа вверху. Запись длиннее ~25 с режется на чанки. Не назначайте `ctrl+space`: macOS забирает его на смену языка.

Настройки пишутся сразу, без кнопки «Сохранить». Раздел «Разрешения» прячется, когда микрофон и универсальный доступ уже выданы.

## Установка

Mac с чипом **Apple** (M1–M4), **macOS 12+**. Intel пока не собираем.

1. Скачайте zip из [Releases](https://github.com/LenarBad/dictator/releases/latest)
2. Перетащите `Dictator.app` в **Программы**
3. Первый запуск: правый клик → **Открыть** (ad-hoc подпись, Gatekeeper предупредит). Не жмите **Move to Trash**
4. Включите **Микрофон** и **Универсальный доступ** для `/Applications/Dictator.app`

Запускайте копию из «Программ», не из «Загрузок». После установки приложение **не ходит в интернет**.

Подробно и FAQ: **[docs/INSTALL.md](docs/INSTALL.md)**. Сборка из исходников: **[docs/BUILD.md](docs/BUILD.md)**. Угрозы: **[SECURITY.md](SECURITY.md)**.

## English

Offline Russian dictation: speak, and the text is pasted into the focused field. Recognition runs on-device with [GigaAM-v3](https://github.com/salute-developers/GigaAM) (MIT). **Currently macOS 12+ on Apple Silicon only.**

Download `Dictator-macos-aarch64.app.zip` from [Releases](https://github.com/LenarBad/dictator/releases/latest), move `Dictator.app` to Applications, then open it with right-click → Open. The build is ad-hoc signed, so Gatekeeper will warn. Enable Microphone and Accessibility. A recording pill appears at the top of the screen while you speak.

Full install steps (Russian): [docs/INSTALL.md](docs/INSTALL.md). Threat model: [SECURITY.md](SECURITY.md). Once installed, the app does not contact the network.

## Лицензия

[MIT](LICENSE), © 2026 [LenarBad](https://github.com/LenarBad).  
Сторонние компоненты: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
