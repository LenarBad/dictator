# Dictator

<img src="desktop/public/icon.png" width="96" alt="Иконка Dictator">

Офлайн-диктовка на русском: говорите — текст вставляется в активное поле. Без облака, без аккаунта.

**Сейчас только macOS 12+.** Windows и Linux будут в этом же репозитории позже: запись и распознавание уже общие, авто-вставка пока только через Cmd+V.

- STT: [GigaAM-v3](https://github.com/salute-developers/GigaAM) RNNT e2e (Sber, MIT), полностью на компьютере
- Пунктуация встроена в модель `v3_e2e_rnnt`
- Tray-приложение на [Tauri 2](https://tauri.app): хоткей, микрофон, вставка. Python не нужен

Лицензия: [MIT](LICENSE), © 2026 [LenarBad](https://github.com/LenarBad).  
Сторонние компоненты: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Безопасность: [SECURITY.md](SECURITY.md).

<details>
<summary>English</summary>

Offline Russian dictation for the Mac menu bar: speak, and text is pasted into the focused field. No cloud, no account. Recognition runs on-device with [GigaAM-v3](https://github.com/salute-developers/GigaAM) (MIT). **macOS 12+ on Apple Silicon only** for now.

Install from [Releases](https://github.com/LenarBad/dictator/releases/latest) (`Dictator-macos-aarch64.app.zip`), move `Dictator.app` to Applications, allow Gatekeeper via right-click → Open (ad-hoc signature), then enable Microphone and Accessibility. Full steps (RU): [docs/INSTALL.md](docs/INSTALL.md).

Privacy: after install the app does not phone home. Notifications may show a short text preview; the clipboard keeps the full phrase; a crash mid-recognition can leave a temp WAV. Threat model and vulnerability reports: [SECURITY.md](SECURITY.md).

</details>

---

## Установка (не нужно быть разработчиком)

Нужен Mac с чипом **Apple** (M1, M2, M3, M4) и **macOS 12+**. Intel пока не собираем.

1. Откройте [Releases](https://github.com/LenarBad/dictator/releases/latest)
2. Скачайте `Dictator-macos-aarch64.app.zip` (~250 МБ, модель уже внутри)
3. Распакуйте архив и перетащите `Dictator.app` в папку **Программы**
4. При первом запуске macOS покажет предупреждение Gatekeeper (ad-hoc подпись). Нажмите **Done** / **Готово**, затем **Системные настройки → Конфиденциальность и безопасность → Всё равно открыть**. Либо правый клик по Dictator → **Открыть** → снова **Открыть**. Не жмите **Move to Trash**.
5. **Системные настройки → Конфиденциальность и безопасность:**
   - **Микрофон** — включите Dictator
   - **Универсальный доступ** — добавьте `/Applications/Dictator.app`

Иконки в Dock не будет. Ищите Dictator **в строке меню справа вверху**, рядом с часами.

Запускайте **только** копию в «Программах», не из «Загрузок». В списках разрешений должен быть **Dictator**, не Terminal. После обновления с нового zip удалите Dictator из универсального доступа и добавьте `/Applications/Dictator.app` заново.

Подробно, FAQ (Gatekeeper, вставка, обновления) и сборка из исходников: **[docs/INSTALL.md](docs/INSTALL.md)**.

---

## Использование

| Действие | Как |
|---|---|
| Старт / стоп записи | Хоткей по умолчанию `ctrl+shift+d` или пункт меню иконки |
| Настройки | Меню иконки → «Настройки…» |
| Только копировать | В настройках выключите авто-вставку |
| Выход | Меню иконки → «Выход» |

Поставьте курсор в текстовое поле, нажмите хоткей, говорите, нажмите хоткей ещё раз. Текст вставится в поле.

Статусы иконки: ожидание / запись / распознавание.

Запись длиннее ~25 с автоматически режется на чанки (лимит short-form API GigaAM).

На macOS не используйте `ctrl+space` / `ctrl+shift+space` — система забирает их на смену языка. Если хоткей молчит, запись можно включить кликом по иконке в строке меню.

## Как это работает

1. Глобальный хоткей включает запись с микрофона
2. Повтор хоткея останавливает запись
3. Аудио распознаётся GigaAM-v3 **без сети**
4. Текст копируется в буфер и вставляется через Cmd+V

## Приватность

- Распознавание локальное. После установки приложение **не отправляет голос и текст в интернет**.
- WAV пишется во временную папку и **удаляется** после распознавания (в том числе при ошибке). Если процесс убить во время STT, временный файл может остаться — его можно удалить вручную из temp.
- Сеть нужна, чтобы скачать zip с GitHub. После установки приложение **не ходит в интернет**.
- Уведомление macOS может показать **начало** распознанной фразы (Notification Center может хранить превью). В буфере обмена остаётся **полный** текст, пока вы его не замените.
- В чувствительных полях выключите авто-вставку в настройках — текст только в буфер.
- Подробнее (права Accessibility, ad-hoc подпись): [SECURITY.md](SECURITY.md).

## Настройки

`~/Library/Application Support/dictator/settings.json`

- хоткей
- авто-вставка или только буфер
- устройство микрофона
- предзагрузка модели при старте

Диагностика вставки: `~/Library/Application Support/dictator/last-paste.log`. Нужно `trusted=true` и `exe=.../Applications/Dictator.app/...`.
