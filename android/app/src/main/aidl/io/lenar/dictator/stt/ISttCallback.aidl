package io.lenar.dictator.stt;

interface ISttCallback {
    void onStatus(int status);
    void onLevel(float level);
    void onPartial(int source, String text);
    void onResult(int source, String text);
    void onError(String message);
}
