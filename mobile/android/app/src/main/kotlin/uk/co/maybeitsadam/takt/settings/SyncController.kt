package uk.co.maybeitsadam.takt.settings

import android.content.Context
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import java.time.Instant
import java.time.OffsetDateTime
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uk.co.maybeitsadam.takt.BuildConfig
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.appContainer
import uk.co.maybeitsadam.takt.core.SyncAccountInfo
import uk.co.maybeitsadam.takt.core.SyncCredentials
import uk.co.maybeitsadam.takt.core.SyncEndpoints
import uk.co.maybeitsadam.takt.data.sync.SyncEndpointCheck
import uk.co.maybeitsadam.takt.data.sync.SyncEndpointProblem
import uk.co.maybeitsadam.takt.data.sync.OkHttpSyncTransport
import uk.co.maybeitsadam.takt.data.sync.SyncEngine
import uk.co.maybeitsadam.takt.data.sync.SyncException
import uk.co.maybeitsadam.takt.data.sync.SyncLocalState
import uk.co.maybeitsadam.takt.data.sync.SyncRealtime
import uk.co.maybeitsadam.takt.data.sync.SyncScheduler
import uk.co.maybeitsadam.takt.data.sync.SyncStore

/** Sync as Settings shows it. Port of `SyncSession.Phase`. */
sealed interface SyncUiState {
    /** Never signed in, or signed out on purpose. */
    data object Unpaired : SyncUiState

    /** The session ended without the user (signed out elsewhere, the account deleted, or an update from before Supabase). */
    data object SessionExpired : SyncUiState
    data class Idle(val lastSyncedAt: Instant?) : SyncUiState
    data object Syncing : SyncUiState
    data class Failed(val message: String) : SyncUiState
}

/** What the signed-out form prefills, and whether the device was signed out for the user rather than by them. */
data class SignedOutHint(val email: String?, val serverURL: String?, val expired: Boolean)

/**
 * Sync for the app: the account (a Supabase Auth session, sealed with a
 * Keystore key), signing in, the rhythm — a long-poll while the app is in
 * front, a cycle two seconds after a local write, and a 15-minute WorkManager
 * job in the background — and the status Settings draws. Port of
 * `SyncSession.swift`.
 *
 * The repository's flows re-query when a pull writes, so nothing here has to
 * tell the screens to reload.
 */
class SyncController(private val container: AppContainer) {
    private class Active(val credentials: SyncCredentials, val engine: SyncEngine, val scheduler: SyncScheduler, val scope: CoroutineScope)

    private val credentialStore by lazy { SyncCredentialStore(container.context) }
    private val _endpoints = MutableStateFlow(hostedEndpoints)

    /** The sync server and Supabase project in use: Takt's, or a self-hosted pair from [useEndpoints]. */
    val endpoints: StateFlow<SyncEndpoints> = _endpoints.asStateFlow()

    @Volatile private var currentAccounts: SupabaseAccounts? = null

    /** The Supabase client for [endpoints]' project, made again when [useEndpoints] changes it. */
    private val accounts: SupabaseAccounts
        get() = currentAccounts ?: synchronized(this) {
            currentAccounts ?: SupabaseAccounts(credentialStore, _endpoints.value).also { currentAccounts = it }
        }
    private val lock = Mutex()
    private val ready = CompletableDeferred<Unit>()
    private val active = MutableStateFlow<Active?>(null)

    @Volatile private var inForeground = false

    private val _credentials = MutableStateFlow<SyncCredentials?>(null)
    val credentials: StateFlow<SyncCredentials?> = _credentials.asStateFlow()

    private val _isSigningIn = MutableStateFlow(false)
    val isSigningIn: StateFlow<Boolean> = _isSigningIn.asStateFlow()

    private val _signInError = MutableStateFlow<String?>(null)
    val signInError: StateFlow<String?> = _signInError.asStateFlow()

