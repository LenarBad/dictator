package io.lenar.dictator.ime

import android.content.ClipData
import android.content.ClipboardManager
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.inputmethod.EditorInfo
import android.widget.Toast
import io.lenar.dictator.R
import io.lenar.dictator.databinding.ImeViewBinding
import io.lenar.dictator.settings.DictatorPrefs

/**
 * Voice panel IME (not QWERTY). Step 1: [commitText] of a fixed test string.
 * STT binds in a later step; the mic button stays a placeholder for now.
 */
class DictatorImeService : InputMethodService() {
    private var binding: ImeViewBinding? = null
    private var editorInfo: EditorInfo? = null

    private val mainHandler = Handler(Looper.getMainLooper())
    private var backspaceRepeating = false

    private val backspaceRepeat =
        object : Runnable {
            override fun run() {
                if (!backspaceRepeating) return
                currentInputConnection?.deleteSurroundingText(1, 0)
                mainHandler.postDelayed(this, BACKSPACE_REPEAT_MS)
            }
        }

    override fun onCreateInputView(): View {
        val themed = android.view.ContextThemeWrapper(this, R.style.Theme_Dictator)
        val inflated = ImeViewBinding.inflate(layoutInflater.cloneInContext(themed))
        binding = inflated
        bindClicks(inflated)
        updateSwitchVisibility(inflated)
        return inflated.root
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        editorInfo = info
        binding?.let {
            updateSwitchVisibility(it)
            updateActionLabel(it, info)
        }
    }

    override fun onDestroy() {
        stopBackspaceRepeat()
        binding = null
        super.onDestroy()
    }

    private fun bindClicks(b: ImeViewBinding) {
        b.btnSwitchIme.setOnClickListener { switchAway() }
        b.btnInsertTest.setOnClickListener { insertTest() }
        b.btnMic.setOnClickListener {
            Toast.makeText(this, R.string.ime_mic_soon, Toast.LENGTH_SHORT).show()
        }
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

    private fun insertTest() {
        val text = TEST_TEXT
        val clipboard = getSystemService(ClipboardManager::class.java)
        clipboard?.setPrimaryClip(ClipData.newPlainText("dictator", text))

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
