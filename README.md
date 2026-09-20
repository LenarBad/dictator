# Dictator

<img src="desktop/public/icon.png" width="96" alt="Иконка Dictator">

Офлайн-диктовка на русском: говорите — текст вставляется в активное поле. Без облака, без аккаунта.

**Сейчас только macOS 12+.** Windows и Linux будут в этом же репозитории позже: запись и распознавание уже общие, авто-вставка пока только через Cmd+V.

- STT: [GigaAM-v3](https://github.com/salute-developers/GigaAM) RNNT e2e (Sber, MIT), полностью на компьютере
- Пунктуация встроена в модель `v3_e2e_rnnt`
- Tray-приложение на [Tauri 2](https://tauri.app): хоткей, микрофон, вставка. Python не нужен

Лицензия: [MIT](LICENSE), © 2026 [LenarBad](https://github.com/LenarBad).

---

## Установка (не нужно быть разработчиком)

Нужен Mac с чипом **Apple** (M1, M2, M3, M4) и **macOS 12+**. Intel пока не собираем.

1. Откройте [Releases](https://github.com/LenarBad/dictator/releases/latest)
2. Скачайте `Dictator-macos-aarch64.app.zip` (~250 МБ, модель уже внутри)
3. Распакуйте архив и перетащите `Dictator.app` в папку **Программы**
4. Правый клик по Dictator → **Открыть** → ещё раз **Открыть**  
   (macOS предупредит, что разработчик неизвестен — это ad-hoc подпись)
5. **Системные настройки → Конфиденциальность и безопасность:**
   - **Микрофон** — включите Dictator
   - **Универсальный доступ** — добавьте `/Applications/Dictator.app`

Иконки в Dock не будет. Ищите Dictator **в строке меню справа вверху**, рядом с часами.

Подробно, с ошибками Gatekeeper и сборкой из исходников: **[docs/INSTALL.md](docs/INSTALL.md)**.

Запускайте **только** копию в «Программах», не из «Загрузок». В списках разрешений должен быть **Dictator**, не Terminal. После обновления с нового zip удалите Dictator из универсального доступа и добавьте `/Applications/Dictator.app` заново.

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
- WAV пишется во временную папку и **удаляется** сразу после распознавания.
- Сеть нужна, чтобы скачать zip с GitHub. После установки приложение **не ходит в интернет**.
- Уведомление macOS может показать начало распознанной фразы. В буфере обмена остаётся полный текст, пока вы его не замените.

## Настройки

`~/Library/Application Support/dictator/settings.json`

- хоткей
- авто-вставка или только буфер
- устройство микрофона
- предзагрузка модели при старте

Диагностика вставки: `~/Library/Application Support/dictator/last-paste.log`. Нужно `trusted=true` и `exe=.../Applications/Dictator.app/...`.

## Сборка для разработки

Ежедневный путь — **`/Applications/Dictator.app`**. Скрипт `npm run build:app` скачивает ONNX, если файлов ещё нет, подписывает ad-hoc и ставит приложение в `/Applications`.

Сборка с ad-hoc подписью: Gatekeeper покажет «неизвестный разработчик». Не открывайте `.app` из репозитория: универсальный доступ привязан к подписи конкретного бандла. Скрипт сборки удаляет копию в `bundle/macos`, чтобы Spotlight не предлагал её рядом с `/Applications`.

Для разработки без пересборки бандла:

```bash
bash scripts/fetch-stt-model.sh
cd desktop
npm install
npm run tauri dev
```

Проверка UI/хоткея/вставки **без модели**:

```bash
cd desktop
DICTATOR_STT_STUB=1 npm run tauri dev
```

Требования для разработки: Node и Rust. Первый `fetch-stt-model` качает чекпоинт (~221 MB) в `desktop/src-tauri/resources/gigaam`. ffmpeg не нужен.

```bash
cd desktop/src-tauri
cargo test
```

Тесты не грузят ONNX. Живое распознавание — через `npm run tauri dev` или `/Applications/Dictator.app`.

Нотаризация и Developer ID — когда появится сертификат: замените `signingIdentity` на `Developer ID Application: …` и прогоните `notarytool`.

Релиз для людей собирает GitHub Actions: тег `v0.1.0` → zip на странице Releases. Вручную: Actions → **Release macOS** → Run workflow.

## Структура

```
desktop/src/                     # окно настроек (Vite + TypeScript)
desktop/src-tauri/src/           # tray, хоткей, запись, STT, вставка
desktop/src-tauri/src/stt.rs     # sherpa-onnx + GigaAM ONNX
desktop/src-tauri/resources/gigaam/  # модель скачивается, не коммитится
scripts/fetch-stt-model.sh
scripts/build-macos-app.sh
scripts/package-macos-zip.sh
.github/workflows/release-macos.yml
docs/INSTALL.md
LICENSE
```

Модель задаётся переменной `DICTATOR_MODEL_DIR`, если файлы лежат не в бандле и не в `resources/gigaam`.

Сторонние компоненты: [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (Apache-2.0), веса GigaAM (MIT).
