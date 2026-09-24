# GigaAM v3 e2e RNNT (sherpa-onnx)

ONNX-файлы сюда не коммитятся. Перед сборкой APK:

```bash
bash scripts/fetch-stt-model.sh
```

Скрипт кладёт веса и в `desktop/.../gigaam/`, и сюда.
Нужны `encoder.int8.onnx`, `decoder.onnx`, `joiner.onnx`, `tokens.txt`.
`LICENSE` (MIT, GigaChat Team) коммитится; ONNX и `tokens.txt` — нет.
