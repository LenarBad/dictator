# Диаризация (sherpa-onnx)

ONNX-файлы сюда не коммитятся. Их скачивает `scripts/fetch-stt-model.sh`
(он вызывает `scripts/fetch-diarize-model.sh`):

```bash
bash scripts/fetch-stt-model.sh
```

Нужны `segmentation.int8.onnx` и `embedding.onnx`. Тексты лицензий в `licenses/`
коммитятся и кладутся в бандл; ONNX — нет.

| Файл | Что это |
|---|---|
| `segmentation.int8.onnx` | pyannote segmentation 3.0, int8 (~1.5 МБ), MIT, Copyright (c) 2022 CNRS |
| `embedding.onnx` | NeMo TitaNet Small (`nemo_en_titanet_small.onnx`, ~38 МБ), Apache-2.0 |

Segmentation: [speaker-segmentation-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models)
(`sherpa-onnx-pyannote-segmentation-3-0`, внутри архива `model.int8.onnx`).

Embedding: [speaker-recongition-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models)
(`nemo_en_titanet_small.onnx`). Карточка: [TitaNet-Small](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/nemo/models/titanet_small).
Рядом с ONNX лицензии нет — в бандл кладётся текст Apache-2.0 NeMo Toolkit 1.19.0.

3D-Speaker и RevAI Reverb сюда не качаются.