    private val _isRequestingReset = MutableStateFlow(false)
    val isRequestingReset: StateFlow<Boolean> = _isRequestingReset.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)

    /** "Check your email…" after making an account or asking for a reset link; cleared by any edit to the form. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    private val _choosingPassword = MutableStateFlow(false)

    /** Set once a password-reset link has signed this device in: Settings asks for the new password. */
    val choosingPassword: StateFlow<Boolean> = _choosingPassword.asStateFlow()

    private val _signedOut = MutableStateFlow<SignedOutHint?>(null)

    /** Set while signed out: the last email and server, and whether the device was signed out for the user. */
    val signedOut: StateFlow<SignedOutHint?> = _signedOut.asStateFlow()

    private val _account = MutableStateFlow<SyncAccountInfo?>(null)

    /** The account and its devices, as last fetched. Null until [refreshAccount] succeeds. */
    val account: StateFlow<SyncAccountInfo?> = _account.asStateFlow()

    private val storedState = container.withSession { SyncStore(it.repository.database).observeSyncState() }
        .onStart { emit(null) }

    /** Signed out, session expired, idle (with when it last synced), syncing, or failed. */
    val status: StateFlow<SyncUiState> = combine(
        _credentials,
        active.flatMapLatest { it?.engine?.status ?: flowOf(null) },
        storedState,
        _signedOut,
    ) { credentials, engine, stored, hint -> describe(credentials, engine, stored, hint?.expired == true) }
        .stateIn(container.scope, SharingStarted.WhileSubscribed(5_000), SyncUiState.Unpaired)

    /** Called once from the container's init, on the main thread. */
    fun attach() {
        runCatching {
            ProcessLifecycleOwner.get().lifecycle.addObserver(
                LifecycleEventObserver { _, event ->
                    when (event) {
                        Lifecycle.Event.ON_START -> {
                            inForeground = true
                            active.value?.scheduler?.start()
                        }
                        Lifecycle.Event.ON_STOP -> {
                            inForeground = false
                            active.value?.scheduler?.stop()
                        }
                        else -> Unit
                    }
                },
            )
        }.onFailure { Log.w(TAG, "No process lifecycle; sync runs only on demand", it) }
        container.scope.launch {
            try {
                val session = container.awaitSession()
                val store = SyncStore(session.repository.database)
                migrateFromDeviceTokens(store)
                _endpoints.value = withContext(Dispatchers.IO) { savedEndpoints() }
                accounts.ready()
                val saved = withContext(Dispatchers.IO) { runCatching { credentialStore.load() }.getOrNull() }
                val state = store.syncState()
                if (saved != null && state != null && accounts.session() != null) {
                    lock.withLock { activate(saved) }
                } else {
                    if (saved != null) {
                        // Half a sign-in: the database forgot it (restored, or reset), or the
                        // session can't be opened (the Keystore key went with the lock screen).
                        if (state != null) store.endSync()
                        accounts.forget()
                        withContext(Dispatchers.IO) {
                            credentialStore.clear()
                            credentialStore.saveSignedOut(SignedOutHint(saved.email, saved.serverURL, expired = true))
                        }
                    }
                    _signedOut.value = withContext(Dispatchers.IO) { runCatching { credentialStore.loadSignedOut() }.getOrNull() }
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                Log.w(TAG, "Couldn't restore the sync session", error)
            } finally {
                ready.complete(Unit)
            }
        }
    }

    /**
     * A build from before Supabase kept a token the server no longer takes.
     * Keep its device id, drop the token, and ask for a sign-in.
     */
    private suspend fun migrateFromDeviceTokens(store: SyncStore) {
        val legacy = withContext(Dispatchers.IO) { runCatching { credentialStore.takeLegacy() }.getOrNull() } ?: return
        withContext(Dispatchers.IO) {
            credentialStore.deviceId(adopting = legacy.deviceId)
            credentialStore.saveSignedOut(SignedOutHint(legacy.email, legacy.serverURL, expired = true))
        }
        store.endSync()
    }

    /**
     * The endpoints kept from last time. A device that chose a server of its
     * own before a Supabase project could be chosen too kept only the server,
     * with Takt's accounts.
     */
    private fun savedEndpoints(): SyncEndpoints {
        runCatching { credentialStore.endpoints }.getOrNull()?.let { return it }
        val server = runCatching { credentialStore.load()?.serverURL ?: credentialStore.loadSignedOut()?.serverURL }.getOrNull()
        return if (server.isNullOrBlank()) hostedEndpoints else hostedEndpoints.copy(serverURL = server)
    }

    /**
     * Switches to the endpoints typed under "Use a different server" (blank
     * fields are Takt's) and answers the server to sign in to, or null with
     * the reason in [signInError]. A self-hosted pair is checked first
     * ([SyncEndpointCheck]). If they differ from the ones in use, this device
     * signs out cleanly — off the old server's device list, out of the old
     * Supabase project, the outbox dropped — before they are saved. The
     * workspace stays as it is.
     */
    suspend fun useEndpoints(server: String, supabaseURL: String, supabaseKey: String): String? {
        ready.await()
        val chosen = resolveEndpoints(server, supabaseURL, supabaseKey) ?: return null
        val current = _endpoints.value
        if (chosen == current) return chosen.serverURL
        _isSigningIn.value = true
        try {
            if (chosen != hostedEndpoints) SyncEndpointCheck().check(chosen)
            val credentials = _credentials.value
            if (credentials != null) {
                runCatching { transport(credentials).signOut() }.onFailure { if (it is CancellationException) throw it }
                accounts.signOut()
                tearDown(SignedOutHint(credentials.email, chosen.serverURL, expired = false))
            } else {
                accounts.forget()
            }
            withContext(Dispatchers.IO) {
                credentialStore.endpoints = chosen.takeIf { it != hostedEndpoints }
                credentialStore.pendingServer = null
                credentialStore.recoveryPending = false
            }
            _endpoints.value = chosen
            if (!chosen.usesAccountsOf(current)) {
                val fresh = SupabaseAccounts(credentialStore, chosen)
                synchronized(this) { currentAccounts = fresh }
                fresh.ready()
            }
            return chosen.serverURL
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _signInError.value = message(error, "Couldn't switch servers.")
            return null
        } finally {
            _isSigningIn.value = false
        }
    }

    /** Asks the typed server and Supabase project whether they answer, without switching to them. */
    suspend fun checkEndpoints(server: String, supabaseURL: String, supabaseKey: String) {
        val chosen = resolveEndpoints(server, supabaseURL, supabaseKey) ?: return
        _isSigningIn.value = true
        try {
            if (chosen != hostedEndpoints) SyncEndpointCheck().check(chosen)
            _notice.value = "Both answered. Sign in to use them."
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _signInError.value = message(error, "Couldn't check the server.")
        } finally {
            _isSigningIn.value = false
        }
    }

    private fun resolveEndpoints(server: String, supabaseURL: String, supabaseKey: String): SyncEndpoints? {
        _signInError.value = null
        _notice.value = null
        return try {
            SyncEndpoints.resolve(server, supabaseURL, supabaseKey, hostedEndpoints)
        } catch (invalid: IllegalArgumentException) {
            _signInError.value = invalid.message
            null
        }
    }

    /** One cycle, for the background worker and "Sync now". True when it succeeded, or there was nothing to do. */
    suspend fun syncOnce(): Boolean {
        ready.await()
        val current = active.value ?: return true
        return current.scheduler.syncNow()
    }

    fun syncNow() {
        container.scope.launch {
            if (!syncOnce()) (status.value as? SyncUiState.Failed)?.let { container.undo.say("Couldn't sync: ${it.message}") }
        }
    }

    /** Signs in to an existing account with its email and password. */
    suspend fun signIn(serverURL: String, email: String, password: String): Boolean {
        val server = checkedServer(serverURL, email, password) ?: return false
        return signInWith(server) {
            accounts.signIn(email.trim(), password)
            true
        }
    }

    /**
     * Makes an account. Signs in straight away when Supabase allows it;
     * otherwise says to confirm the email, whose link signs this device in.
     */
    suspend fun signUp(serverURL: String, email: String, password: String): Boolean {
        val server = checkedServer(serverURL, email, password) ?: return false
        withContext(Dispatchers.IO) { credentialStore.pendingServer = server }
        return signInWith(server) {
            val signedIn = accounts.signUp(email.trim(), password)
            if (!signedIn) _notice.value = confirmEmailNotice(email.trim())
            signedIn
        }
    }

    /** Sign in with Google: Credential Manager's picker over [activity], then Supabase. */
    suspend fun signInWithGoogle(activity: Context, serverURL: String): Boolean {
        val server = checkedServer(serverURL) ?: return false
        return signInWith(server) {
            val token = GoogleSignIn.request(activity) ?: return@signInWith false
            accounts.signInWithGoogle(token.idToken, token.rawNonce)
            true
        }
    }

    /** Sign in with Apple: opens Supabase in a Custom Tab, and finishes in [handleAuthRedirect]. */
    suspend fun signInWithApple(serverURL: String): Boolean {
        val server = checkedServer(serverURL) ?: return false
        clearSignInError()
        return try {
            withContext(Dispatchers.IO) {
                credentialStore.pendingServer = server
                credentialStore.recoveryPending = false
            }
            accounts.startApple()
            true
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _signInError.value = message(error, "Couldn't open Sign in with Apple.")
            false
        }
    }

    /**
     * The browser came back to `takt://auth-callback` (Apple, a confirmed
     * email, or a password-reset link): exchange its code and sign in.
     */
    fun handleAuthRedirect(uri: Uri) {
        container.scope.launch {
            ready.await()
            val (server, recovering) = withContext(Dispatchers.IO) {
                (credentialStore.pendingServer ?: _endpoints.value.serverURL) to credentialStore.recoveryPending
            }
            val signedIn = signInWith(server) {
                accounts.completeRedirect(uri)
                true
            }
            withContext(Dispatchers.IO) {
                credentialStore.pendingServer = null
                credentialStore.recoveryPending = false
            }
            if (signedIn) {
                if (recovering) _choosingPassword.value = true
                container.undo.say(_credentials.value?.email?.let { "Signed in as $it" } ?: "Signed in")
            } else {
                _signInError.value?.let { container.undo.say("Couldn't sign in: $it") }
            }
        }
    }

    private fun checkedServer(serverURL: String, email: String? = null, password: String? = null): String? {
        val server = serverURL.trim().ifEmpty { _endpoints.value.serverURL }
        _signInError.value = when {
            !isHttpURL(server) -> "Enter the server's full address, starting with https://."
            email != null && email.isBlank() -> "Enter your email."
            password != null && password.isEmpty() -> "Enter your password."
            else -> return server
        }
        return null
    }

    /**
     * Emails a link that signs this device in to choose a new password. Supabase
     * answers the same whether or not the account exists, so the notice says "if".
     * Errors land in [signInError].
     */
    suspend fun requestPasswordReset(serverURL: String, email: String): Boolean {
        val address = email.trim()
        _notice.value = null
        if (address.isEmpty()) {
            _signInError.value = "Enter your email first."
            return false
        }
        val server = checkedServer(serverURL) ?: return false
        _isRequestingReset.value = true
        return try {
            withContext(Dispatchers.IO) {
                credentialStore.pendingServer = server
                credentialStore.recoveryPending = true
            }
            accounts.sendPasswordReset(address)
            _notice.value = passwordResetNotice(address)
            true
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _signInError.value = message(error, "Couldn't send a reset link.")
            false
        } finally {
            _isRequestingReset.value = false
        }
    }

    /** Sets the signed-in account's password. Returns the error to show, or null. */
    suspend fun setPassword(password: String): String? {
        if (password.isEmpty()) return "Enter a new password."
        return try {
            accounts.setPassword(password)
            _choosingPassword.value = false
            container.undo.say("Password changed.")
            null
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            message(error, "Couldn't change the password.")
        }
    }

    fun dismissChoosingPassword() {
        _choosingPassword.value = false
    }

    fun clearSignInError() {
        _signInError.value = null
        _notice.value = null
    }

    /**
     * Runs [authenticate] (which signs in to Supabase, or answers false when
     * there's nothing more to do yet), then records this device on the
     * server and starts syncing.
     */
    private suspend fun signInWith(server: String, authenticate: suspend () -> Boolean): Boolean {
        _isSigningIn.value = true
        _signInError.value = null
        _notice.value = null
        return try {
            if (!authenticate()) return false
            connect(server)
            true
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            _signInError.value = message(error, "Couldn't sign in.")
            false
        } finally {
            _isSigningIn.value = false
        }
    }

    private suspend fun connect(server: String) {
        val session = accounts.session() ?: throw SyncException.Unauthorized()
        val deviceId = withContext(Dispatchers.IO) { credentialStore.deviceId() }
        val credentials = SyncCredentials(server, deviceId, session.email, session.userId)
        try {
            transport(credentials).registerDevice(deviceName(container.context))
        } catch (error: Exception) {
            // Signed in to Supabase but not to sync is no state to leave the app in.
            accounts.forget()
            throw error
        }
        lock.withLock {
            deactivate()
            SyncStore(container.repository().database).beginSync(deviceId, server)
            withContext(Dispatchers.IO) {
                credentialStore.save(credentials)
                credentialStore.clearSignedOut()
            }
            _signedOut.value = null
            activate(credentials)
        }
        container.scope.launch { refreshAccount() }
    }

    private fun transport(credentials: SyncCredentials) =
        OkHttpSyncTransport(credentials.serverURL, credentials.deviceId, accounts.tokens)

    /** Fetches the account and its devices. Returns the error to show, or null. */
    suspend fun refreshAccount(): String? {
        val current = active.value ?: return null
        return try {
            _account.value = transport(current.credentials).account()
            null
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: SyncException.Unauthorized) {
            expire(current.credentials)
            null
        } catch (error: Exception) {
            message(error, "Couldn't load the account.")
        }
    }

    /** Signs out of the account. The workspace stays as it is on this device. */
    fun signOut() {
        container.scope.launch {
            val credentials = _credentials.value ?: return@launch
            // Best effort, before the session goes: an unreachable server
            // mustn't keep the device signed in.
            runCatching { transport(credentials).signOut() }
                .onFailure { if (it is CancellationException) throw it }
            accounts.signOut()
            tearDown(SignedOutHint(credentials.email, credentials.serverURL, expired = false))
            container.undo.say("Signed out. Your tasks stay on this device.")
        }
    }

    /**
     * Deletes the account, everything it synced and every device on it. Each
     * device keeps its own copy of the workspace. Returns the error to show,
     * or null once it is gone.
     */
    suspend fun deleteAccount(): String? {
        val credentials = _credentials.value ?: return "This device is not signed in."
        return try {
            transport(credentials).deleteAccount()
            accounts.forget()
            tearDown(SignedOutHint(email = null, serverURL = credentials.serverURL, expired = false))
            container.undo.say("Account deleted. Your tasks stay on this device.")
            null
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (_: SyncException.Unauthorized) {
            expire(credentials)
            null
        } catch (error: Exception) {
            message(error, "Couldn't delete the account.")
        }
    }

    /** Supabase won't refresh the session any more: stop, and ask for a sign-in. */
    private suspend fun expire(credentials: SyncCredentials) {
        if (_credentials.value != credentials) return
        accounts.forget()
        tearDown(SignedOutHint(credentials.email, credentials.serverURL, expired = true))
        container.undo.say("Signed out of sync. Sign in again to keep syncing.")
    }

    private suspend fun tearDown(hint: SignedOutHint) {
        lock.withLock {
            deactivate()
            SyncStore(container.repository().database).endSync()
            withContext(Dispatchers.IO) {
                credentialStore.clear()
                credentialStore.saveSignedOut(hint)
            }
            _signedOut.value = hint
            _account.value = null
            _signInError.value = null
            _choosingPassword.value = false
            WorkManager.getInstance(container.context).cancelUniqueWork(WORK_NAME)
            WorkManager.getInstance(container.context).cancelUniqueWork(LEGACY_WORK_NAME)
        }
    }

    private suspend fun activate(credentials: SyncCredentials) {
        val database = container.repository().database
        val store = SyncStore(database)
        val scope = CoroutineScope(SupervisorJob(container.scope.coroutineContext[Job]) + Dispatchers.Default)
        val engine = SyncEngine(store, transport(credentials), credentials.deviceId)
        val endpoints = _endpoints.value
        val realtime = credentials.accountId?.let { account ->
            SyncRealtime(credentials.serverURL, endpoints.supabaseURL, endpoints.supabaseKey, account, accounts.tokens)
        }
        val scheduler = SyncScheduler(engine, scope, realtime = realtime)
        scheduler.watchLocalWrites(store)
        active.value = Active(credentials, engine, scheduler, scope)
        _credentials.value = credentials
        // A refresh Supabase refused: tear down from outside this scope, which the teardown cancels.
        scope.launch {
            engine.status.first { it == SyncEngine.Status.SignedOut }
            container.scope.launch { expire(credentials) }
        }
        if (inForeground) scheduler.start() else scope.launch { scheduler.syncNow() }
        schedulePeriodicWork(container.context)
    }

    private fun deactivate() {
        val current = active.value ?: return
        current.scheduler.stop()
        current.scope.cancel()
        active.value = null
        _credentials.value = null
    }

    companion object {
        private const val TAG = "TaktSync"
        const val WORK_NAME = "takt.sync.periodic"

        // Work enqueued before the package was renamed names a worker class that no
        // longer exists, and KEEP would leave it in place, so it is cancelled by name.
        private const val LEGACY_WORK_NAME = "priority.sync.periodic"

        /** The server new accounts go to, unless Settings is told another. */
        val defaultServer: String get() = BuildConfig.SYNC_SERVER

        /** Takt's own server and Supabase project, as built (gradle properties can point a build elsewhere). */
        val hostedEndpoints: SyncEndpoints
            get() = SyncEndpoints(BuildConfig.SYNC_SERVER, BuildConfig.SUPABASE_URL, BuildConfig.SUPABASE_PUBLISHABLE_KEY)

        fun describe(credentials: SyncCredentials?, engine: SyncEngine.Status?, stored: SyncLocalState?, expired: Boolean = false): SyncUiState = when {
            credentials == null -> if (expired) SyncUiState.SessionExpired else SyncUiState.Unpaired
            engine is SyncEngine.Status.SignedOut -> SyncUiState.SessionExpired
            engine is SyncEngine.Status.Syncing -> SyncUiState.Syncing
            engine is SyncEngine.Status.Failed -> SyncUiState.Failed(engine.message)
            engine is SyncEngine.Status.Synced -> SyncUiState.Idle(engine.at)
            else -> SyncUiState.Idle(stored?.lastSyncedAt)
        }

        /** The server's or Supabase's own words for a failure, or a plain fallback for anything else. */
        fun message(error: Throwable, fallback: String): String = SupabaseAccounts.message(error) ?: when (error) {
            is SyncException -> error.message ?: fallback
            is SyncEndpointProblem -> error.message ?: fallback
            is AccountException -> error.message ?: fallback
            is java.io.IOException -> "Couldn't connect. Check the connection and the server address."
            is IllegalArgumentException -> "That server address isn't valid."
            else -> error.message ?: fallback
        }

        fun passwordResetNotice(email: String): String =
            "If there's an account for $email, we've sent it a link. Open it on this device to choose a new password."

        fun confirmEmailNotice(email: String): String =
            "Check your email to confirm. We've sent a link to $email; open it on this device to finish signing in."

        fun parseExpiry(text: String): Instant? =
            runCatching { Instant.parse(text) }.getOrNull() ?: runCatching { OffsetDateTime.parse(text).toInstant() }.getOrNull()

        fun hostOf(serverURL: String): String = runCatching { java.net.URI(serverURL).host }.getOrNull() ?: serverURL

        /**
         * Swift's `URL(string:)` plus `scheme.hasPrefix("http")`. Foundation
         * accepts (and escapes) characters such as spaces, so only the scheme
         * is checked here; the transport reports anything else when it connects.
         */
        fun isHttpURL(server: String): Boolean {
            val scheme = SCHEME_PATTERN.find(server)?.groupValues?.get(1) ?: return false
            return scheme.startsWith("http")
        }

        private val SCHEME_PATTERN = Regex("^([A-Za-z][A-Za-z0-9+.-]*):")

        fun deviceName(context: Context): String =
            runCatching { Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME) }.getOrNull()
                ?.takeIf { it.isNotBlank() } ?: Build.MODEL ?: "Android"

        fun schedulePeriodicWork(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context).cancelUniqueWork(LEGACY_WORK_NAME)
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}

/** The background cycle: one push-then-pull every 15 minutes or so, on any network. */
class SyncWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result =
        if (applicationContext.appContainer.sync.syncOnce()) Result.success() else Result.retry()
}
