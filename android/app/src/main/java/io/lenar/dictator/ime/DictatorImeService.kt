package io.lenar.dictator.ime

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.RemoteException
import android.os.SystemClock
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.Toast
import androidx.core.content.ContextCompat
import io.lenar.dictator.R
import io.lenar.dictator.databinding.ImeViewBinding
import io.lenar.dictator.settings.DictatorPrefs
import io.lenar.dictator.stt.ISttCallback
import io.lenar.dictator.stt.ISttService
import io.lenar.dictator.stt.SttContract
import io.lenar.dictator.stt.SttService

/**
 * Voice panel IME. Mic talks to [SttService] in process `:stt` over AIDL.
 */
class DictatorImeService : InputMethodService() {
    private var binding: ImeViewBinding? = null
    private var editorInfo: EditorInfo? = null
    private var stt: ISttService? = null
    private var bound = false
    private var status: Int = SttContract.STATUS_IDLE
    private var recordStartedAt = 0L

    private val mainHandler = Handler(Looper.getMainLooper())
    private var backspaceRepeating = false

    private val timerTick =
        object : Runnable {
            override fun run() {
                if (status != SttContract.STATUS_RECORDING) return
                val elapsedMs = SystemClock.elapsedRealtime() - recordStartedAt
                val sec = elapsedMs / 1000
                val mm = sec / 60
                val ss = sec % 60
                binding?.txtHint?.text =
                    getString(R.string.ime_status_recording, "%d:%02d".format(mm, ss))
                mainHandler.postDelayed(this, 200)
            }
        }

    private val backspaceRepeat =
        object : Runnable {
            override fun run() {
                if (!backspaceRepeating) return
                currentInputConnection?.deleteSurroundingText(1, 0)
                mainHandler.postDelayed(this, BACKSPACE_REPEAT_MS)
            }
        }

    private val sttCallback =
        object : ISttCallback.Stub() {
            override fun onStatus(newStatus: Int) {
                mainHandler.post { applyStatus(newStatus) }
            }

            override fun onLevel(level: Float) {
                mainHandler.post {
                    binding?.levelBar?.progress = (level * 100f).toInt().coerceIn(0, 100)
                }
            }

            override fun onPartial(source: Int, text: String?) = Unit

            override fun onResult(source: Int, text: String?) {
                mainHandler.post { handleResult(text.orEmpty()) }
            }

            override fun onError(message: String?) {
                mainHandler.post {
                    Toast.makeText(
                        this@DictatorImeService,
                        message ?: "STT error",
                        Toast.LENGTH_SHORT,
                    ).show()
                    applyStatus(SttContract.STATUS_IDLE)
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
                    applyStatus(api.status())
                    if (DictatorPrefs.preloadModel(this@DictatorImeService)) {
                        binding?.txtHint?.text = getString(R.string.ime_status_loading)
                        api.preload()
                    }
                } catch (_: RemoteException) {
                    Toast.makeText(
                        this@DictatorImeService,
                        R.string.ime_stt_unavailable,
                        Toast.LENGTH_SHORT,
                    ).show()
                }
            }

            override fun onServiceDisconnected(name: ComponentName?) {
                stt = null
                bound = false
                applyStatus(SttContract.STATUS_IDLE)
            }
        }

    override fun onCreate() {
        super.onCreate()
        bindStt()
    }

    override fun onCreateInputView(): View {
        val themed = android.view.ContextThemeWrapper(this, R.style.Theme_Dictator)
        val inflated = ImeViewBinding.inflate(layoutInflater.cloneInContext(themed))
        binding = inflated
        bindClicks(inflated)
        updateSwitchVisibility(inflated)
        applyStatus(status)
        return inflated.root
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        editorInfo = info
        if (!bound) bindStt()
        binding?.let {
            updateSwitchVisibility(it)
            updateActionLabel(it, info)
        }
    }

    override fun onDestroy() {
        stopBackspaceRepeat()
        mainHandler.removeCallbacks(timerTick)
        if (bound) {
            try {
                unbindService(connection)
            } catch (_: IllegalArgumentException) {
            }
            bound = false
        }
        stt = null
        binding = null
        super.onDestroy()
    }

    private fun bindStt() {
        val intent = Intent(this, SttService::class.java)
        bindService(intent, connection, Context.BIND_AUTO_CREATE)
    }

