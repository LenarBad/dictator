# План: Dictator на Android

Это **план**, не руководство по установке. Desktop уже есть на Mac и Windows. Android появится в этом же репозитории отдельным native-приложением: не порт Tauri, не `platform/windows.rs`.

На телефоне нет глобального хоткея и нет Cmd+V / Ctrl+V в чужое поле. Цель та же, что на Mac: **сказал — текст в том поле, где курсор**. Офлайн, русский, GigaAM, без аккаунта, после установки без сети.

Цель первого релиза: **Android 10+ (API 29), только arm64**. APK с GitHub, не Play Store.

## Кто где работает

На тестовом телефоне **не ставить** Android Studio, SDK, NDK. Телефон нужен только как пользователь: скачал APK → разрешил неизвестные источники → поставил → прогнал сценарии.

| Машина | Роль |
|---|---|
| Mac | Код Kotlin в `android/`, проверка что desktop/macOS не сломались |
| GitHub `ubuntu-latest` | Единственный компилятор APK (`arm64-v8a`) |
| Телефон Android 10+ arm64 | Установка артефакта и ручной прогон |

Эмулятор x86_64 не собираем и не считаем тестовой машиной.

## Статус

| Шаг | Состояние |
|---|---|
| 1. Каркас APK + мастер + пустой IME (`commitText("тест")`) | сделано |
| 2. Процесс `:stt`, AIDL, микрофон, stub STT | сделано |
| 3. GigaAM в assets через sherpa-onnx | сделано |
| 4. Плитка QS + `RECOGNIZE_SPEECH` | в работе — прогон на телефоне |
| 5. CI + README / INSTALL / SECURITY | частично: workflow тянет модель; доки INSTALL/SECURITY — после прогона |

Код Android не смешивать с desktop (`platform/windows.rs` уже закрыт).

## Почему не хоткей и не Tauri

Обычное приложение на Android не перехватывает клавиши глобально. Volume, питание и «ассистент» заняты системой. Стать default assistant нельзя: это замена Gemini.

Tauri 2 mobile даёт WebView-Activity, не клавиатуру. Вставить текст в Telegram можно только из:

1. **IME** (`InputMethodService` → `InputConnection.commitText()`) — основной путь, как голос Gboard. Accessibility не нужен. Клавиатуру пользователь включает сам (системный экран, тихо нельзя).
2. **`ACTION_RECOGNIZE_SPEECH` + subtype `voice`** — FlorisBoard, HeliBoard, AOSP, SwiftKey открывают Dictator. **Gboard и Samsung Keyboard свой микрофон третьим не отдают.** Это ограничение рынка, не баг.
3. **Accessibility + `FLAG_INPUT_METHOD_EDITOR` (API 33+)** — плавающая кнопка поверх Gboard. Ближе всего к хоткею, но выглядит как кейлоггер и бьётся о политику Play. **В v1 не делать.**

Оверлей `SYSTEM_ALERT_WINDOW` без IME сам текст не вставит. Один буфер — это «надиктовал в заметки», не мессенджер.

Rust `stt.rs`, HUD-WebView и tray не портировать. Те же веса GigaAM и sherpa-onnx (на Android — официальный JNI/AAR).

```
IME / плитка / Recognition UI     ← процесс приложения, без ONNX
SttService (process :stt)         ← микрофон, GigaAM, AIDL
SetupActivity                     ← мастер разрешений
```

## Зафиксированный UX v1

Не QWERTY (это другой продукт). Dictator — голосовой ввод рядом с Gboard.

Доставка по **источнику сессии**, без угадывания «IME сейчас активен?»:

| Откуда старт | Куда текст |
|---|---|
| Микрофон на панели Dictator | `commitText` в поле + всегда копия в буфер. Если в настройках «вернуться к прежней клавиатуре» (по умолчанию вкл) → `switchToPreviousInputMethod()` |
| Плитка Quick Settings | Только буфер + уведомление «Скопировано, вставьте». Плитка **не** вызывает `commitText`: `InputConnection` живёт только в IME |
| Чужая клавиатура / `RECOGNIZE_SPEECH` | `Bundle` распознавания в вызывающее приложение; если его нет — буфер |

Ежедневный сценарий IME:

1. Курсор в Telegram (часто на Gboard).
2. Глобус / список клавиатур → Dictator.
3. Большой микрофон, волна, таймер.
4. Ещё раз нажать — текст в поле, возврат на Gboard.

