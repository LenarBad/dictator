# План: диаризация спикеров

Это **план**, не руководство по установке. Сейчас Dictator склеивает всех говорящих в одну строку. Цель: по желанию вставлять текст с метками «Спикер 1 / Спикер 2».

Офлайн, без Python, без облака, тот же шерpa-onnx, что уже крутит GigaAM. GigaAM **не** учится различать голоса — рядом ставится второй движок.

Цель первого среза: **desktop (macOS и Windows)** — один `stt` / `pipeline`, тумблер в настройках, по умолчанию выкл. Android — не в этом плане ([ANDROID.md](ANDROID.md)). Windows уже в том же бинаре, что Mac ([WINDOWS.md](WINDOWS.md)); отдельного порта не ждать.

## Статус

| Шаг | Состояние |
|---|---|
| 1. Веса + fetch + лицензии | сделано |
| 2. `Diarizer` в Rust, без UI | сделано |
| 3. Склейка сегментов → текст | сделано |
| 4. Настройка и вставка | сделано |
| 5. Тесты, доки, размер бандла | тесты и доки сделаны; размер zip/NSIS — после сборки CI |

## Что это не делает

- Не заменяет GigaAM. Она по-прежнему пишет текст.
- Не узнаёт имена («Маша»). Только анонимные `Спикер N`. Имена — follow-up: запись образцов (speaker identification).
- Не чинит перебивки: два голоса одновременно останутся кашей.
- **RevAI Reverb** (`reverb-diarization-v1`) не брать: та же роль, что pyannote (нарезка дорожки), но лицензия Rev Non-Production — в продукт нельзя.

Python / `pyannote.audio` / PyTorch не ставить. Примеры sherpa часто на Python — это демо API, не рантайм Dictator.

## Две модели, не три стека

Диаризация в sherpa — два ONNX на CPU (`provider = cpu`), плюс уже лежащий GigaAM.

| Файл | Роль | Откуда |
|---|---|---|
| pyannote `model.int8.onnx` (~1.5 МБ) | Где речь, границы реплик | [speaker-segmentation-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models) → `sherpa-onnx-pyannote-segmentation-3-0` |
| NeMo TitaNet Small (~ один `.onnx`) | Вектор голоса на кусок | [speaker-recongition-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models) → `nemo_en_titanet_small.onnx` |
| GigaAM e2e RNNT | Текст | как сейчас |

`int8` — сжатая pyannote, не другой алгоритм. **TitaNet и 3D-Speaker не включать оба**: это взаимозаменяемые эмбеддинги. В v1 зафиксирован **TitaNet Small** (в бенчмарках sherpa быстрее). Если на русских диалогах путает спикеров — свап на `3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx` без смены API. Это запас по качеству, не по лицензии.

Крейт уже умеет это: `sherpa_onnx::OfflineSpeakerDiarization` (есть в `1.13.8`). Новый crate не нужен.

## Пайплайн

Сейчас: стоп → WAV → нарезка ~20 с **для ASR** → склейка строк → буфер / Cmd+V.

С тумблером вкл. порядок другой: **сначала вся запись, потом GigaAM по кускам**. Лимит ~25 с у RNNT остаётся только на сегмент.

```
WAV 16 kHz mono (как пишет recorder; сессия до часа, с диаризацией целиком)
        │
        ├─ diarization_enabled = false  → Engine::transcribe как сейчас
        │
        └─ true
              │
              ▼
     OfflineSpeakerDiarization::process(samples)
              │  [start, end, speaker_id]…
              ▼
     склеить соседние куски одного спикера
              │
              ▼
     каждый кусок → GigaAM (если >24 с — split_for_asr)
              │
              ▼
     1 спикер  → обычная строка (без префиксов)
     2+        → «Спикер 1: …» / «Спикер 2: …»
              │
              ▼
     paste::deliver_text — Mac/Windows как сейчас (Cmd+V / Ctrl+V)
```

Резать WAV на 20 с **до** диаризации нельзя: кластеризация не увидит, что это те же люди.

Кластеризация v1: `num_clusters = -1`, `threshold = 0.5` (sherpa default). Число собеседников в настройках — follow-up. `min_duration_on = 0.2`, `min_duration_off = 0.15`: пауза 0.5 с склеивала быстрый диалог в одного спикера, а порог 0.4 дробил один голос на несколько. Кусок, который больше чем наполовину лежит внутри более длинного, забирает спикера этого длинного куска. Голос короче 2 с, чей отпечаток близок к более длинной реплике, получает её метку — иначе короткая фраза того же человека становится отдельным «Спикером».

Если диаризация не вернула ни одного сегмента, весь файл идёт в GigaAM без префиксов — короткая фраза не теряется.

