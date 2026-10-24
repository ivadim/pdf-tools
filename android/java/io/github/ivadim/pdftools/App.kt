// PDF Tools for Android: every tool runs on the device with the same Rust
// engine as the desktop app; files never leave it.

package io.github.ivadim.pdftools

import android.app.ActivityManager
import android.app.Application
import android.content.Context

class App : Application() {
    override fun onCreate() {
        super.onCreate()
        context = this
        Workspace.clean()
        val memory = ActivityManager.MemoryInfo().also { getSystemService(ActivityManager::class.java).getMemoryInfo(it) }
        Engine.configure(memory.totalMem)
    }

    companion object {
        lateinit var context: Context
            private set
    }
}
