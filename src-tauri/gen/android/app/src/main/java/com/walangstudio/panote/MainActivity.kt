package com.walangstudio.panote

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

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

    override fun onDestroy() {
        super.onDestroy()
        if (multicastLock.isHeld) {
            multicastLock.release()
        }
    }
}
