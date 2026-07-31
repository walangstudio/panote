package com.walangstudio.panote

import android.content.Context
import android.graphics.Rect
import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
    private lateinit var multicastLock: WifiManager.MulticastLock

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("panote_mdns")
        multicastLock.setReferenceCounted(true)
        multicastLock.acquire()
    }

    /// Publishes the on-screen keyboard's height to CSS as `--kb`.
    ///
    /// Measured on device: with the keyboard open, window, visualViewport and
    /// body all still reported the full 915px. `enableEdgeToEdge()` above makes
    /// the window span the whole screen, and Android 15 then ignores
    /// `windowSoftInputMode=adjustResize` and dispatches an inset instead. The
    /// VirtualKeyboard API is absent from this WebView too, so
    /// `env(keyboard-inset-height)` stays 0. This listener is the only channel
    /// left that can tell the web layer the keyboard exists.
    override fun onWebViewCreate(webView: WebView) {
        val root = findViewById<View>(android.R.id.content)
        ViewCompat.setOnApplyWindowInsetsListener(root) { view, insets ->
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
            // Only the part of the keyboard actually covering the WebView. Where
            // adjustResize does take effect - older devices, which is also where
            // Type.ime() reports 0 - the window has already shrunk by that much,
            // and subtracting it again in CSS would move the layout twice.
            val visible = Rect().also { view.getGlobalVisibleRect(it) }
            val covered = (ime - (view.rootView.height - visible.bottom)).coerceAtLeast(0)
            val css = (covered / view.resources.displayMetrics.density).toInt()
            webView.evaluateJavascript(
                "document.documentElement.style.setProperty('--kb','${css}px')",
                null,
            )
            insets
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        if (multicastLock.isHeld) {
            multicastLock.release()
        }
    }
}