Плитка — замена хоткея. Тап всегда обрабатывается (не `STATE_UNAVAILABLE`: на части OEM клик тогда глотается). Сессия — `TileSessionActivity`, её `startActivityAndCollapse` вызывается синхронно из `onClick`: на Android 14+ `microphone` FGS разрешён только пока приложение на экране, `showDialog` этого не даёт и оставляет системное «Starting FGS…». «Стоп» и «назад» заканчивают запись. «Свернуть» прячет окно, запись продолжается: сервис к этому моменту уже started. Остановить свёрнутую запись — повторный тап по плитке или действие «Стоп» на уведомлении «Запись…». Пока идёт распознавание, новый сеанс с плитки не стартует. Текст только в буфер + уведомление.

Мастер первого запуска (не молчаливый tray):

- Кнопка в системные «Языки и ввод» (`Settings.ACTION_INPUT_METHOD_SETTINGS`)
- Проверка, что Dictator в `ENABLED_INPUT_METHODS`
- `RECORD_AUDIO`
- На API 33+: `POST_NOTIFICATIONS` (foreground-сервис и тост с результатом)
- Экран «это не клавиатура для набора, нажатия не читаем» — иначе сторонний IME пугает

## Что сознательно не делать в v1

- Tauri на телефоне, общий HTML UI, Rust JNI к `stt.rs`
- Полный QWERTY, свайп, автокоррект, эмодзи
- Accessibility, чат-хед, default assistant
- Play Store, Play Asset Delivery, сеть после установки (не объявлять `INTERNET`)
- NNAPI / GPU — `provider = cpu`. Если на среднем телефоне RTF >> 1 — follow-up: CTC GigaAM (`sherpa-onnx-nemo-ctc-punct-giga-am-v3-russian-…`), не смена UX в том же PR
- ABI кроме `arm64-v8a`
- Jetpack Compose в IME. Settings — XML
- Качать модель при первом запуске

## Стек

Новый каталог `android/`, один модуль `app`, Kotlin.

| | |
|---|---|
| `applicationId` | `io.lenar.dictator` (как `identifier` в `desktop/src-tauri/tauri.conf.json`) |
| minSdk / target / compile | 29 / 35 / 35 |
| ABI | только `arm64-v8a` |
| sherpa | JitPack `com.github.k2-fsa.sherpa-onnx:sherpa-onnx:v1.13.8` (как crate в `Cargo.toml`). Если JitPack на 1.13.8 не соберётся — ближайший `v1.13.x` с тем же `OfflineRecognizer`, версию зафиксировать здесь |
| UI | ViewBinding + XML. IME **не** импортирует `com.k2fsa.sherpa.onnx` (иначе `loadLibrary` в процессе клавиатуры) |
| versionName | своя линия, сейчас `0.1.0`. Релиз — тег `android-vX.Y.Z`. `versionCode` +1 на каждый Android-релиз после первого |

### Дерево

```
android/app/src/main/
  AndroidManifest.xml
  assets/gigaam/          # gitignore: *.onnx, tokens.txt; LICENSE как на десктопе
  aidl/.../ISttService.aidl
  aidl/.../ISttCallback.aidl
  res/xml/method.xml
  res/layout/ime_view.xml
  java/io/lenar/dictator/
    ime/DictatorImeService.kt
    stt/SttService.kt     # android:process=":stt"
    stt/Engine.kt         # только из :stt
    stt/AudioRecorder.kt
    stt/Chunker.kt
    tile/DictatorTileService.kt
    recog/DictatorRecognitionService.kt
    settings/SetupActivity.kt
```

## IPC

Bound service в `:stt`, AIDL (не Messenger: нужен callback через процессы).

`ISttService`: `register(ISttCallback)`, `start(source)`, `stop()`, `preload()`, `status()`. Источники: `SOURCE_IME=1`, `SOURCE_TILE=2`, `SOURCE_RECOG=3`. Статусы: Idle / Recording / Transcribing.

`ISttCallback`: `onStatus`, `onLevel(float)` (RMS для волны, не текст), `onResult(source, text)`, `onError(message)` — без стека и без путей к файлам.

Правила:

- Один сеанс. Повторный `start` во время Recording — это `stop` (как хоткей-тоггл на Mac).
- Микрофон и ONNX только в `:stt`.
- Для `SOURCE_IME` буфер пишется всегда. Если IME убили во время Transcribing — текст всё равно в буфере + уведомление.
- Клиенты: `bindService` в `onCreate` / `onStartListening`, `unbind` в `onDestroy`.
- `SttService` — `START_NOT_STICKY`. После Idle без биндов останавливается; recognizer можно оставить в памяти процесса до LMK (preload).

Foreground service: тип `microphone`, канал «Dictator»: «Запись…» (действие «Стоп») / «Распознавание…». Сессия из плитки ещё и `startForegroundService`, чтобы пережить закрытие диалога. Без уведомления Android 10+ обрежет запись из плитки.

