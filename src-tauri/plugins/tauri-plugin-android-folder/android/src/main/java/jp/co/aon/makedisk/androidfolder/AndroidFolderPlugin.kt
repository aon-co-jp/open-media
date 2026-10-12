package jp.co.aon.makedisk.androidfolder

import android.app.Activity
import android.content.Intent
import androidx.activity.result.ActivityResult
import app.tauri.Logger
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class AndroidFolderPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun pickOutputTree(invoke: Invoke) {
        try {
            val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
            startActivityForResult(invoke, intent, "pickOutputTreeResult")
        } catch (ex: Exception) {
            val message = ex.message ?: "Failed to open folder picker"
            Logger.error(message)
            invoke.reject(message)
        }
    }

    @ActivityCallback
    fun pickOutputTreeResult(invoke: Invoke, result: ActivityResult) {
        try {
            when (result.resultCode) {
                Activity.RESULT_OK -> {
                    val uri = result.data?.data
                    val ret = JSObject()
                    if (uri != null) {
                        // 選んだフォルダへの書き込み権限をアプリ再起動後も
                        // 使えるよう永続化する(SAFの流儀)。
                        activity.contentResolver.takePersistableUriPermission(
                            uri,
                            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                        )
                        ret.put("uri", uri.toString())
                    } else {
                        ret.put("uri", null)
                    }
                    invoke.resolve(ret)
                }
                Activity.RESULT_CANCELED -> {
                    val ret = JSObject()
                    ret.put("uri", null)
                    invoke.resolve(ret)
                }
                else -> invoke.reject("Failed to pick output folder")
            }
        } catch (ex: Exception) {
            val message = ex.message ?: "Failed to read folder pick result"
            Logger.error(message)
            invoke.reject(message)
        }
    }
}