    private fun bindClicks(b: ImeViewBinding) {
        b.btnSwitchIme.setOnClickListener { switchAway() }
        b.btnInsertTest.setOnClickListener { insertTest() }
        b.btnMic.setOnClickListener { toggleMic() }
        b.btnSpace.setOnClickListener {
            currentInputConnection?.commitText(" ", 1)
        }
        b.btnAction.setOnClickListener { performEditorAction() }
        b.btnBackspace.setOnClickListener {
            currentInputConnection?.deleteSurroundingText(1, 0)
        }
        b.btnBackspace.setOnLongClickListener {
            startBackspaceRepeat()
            true
        }
        b.btnBackspace.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                android.view.MotionEvent.ACTION_UP,
                android.view.MotionEvent.ACTION_CANCEL,
                -> stopBackspaceRepeat()
            }
            false
        }
    }

    private fun toggleMic() {
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO)
            != PackageManager.PERMISSION_GRANTED
        ) {
            Toast.makeText(this, R.string.ime_need_mic, Toast.LENGTH_SHORT).show()
            return
        }
        val api = stt
        if (api == null) {
            Toast.makeText(this, R.string.ime_stt_unavailable, Toast.LENGTH_SHORT).show()
            bindStt()
            return
        }
        try {
            if (status == SttContract.STATUS_RECORDING) {
                api.stop()
            } else {
                api.start(SttContract.SOURCE_IME, false)
            }
        } catch (_: RemoteException) {
            Toast.makeText(this, R.string.ime_stt_unavailable, Toast.LENGTH_SHORT).show()
        }
    }

    private fun handleResult(text: String) {
        if (text.isNotEmpty() && DictatorPrefs.pasteEnabled(this)) {
            currentInputConnection?.commitText(text, 1)
        }
        if (DictatorPrefs.returnToPreviousIme(this)) {
            switchAway()
        }
    }

    private fun applyStatus(newStatus: Int) {
        status = newStatus
        val b = binding ?: return
        when (newStatus) {
            SttContract.STATUS_RECORDING -> {
                recordStartedAt = SystemClock.elapsedRealtime()
                b.btnMic.text = getString(R.string.ime_mic_recording)
                b.btnMic.isEnabled = true
                mainHandler.removeCallbacks(timerTick)
                mainHandler.post(timerTick)
            }
            SttContract.STATUS_TRANSCRIBING -> {
                mainHandler.removeCallbacks(timerTick)
                b.btnMic.text = getString(R.string.ime_status_transcribing)
                b.btnMic.isEnabled = false
                b.txtHint.text = getString(R.string.ime_status_transcribing)
                b.levelBar.progress = 0
            }
            else -> {
                mainHandler.removeCallbacks(timerTick)
                b.btnMic.text = getString(R.string.ime_mic_idle)
                b.btnMic.isEnabled = true
                b.txtHint.text = getString(R.string.ime_status_idle)
                b.levelBar.progress = 0
            }
        }
    }

    private fun insertTest() {
        val text = TEST_TEXT
        if (DictatorPrefs.pasteEnabled(this)) {
            currentInputConnection?.commitText(text, 1)
        }
        if (DictatorPrefs.returnToPreviousIme(this)) {
            switchAway()
        }
    }

    private fun switchAway() {
        if (!switchToPreviousInputMethod()) {
            switchToNextInputMethod(false)
        }
    }

    private fun performEditorAction() {
        val info = editorInfo ?: return
        val action = info.imeOptions and EditorInfo.IME_MASK_ACTION
        if (action != EditorInfo.IME_ACTION_NONE && action != EditorInfo.IME_ACTION_UNSPECIFIED) {
            currentInputConnection?.performEditorAction(action)
        } else {
            currentInputConnection?.performEditorAction(EditorInfo.IME_ACTION_DONE)
        }
    }

    private fun updateSwitchVisibility(b: ImeViewBinding) {
        b.btnSwitchIme.visibility =
            if (shouldOfferSwitchingToNextInputMethod()) View.VISIBLE else View.GONE
    }

    private fun updateActionLabel(b: ImeViewBinding, info: EditorInfo?) {
        val label =
            when (info?.imeOptions?.and(EditorInfo.IME_MASK_ACTION)) {
                EditorInfo.IME_ACTION_SEND -> "Send"
                EditorInfo.IME_ACTION_GO -> "Go"
                EditorInfo.IME_ACTION_SEARCH -> "Search"
                EditorInfo.IME_ACTION_NEXT -> "Next"
                EditorInfo.IME_ACTION_DONE -> "Done"
                else -> getString(R.string.ime_action_default)
            }
        b.btnAction.text = label
    }

    private fun startBackspaceRepeat() {
        backspaceRepeating = true
        currentInputConnection?.deleteSurroundingText(1, 0)
        mainHandler.postDelayed(backspaceRepeat, BACKSPACE_REPEAT_MS)
    }

    private fun stopBackspaceRepeat() {
        backspaceRepeating = false
        mainHandler.removeCallbacks(backspaceRepeat)
    }

    companion object {
        private const val TEST_TEXT = "тест"
        private const val BACKSPACE_REPEAT_MS = 50L
    }
}
