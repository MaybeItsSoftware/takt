package uk.co.maybeitsadam.takt

import android.app.Application
import android.content.Context
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.wear.WearBridge

/** Owns the [AppContainer]; the workspace opens off the main thread as the process starts. */
class TaktApplication : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        // The core asks which device it is in before working out what holds,
        // so a condition marked "phone" or "tablet" holds here. It is the
        // device's, not the account's, so it is said in every process start,
        // before the container opens the workspace and anything reads it.
        // A tablet is told by its smallest width, as Android's own layouts are.
        uniffi.takt_core.setThisDeviceKind(
            if (resources.configuration.smallestScreenWidthDp >= TABLET_SMALLEST_WIDTH_DP) "tablet" else "phone",
        )
        container = AppContainer(this)
        WearBridge.attach(container)
    }
}

/** From this smallest width on, the device is a tablet: the sw600dp line. */
private const val TABLET_SMALLEST_WIDTH_DP = 600

/** The process's container, from any context. */
val Context.appContainer: AppContainer get() = (applicationContext as TaktApplication).container
