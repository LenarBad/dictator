package io.lenar.dictator.tile

import android.content.Intent
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import io.lenar.dictator.R
import io.lenar.dictator.settings.SetupActivity
import io.lenar.dictator.settings.SetupGate

/**
 * Quick Settings entry point. Does not start the microphone FGS itself —
 * Android 14+ rejects mic FGS from a tile-only background context.
 * Opens [TileSessionActivity], which is a visible foreground UI.
 */
class DictatorTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        refreshTile()
    }

    override fun onClick() {
        super.onClick()
        if (!SetupGate.isVoiceReady(this)) {
            Toast.makeText(this, R.string.tile_setup_required, Toast.LENGTH_SHORT).show()
            val setup = Intent(this, SetupActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            @Suppress("DEPRECATION")
            startActivityAndCollapse(setup)
            return
        }

        val session =
            Intent(this, TileSessionActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val launch = {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(session)
        }
        if (isLocked) {
            unlockAndRun(launch)
        } else {
            launch()
        }
    }

    private fun refreshTile() {
        val tile = qsTile ?: return
        if (!SetupGate.isVoiceReady(this)) {
            tile.state = Tile.STATE_UNAVAILABLE
            tile.label = getString(R.string.tile_label)
            tile.subtitle = getString(R.string.tile_subtitle_setup)
        } else {
            tile.state = Tile.STATE_INACTIVE
            tile.label = getString(R.string.tile_label)
            tile.subtitle = getString(R.string.tile_subtitle_idle)
        }
        tile.updateTile()
    }
}
