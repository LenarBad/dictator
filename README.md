# Dictator

<img src="desktop/public/icon.png" width="96" alt="Иконка Dictator">

Офлайн-диктовка на русском: говорите - текст вставляется в активное поле. Без облака, без аккаунта.

**Сейчас только macOS 12+ на Apple Silicon.** Windows и Linux будут в этом же репозитории позже.

- STT: [GigaAM-v3](https://github.com/salute-developers/GigaAM) RNNT e2e (Sber, MIT), полностью на компьютере
- Пунктуация встроена в модель `v3_e2e_rnnt`
- Tray-приложение на [Tauri 2](https://tauri.app): хоткей, микрофон, вставка. Python не нужен

Лицензия: [MIT](LICENSE), © 2026 [LenarBad](https://github.com/LenarBad).  
Сторонние компоненты: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Безопасность: [SECURITY.md](SECURITY.md).

<details>
<summary>English</summary>

Offline Russian dictation: speak, and the text is pasted into the focused field. No cloud, no account. Recognition runs on-device with [GigaAM-v3](https://github.com/salute-developers/GigaAM) (MIT). **Currently macOS 12+ on Apple Silicon only.**

Download `Dictator-macos-aarch64.app.zip` from [Releases](https://github.com/LenarBad/dictator/releases/latest), move `Dictator.app` to Applications, then open it with right-click → Open. The build is ad-hoc signed, so Gatekeeper will warn. Enable Microphone and Accessibility. Full install steps (Russian): [docs/INSTALL.md](docs/INSTALL.md).

Once installed, the app does not contact the network. Threat model: [SECURITY.md](SECURITY.md).

</details>

---

## Установка

Mac с чипом **Apple** (M1–M4), **macOS 12+**. Intel пока не собираем.

1. Скачайте `Dictator-macos-aarch64.app.zip` (~250 МБ) из [Releases](https://github.com/LenarBad/dictator/releases/latest)
2. Перетащите `Dictator.app` в **Программы**
3. Первый запуск: правый клик → **Открыть** (ad-hoc подпись, Gatekeeper предупредит). Не жмите **Move to Trash**
4. Включите **Микрофон** и **Универсальный доступ** для `/Applications/Dictator.app`

Иконки в Dock нет — Dictator в **строке меню**, справа вверху. Запускайте копию из «Программ», не из «Загрузок».

Подробно и FAQ: **[docs/INSTALL.md](docs/INSTALL.md)**. Сборка из исходников: **[docs/BUILD.md](docs/BUILD.md)**.

## Использование

Поставьте курсор в поле, нажмите `ctrl+shift+d`, говорите, нажмите хоткей ещё раз. Текст копируется в буфер и вставляется через Cmd+V.

| Действие | Как |
|---|---|
| Старт / стоп | `ctrl+shift+d` или пункт меню иконки |
| Настройки | Меню иконки → «Настройки…» |
| Только копировать | В настройках выключите авто-вставку |
| Выход | Меню иконки → «Выход» |

Запись длиннее ~25 с режется на чанки. Не назначайте `ctrl+space` — macOS забирает его на смену языка.

Настройки: `~/Library/Application Support/dictator/settings.json`. После установки приложение **не ходит в интернет**  [SECURITY.md](SECURITY.md).