Плитка неактивна (`STATE_UNAVAILABLE`), пока мастер не подтвердил включённый IME и микрофон.

## IME: минимальные клавиши

Панель ~220–260 dp, тёмная, без QWERTY:

- Переключение клавиатуры: `switchToPreviousInputMethod()`, иначе `switchToNextInputMethod(false)`. Показывать, если `shouldOfferSwitchingToNextInputMethod()`.
- Большой микрофон (старт/стоп)
- Волна + таймер, пока Recording; «…» пока Transcribing
- Backspace, long-press repeat → `deleteSurroundingText`
- Пробел
- Action: `EditorInfo.imeOptions` → Send / Done / Go / Next через `performEditorAction` (в Telegram — отправить)

`method.xml`: один subtype `languageTag="ru-RU"`, `imeSubtypeMode="voice"`, `isAsciiCapable="true"` (поля пароля), `isAuxiliary="false"` (иначе нельзя выбрать Dictator в настройках ввода), `supportsSwitchingToNextInputMethod="true"`, `settingsActivity` на мастер.

Не `isAuxiliary=true`: тогда Dictator нельзя сделать текущей клавиатурой с глобуса. One-shot даёт настройка «вернуться после вставки».

## Модель на диске

Те же четыре файла, что качает [scripts/fetch-stt-model.sh](../scripts/fetch-stt-model.sh): `encoder.int8.onnx`, `decoder.onnx`, `joiner.onnx`, `tokens.txt`. Веса не коммитить.

Упаковка: **assets + mmap, без extract в `filesDir`**.

- CI копирует `desktop/src-tauri/resources/gigaam/` → `android/app/src/main/assets/gigaam/`
- `androidResources.noCompress += listOf("onnx", "txt")`
- `OfflineRecognizer(assetManager, config)` с путями `gigaam/encoder.int8.onnx` и т.д.
- Размер APK ≈ 250 МБ (как zip на Mac)

Конфиг как [`desktop/src-tauri/src/stt.rs`](../desktop/src-tauri/src/stt.rs): `sample_rate=16000`, `feature_dim=64` (не 80), `model_type=nemo_transducer`, `decoding_method=greedy_search`, `provider=cpu`. На телефоне `numThreads=2` (на десктопе 4).

Чанки как desktop: короче 0.35 с — пустой результат; длиннее ~24 с — куски по 20 с с перекрытием 0.4 с, склейка пробелом. `Chunker.kt` — JVM unit-тест, без JNI.

Запись: `AudioRecord`, 16 кГц, mono, PCM16 → float `[-1, 1]` в `acceptWaveform`. Потолок сеанса **180 с**, потом авто-stop. WAV на диск не писать (180 с × 16 кГц × 2 байта ≈ 6 МБ в RAM).

Расширить `fetch-stt-model.sh`: после download копировать в `android/app/src/main/assets/gigaam/`, если каталог `android/` есть. Один SHA-256, два потребителя.

## Manifest (обязательное)

- `RECORD_AUDIO`, `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE`. **Не** `INTERNET`.
- IME: `permission="android.permission.BIND_INPUT_METHOD"`, `exported=true`, meta-data `@xml/method`
- `SttService`: `foregroundServiceType="microphone"`, `process=":stt"`, `exported=false`
- `TileService`: `permission="android.permission.BIND_QUICK_SETTINGS_TILE"`, `exported=true`
- RecognitionService: `BIND_RECOGNITION_SERVICE` + xml `android.speech.RecognitionService`
- `android:extractNativeLibs="false"`; 16 KB page size на Android 15 — если `.so` из AAR не выровнены, обновление sherpa / пересборка native, не `extractNativeLibs=true` «на всякий случай»
- `abiFilters += "arm64-v8a"`

## Настройки (SharedPreferences)

| Ключ | Default | Смысл |
|---|---|---|
| `return_to_previous_ime` | true | После вставки вернуться на Gboard |
| `paste_enabled` | true | false = только буфер, не `commitText` (как «только копировать» на Mac) |
| `preload_model` | true | `SttService.preload()` после мастера |

Max duration в UI v1 не выносить (константа 180 с). Нет аккаунта, аналитики, updater.

## Подпись и CI

Как Mac: sideload, не Play. Стабильный ключ, иначе обновление APK не встанет поверх.

