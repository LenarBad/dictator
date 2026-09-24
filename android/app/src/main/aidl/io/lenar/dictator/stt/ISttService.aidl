package io.lenar.dictator.stt;

import io.lenar.dictator.stt.ISttCallback;

interface ISttService {
    void register(ISttCallback callback);
    void start(int source, boolean liveChunks);
    void stop();
    void preload();
    int status();
}