## UX

Dictator — диктовка в поле, не протокол совещания. Поэтому:

- Тумблер **«Разделять говорящих»** в блоке «Диктовка», сразу под предзагрузкой. Hint: «В диалоге подпишет, кто говорил. Дольше распознаёт».
- По умолчанию **выкл.** Старый JSON настроек без поля = выкл.
- HUD/статус не менять: по-прежнему «Распознавание».
- Один спикер в записи — текст как сейчас, без «Спикер 1:».
- Пустые сегменты (GigaAM вернул `""`) не печатать.
- Нумерация с 1 в порядке **первого появления**, не внутренний `speaker_00`.

Формат вставки:

```
Спикер 1: Добрый день, давайте начнём.

Спикер 2: Хорошо, я готов.
```

Между репликами — пустая строка. После двоеточия пробел.

## Куда класть код

Не раздувать `stt.rs` бесконечно. Новый модуль, движок STT остаётся про GigaAM.

| Файл | Что |
|---|---|
| `desktop/src-tauri/src/diarize.rs` | `Diarizer`: load, process → `Vec<Segment { start, end, speaker }>`. Stub при `DICTATOR_STT_STUB`. |
| `desktop/src-tauri/src/stt.rs` | Метод `transcribe_segment(&[f32], rate)` или нарезка WAV в памяти. Сейчас читает только файл — не гонять каждый сегмент через диск. |
| `desktop/src-tauri/src/wav.rs` | Срез сэмплов по `[start, end]` в секундах. `split_for_asr` не трогать. |
| `desktop/src-tauri/src/speakers.rs` | Чистая функция: сегменты + тексты → строка. Юнит-тесты без ONNX. |
| `desktop/src-tauri/src/pipeline.rs` | После `ensure_engine`: если флаг — diarize → ASR кусков → `speakers::format`; иначе `transcribe`. |
| `desktop/src-tauri/src/settings.rs` | `diarization_enabled: bool` (`#[serde(default)]`). |
| `desktop/src-tauri/src/lib.rs` | Прокинуть поле в `UiState` / `save_settings`. `model_name` по-прежнему форсить в GigaAM. |
| `desktop/index.html`, `desktop/src/main.ts` | Чекбокс, тип `Settings`. |
| `desktop/src-tauri/src/lib.rs` `mod` | Подключить `diarize`, `speakers`. |

Предзагрузка: если тумблер вкл. и `preload_model` — грузить diarizer вместе с GigaAM. Если пользователь включил тумблер позже — создать diarizer при первой такой записи (как сейчас движок при первом стопе).

Пути к весам: каталог `desktop/src-tauri/resources/diarize/` рядом с `gigaam/`, тот же поиск что у GigaAM (`DICTATOR_MODEL_DIR`, `.app/Contents/Resources`, каталог рядом с `.exe`). Не смешивать ONNX GigaAM и диаризации в одной папке.

## Веса, fetch, бандл

Новый `scripts/fetch-diarize-model.sh`, его зовут `fetch-stt-model.sh` и CI. Отдельный скрипт проще откатить и проще SHA.

Вызов — **до** раннего `exit 0` в `fetch-stt-model.sh`. Если GigaAM уже лежит в `resources/gigaam/`, скрипт сейчас выходит и дочерний fetch не запустится.

Нужны:

- `resources/diarize/segmentation.int8.onnx` (из архива pyannote: `model.int8.onnx`)
- `resources/diarize/embedding.onnx` (`nemo_en_titanet_small.onnx`)
- `resources/diarize/licenses/` — два текста, см. ниже
- `resources/diarize/README.md` — откуда скачали, как GigaAM README

Не коммитить `.onnx`. `.gitignore`: `desktop/src-tauri/resources/diarize/*.onnx`.

`tauri.conf.json` `bundle.resources`: ONNX и оба LICENSE, как у GigaAM. CI уже вызывает `fetch-stt-model.sh` — после расширения Mac/Windows подхватят сами.

### Лицензии (проверено)

Класть в бандл можно. На 3D-Speaker из‑за лицензии не переключаться. TitaNet-Large (Hugging Face, CC-BY-4.0) не брать — это другая модель.

| Веса | Лицензия | Откуда текст |
|---|---|---|
| pyannote segmentation 3.0 | MIT, Copyright 2022 CNRS | `LICENSE` внутри `sherpa-onnx-pyannote-segmentation-3-0.tar.bz2` |
| TitaNet Small | Apache-2.0 | [карточка NGC](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/titanet_small) отсылает к лицензии NeMo Toolkit; NeMo 1.19.0 — Apache-2.0. Рядом с `nemo_en_titanet_small.onnx` файла LICENSE нет, текст кладём сами |

