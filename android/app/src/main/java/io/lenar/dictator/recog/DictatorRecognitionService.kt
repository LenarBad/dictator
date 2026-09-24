package io.lenar.dictator.recog

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteException
import android.speech.RecognitionService
import android.speech.SpeechRecognizer
import io.lenar.dictator.settings.SetupGate
import io.lenar.dictator.stt.ISttCallback
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract
import io.lenar.dictator.stt.SttService

/**
 * Offline [RecognitionService] for keyboards that expose a third-party mic
 * (FlorisBoard / HeliBoard / AOSP). Gboard and Samsung Keyboard do not call this.
 */
class DictatorRecognitionService : RecognitionService() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var stt: ISttService? = null
    private var bound = false
    private var activeCallback: Callback? = null
    private var cancelled = false
    private var awaitingResult = false

    private val sttCallback =
        object : ISttCallback.Stub() {
            override fun onStatus(status: Int) {
                if (status == SttContract.STATUS_RECORDING) {
                    mainHandler.post {
                        try {
                            activeCallback?.beginningOfSpeech()
                        } catch (_: Exception) {
                        }
                    }
                }
            }

            override fun onPartial(source: Int, text: String?) = Unit

            override fun onLevel(level: Float) {
                mainHandler.post {
                    try {
                        activeCallback?.rmsChanged(20f * level)
                    } catch (_: Exception) {
                    }
                }
            }

            override fun onResult(source: Int, text: String?) {
                if (source != SttContract.SOURCE_RECOG) return
                mainHandler.post { deliverResult(text.orEmpty()) }
            }

            override fun onError(message: String?) {
                mainHandler.post {
                    fail(SpeechRecognizer.ERROR_CLIENT)
                }
            }
        }

    private val connection =
        object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
                val api = ISttService.Stub.asInterface(service)
                stt = api
                bound = true
                try {
                    api.register(sttCallback)
                    if (awaitingResult && activeCallback != null && !cancelled) {
                        api.start(SttContract.SOURCE_RECOG, false)
                    }
                } catch (_: RemoteException) {
                    fail(SpeechRecognizer.ERROR_CLIENT)
                }
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                stt = null
                bound = false
            }
        }

    override fun onStartListening(recognizerIntent: Intent, callback: Callback) {
        if (!SetupGate.isVoiceReady(this)) {
            try {
                callback.error(SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS)
            } catch (_: Exception) {
            }
            return
        }
        cancelled = false
        awaitingResult = true
        activeCallback = callback
        if (bound) {
            try {
                stt?.start(SttContract.SOURCE_RECOG, false)
            } catch (_: RemoteException) {
                fail(SpeechRecognizer.ERROR_CLIENT)
            }
        } else {
            bindService(Intent(this, SttService::class.java), connection, Context.BIND_AUTO_CREATE)
        }
    }

    override fun onCancel(callback: Callback) {
        cancelled = true
        awaitingResult = false
        try {
            stt?.stop()
        } catch (_: RemoteException) {
        }
        try {
            callback.error(SpeechRecognizer.ERROR_CLIENT)
        } catch (_: Exception) {
        }
        activeCallback = null
    }

    override fun onStopListening(callback: Callback) {
        activeCallback = callback
        try {
            stt?.stop()
        } catch (_: RemoteException) {
            fail(SpeechRecognizer.ERROR_CLIENT)
        }
    }

    override fun onDestroy() {
        if (bound) {
            try {
                unbindService(connection)
            } catch (_: IllegalArgumentException) {
            }
            bound = false
        }
        stt = null
        activeCallback = null
        super.onDestroy()
    }

    private fun deliverResult(text: String) {
        val cb = activeCallback
        activeCallback = null
        awaitingResult = false
        if (cancelled || cb == null) return
        try {
            if (text.isEmpty()) {
                cb.error(SpeechRecognizer.ERROR_NO_MATCH)
                return
            }
            val bundle = Bundle()
            bundle.putStringArrayList(
                SpeechRecognizer.RESULTS_RECOGNITION,
                arrayListOf(text),
            )
            cb.results(bundle)
        } catch (_: Exception) {
        }
    }

    private fun fail(code: Int) {
        val cb = activeCallback
        activeCallback = null
        awaitingResult = false
        if (cb == null) return
        try {
            cb.error(code)
        } catch (_: Exception) {
        }
    }
}