- Committed `android/sideload.jks` + пароль в `android/keystore.properties.example` (ad-hoc по смыслу; кто хочет — пересоберёт своим). Либо GitHub secret `ANDROID_KEYSTORE_BASE64` — тогда секрет обязателен до первого артефакта.
- Workflow `.github/workflows/release-android.yml`: `ubuntu-latest`, JDK 17, `bash scripts/fetch-stt-model.sh`, копия в assets, `./gradlew :app:assembleRelease`, артефакт **`Dictator-android-arm64.apk`**.
- `on: push` теги `android-v*` + `workflow_dispatch`. Тег создаёт отдельный GitHub Release `Android X.Y.Z` (`softprops`, `make_latest: false`). Значок Latest остаётся у десктопа. **Не** `gh release create`.
- Кэш GigaAM как в `release-windows.yml`; Gradle cache.
- Тестовый телефон ставит артефакт Actions, не локальный `assemble` как единственную правду.

| | Mac | Windows | Android |
|---|---|---|---|
| Workflow | `release-macos.yml` | `release-windows.yml` | `release-android.yml` |
| Тег | `v*` | `v*` | `android-v*` |
| Runner | `macos-14` | `windows-2022` | `ubuntu-latest` |
| Артефакт | `Dictator-macos-aarch64.app.zip` | `Dictator-windows-x64.exe` | `Dictator-android-arm64.apk` |

### Цикл итерации

1. Правка на Mac, push ветки.
2. Actions → Release Android → Run workflow (эта ветка). Ждать сборку.
3. Скачать `Dictator-android-arm64.apk`, поставить на телефон, прогнать чеклист.
4. Замечания → снова шаг 1.

## Документация (после рабочего APK)

README, [INSTALL.md](INSTALL.md), [SECURITY.md](../SECURITY.md), [CONTRIBUTING.md](../CONTRIBUTING.md): Android 10+ arm64, голосовая клавиатура (не замена Gboard), Gboard свой mic не отдаёт, неизвестные источники, IME не читает нажатия, нет `INTERNET`.

Это руководство (`docs/ANDROID.md`) после релиза можно свернуть до короткой пометки «сделано» или заменить ссылкой на INSTALL.

## Порядок работ

Чтобы не тащить 221 МБ модели в первый PR с пустым IME. Desktop Windows не блокирует эти шаги, но не смешивать PR.

1. **Каркас** — `android/` Gradle, мастер (интент в input settings + runtime permissions), IME с кнопкой «вставить тест» → `commitText("тест")`. Прогон: Telegram, Chrome. Без sherpa.
2. **`:stt` + AIDL** — запись, уровень волны, stub (`[stub] 1.2s`). IME start/stop. Прогон: микрофон, FGS-уведомление, убийство IME посреди записи.
3. **GigaAM** — AAR, assets, Engine, Chunker. Прогон на **реальном** arm64. Засечь RTF на одном mid-range и одном флаге.
4. **Плитка + RecognitionService** — буфер/тост; `RECOGNIZE_SPEECH` с FlorisBoard или AOSP, не с Gboard.
5. **CI + доки** — когда APK уже ставится с Actions.

## Чеклист на телефоне

Ставить **APK с Actions**.

1. Мастер: включение IME, микрофон, уведомления.
2. Telegram: глобус → Dictator → фраза → текст в поле → возврат на Gboard.
3. Telegram Send с action-клавиши.
4. Chrome URL / WhatsApp / системные Сообщения.
5. Поле пароля не падает (`isAsciiCapable`); «только буфер» не вставляет в поле.
6. Плитка при Gboard: текст в буфере, тост, в поле не лезет сама.
7. Шторка поверх Dictator IME: сессия не рвётся.
8. Свернуть Telegram во время Transcribing: текст не теряется (буфер + тост).
9. Запись > 24 с (чанки) и < 0.35 с (пусто).
10. Первая загрузка модели: состояние «Модель готовится», IME не ANR.
11. Устройство ~6 ГБ RAM: нет OOM в процессе IME (`adb shell dumpsys meminfo`).
12. Android 10 и Android 15 (или 14), только arm64.
13. Нет сети после установки: permission `INTERNET` нет.

## Риски, которые не закрыть бумагой

- RTF GigaAM RNNT на среднем Snapdragon может быть неприемлем. Тогда CTC follow-up, не смена UX.
- 16 KB pages (Pixel 8 / Android 15): `.so` из JitPack AAR могут не пройти установку. Лечится обновлением sherpa.
- Gboard никогда не вызовет Dictator. Писать в INSTALL, не чинить.

## Итог

На Android нет `⌃⇧D`. Текст в мессенджер идёт через голосовую клавиатуру и `commitText`; плитка в шторке — запасной путь в буфер. Распознавание — тот же GigaAM в отдельном процессе. Код пишется на Mac, собирает GitHub, на телефоне только ставится APK. Новый код — каталог `android/`, не вторая копия Tauri.
