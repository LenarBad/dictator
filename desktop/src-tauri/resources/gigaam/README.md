# GigaAM v3 e2e RNNT (sherpa-onnx)

ONNX-файлы сюда не коммитятся. Скачайте перед `tauri dev` / сборкой `.app`:

```bash
bash scripts/fetch-stt-model.sh
```

Нужны `encoder.int8.onnx`, `decoder.onnx`, `joiner.onnx`, `tokens.txt`.
Источник: [sherpa-onnx asr-models](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)
(`sherpa-onnx-nemo-transducer-punct-giga-am-v3-russian-2025-12-16`).
Веса GigaAM: MIT, [salute-developers/GigaAM](https://github.com/salute-developers/GigaAM).