В `THIRD_PARTY_NOTICES.md` — имена, URL и эти лицензии (можно вместе с шагом 1, не ждать Rust).

Rev Reverb в скрипт не добавлять.

## Тесты

Без сети и без обязательных весов на CI разработчика:

- `speakers::format`: 0 сегментов; один спикер без префикса; два спикера; склейка соседей; пустой текст сегмента; порядок первого появления.
- `wav` slice: границы, пустой интервал, выход за длину.
- Stub: `DICTATOR_STT_STUB=1` + `diarization_enabled` не падает (либо один фейковый спикер, либо тот же stub-текст что сейчас).
- Существующий `transcribes_official_example_when_model_present` не ломать. Опционально: если diarize ONNX есть, прогнать WAV из `tests/fixtures` и проверить, что API не `None`.

Фикстуру «два голоса» не синтезировать в v1, если нет короткого файла с ясной сменой. Ручной прогон важнее.

## Документация после кода

- `THIRD_PARTY_NOTICES.md` — pyannote segmentation 3.0, NeMo TitaNet (имена, URL, лицензия).
- `SECURITY.md` — по-прежнему нет сети; два локальных ONNX вместо одного.
- `docs/BUILD.md` / `desktop/README.md` — fetch подтягивает и diarize.
- README продукта: одна строка, что разделение говорящих опционально и выкл. по умолчанию. Не обещать имена.

Этот файл после релиза свернуть до короткой справки, как [WINDOWS.md](WINDOWS.md).

## Порядок работ

Не смешивать с Android-каркасом. Mac и Windows — один desktop-код; отдельной ветки `platform/windows.rs` для этой фичи нет.

1. **Fetch** — лицензии уже прочитаны (см. выше). Зафиксировать SHA-256, скрипт, gitignore, README и `licenses/` в `resources/diarize/`, строки в `tauri.conf.json` и `THIRD_PARTY_NOTICES.md`. Пока без Rust-логики.
2. **`diarize.rs`** — создать из конфига sherpa, `process` → сегменты. Дым: `cargo test` + локальный прогон на любом 16 kHz wav, печать интервалов в stderr за `debug`/временный bin не обязателен — достаточно теста skip-if-missing.
3. **Склейка + STT кусков** — `speakers.rs`, нарезка сэмплов, `transcribe` сегмента без временных файлов. `pipeline` ветка за флагом. Выкл. = байт-в-байт старое поведение.
4. **Настройки** — поле, чекбокс, preload. Высота окна настроек (сейчас 560) — проверить, что ряд влезает.
5. **Доки + ручной прогон** — один голос; два по очереди; тумблер выкл.; запись < 0.35 с; запись > 24 с с двумя людьми.

## Чеклист на desktop

Прогон на Mac (zip) и на Windows (NSIS с Actions). Поведение текста одно и то же; вставка — Cmd+V / Ctrl+V как сейчас.

1. Тумблер выкл., одна фраза → текст без «Спикер», как в текущем релизе.
2. Тумблер вкл., говорит один → снова без префикса.
3. Два человека по очереди у одного микрофона → две подписанные реплики, порядок как в записи.
4. Соседние фразы одного человека не дробятся на «Спикер 1» дважды подряд.
5. Запись ~30–60 с: дождётся, не зависает HUD, вставка одним куском.
6. Первый запуск с тумблером вкл. без предзагрузки: нет паники, ошибка в тосте если ONNX нет.
7. `DICTATOR_STT_STUB=1` по-прежнему отдаёт stub.
8. После установки артефакта с Actions приложение не ходит в сеть (как сейчас).
9. Размер zip/NSIS вырос на порядка единиц–десятков МБ, не на второй GigaAM.

## Риски

- **Порог кластеризации.** На двух похожих голосах будет один спикер; на одном с паузами — ложный второй. v1 живёт с default, не крутилкой в UI.
- **RAM.** Две ONNX-сессии в процессе. На 8 ГБ Mac обычно ок; не грузить diarizer, если тумблер выкл.
- **RTF.** Диаризация + N вызовов GigaAM длиннее одной транскрипции. Сессия до часа; с диаризацией один проход в конце, без неё сегменты распознаются по ходу.
- **Rust-обёртка.** Если `OfflineSpeakerDiarization` в 1.13.8 ведёт себя иначе, чем docs.rs latest — править вызовы, **не** бампать sherpa «заодно» без причины.

## Итог

Фича — опциональная ветка после стопа записи: pyannote int8 режет дорожку, TitaNet клеит голоса, GigaAM пишет текст в каждый кусок. Новый код — `diarize.rs` + `speakers.rs` и тумблер. Python, Reverb и имена людей в v1 не входят.
