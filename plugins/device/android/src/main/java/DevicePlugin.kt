package org.dupelens.device

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.Settings
import android.view.ViewGroup
import android.webkit.WebView
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

/**
 * The Android-only system calls of DupeLens (docs/decisions/0016-android-app.md).
 * The Rust side is plugins/device/src/lib.rs; the page never calls this directly.
 */
@TauriPlugin
class DevicePlugin(private val activity: Activity) : Plugin(activity) {

    /**
     * Android 15 and newer draw an app edge to edge: behind the status bar, the camera
     * cut-out and the navigation bar. A web page has no idea, so its top bar would sit under
     * the clock. Keep the web view inside the free area instead (and above the keyboard).
     */
    override fun load(webView: WebView) {
        super.load(webView)
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            val free = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime()
            )
            (view.layoutParams as? ViewGroup.MarginLayoutParams)?.let { margins ->
                margins.setMargins(free.left, free.top, free.right, free.bottom)
                view.layoutParams = margins
            }
            WindowInsetsCompat.CONSUMED
        }
        ViewCompat.requestApplyInsets(webView)
    }

    private fun hasStorageAccess(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            ContextCompat.checkSelfPermission(activity, Manifest.permission.READ_EXTERNAL_STORAGE) ==
                PackageManager.PERMISSION_GRANTED
        }

    /** { supported: true, granted: <can this app read all files now?> } */
    @Command
    fun storageStatus(invoke: Invoke) {
        val result = JSObject()
        result.put("supported", true)
        result.put("granted", hasStorageAccess())
        invoke.resolve(result)
    }

    /**
     * Opens the settings page where the person allows access. Android 11+ has a page just
     * for "all files access"; older versions only have the app's general settings page,
     * where the Storage permission can be switched on.
     */
    @Command
    fun openStorageSettings(invoke: Invoke) {
        val appUri = Uri.parse("package:" + activity.packageName)
        val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, appUri)
        } else {
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, appUri)
        }
        try {
            activity.startActivity(intent)
            invoke.resolve()
        } catch (first: Exception) {
            // Some phones lack the per-app page; fall back to the list of all apps.
            try {
                activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
                invoke.resolve()
            } catch (second: Exception) {
                invoke.reject("Could not open Android's settings: " + (second.message ?: "unknown error"))
            }
        }
    }
}
