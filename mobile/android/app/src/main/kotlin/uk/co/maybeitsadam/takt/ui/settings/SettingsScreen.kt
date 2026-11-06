package uk.co.maybeitsadam.takt.ui.settings

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.autofill.ContentType
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.contentType
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.Instant
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.core.SyncCredentials
import uk.co.maybeitsadam.takt.core.SyncDeviceInfo
import uk.co.maybeitsadam.takt.settings.GoogleSignIn
import uk.co.maybeitsadam.takt.settings.SignedOutHint
import uk.co.maybeitsadam.takt.settings.SyncController
import uk.co.maybeitsadam.takt.settings.SyncUiState
import uk.co.maybeitsadam.takt.ui.components.Card
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.SignInWithAppleButton
import uk.co.maybeitsadam.takt.ui.components.SignInWithGoogleButton
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.inspector.SwitchRow
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.review.Segmented
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/** Theme, celebrations, sync and about. */
@Composable
fun SettingsScreen() {
    val shell = LocalShell.current
    val model = viewModel { SettingsViewModel(shell.container) }
    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        TaktTopBar(
            "Settings",
            showHistory = false,
            navigation = { IconAction(Icons.AutoMirrored.Filled.ArrowBack, "Back") { shell.navigator.back() } },
        )
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).imePadding().padding(Metrics.lg).testTag("settings"),
            verticalArrangement = Arrangement.spacedBy(Metrics.xl),
        ) {
            ThemeSection(model)
            TypeSection(model)
            CelebrationSection(model)
            FocusSection(model)
            ReminderSection(shell.container.reminders)
            SyncSection(shell.container.sync)
            CalendarSection(shell.container.calendar)
            DataSection(model)
            AboutSection(model)
        }
    }
}

@Composable
internal fun Section(title: String, footer: String? = null, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Text(title, style = TaktTheme.type.label, color = TaktTheme.colors.mutedText)
        Card(Modifier.fillMaxWidth()) { Column(content = content) }
        if (footer != null) Text(footer, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
    }
}

/** A radio row: a ring that fills azure when chosen. */
@Composable
internal fun ChoiceRow(title: String, selected: Boolean, detail: String? = null, tag: String = "", onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }
            .then(if (tag.isNotEmpty()) Modifier.testTag(tag) else Modifier)
            .padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = TaktTheme.type.body, color = TaktTheme.colors.ink)
            if (detail != null) Text(detail, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
        }
        RadioRing(selected)
    }
}

@Composable
private fun CelebrationSection(model: SettingsViewModel) {
    val style by model.celebration.collectAsStateWithLifecycle()
    // Each choice, and Preview, plays the chosen celebration on a sample row.
    var play by rememberSaveable { mutableStateOf(0) }
    CollapsibleSection("Completing a task", key = "celebration", summary = style.title) {
        CelebrationStyle.entries.forEachIndexed { index, option ->
            if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
            ChoiceRow(option.title, option == style, detail = option.detail, tag = "settings_celebration_${option.raw}") {
                model.setCelebration(option)
                play += 1
            }
        }
        Hairline()
        CelebrationPreview(style, play, onPreview = { play += 1 })
    }
}

@Composable
private fun FocusSection(model: SettingsViewModel) {
    val scoring by model.scoresEachFocusBlock.collectAsStateWithLifecycle()
    Section("Focus") {
        SwitchRow(
            "Focus points",
            checked = scoring,
            onCheckedChange = model::setScoresEachFocusBlock,
            detail = "Finishing a block asks how it went — a multiplier from ×1.0, nudged a tenth at a time — and " +
                "minutes times that is the block's points. Turn this off to drop scoring altogether: no question at " +
                "the end of a block, and no points anywhere.",
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("settings_focus_points"),
        )
    }
}

@Composable
private fun AboutSection(model: SettingsViewModel) {
    val licences by produceState(emptyList<Pair<String, String>>()) { value = model.licences() }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    CollapsibleSection("About", key = "about", summary = remember { model.versionName() }) {
        Row(Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(horizontal = Metrics.md), verticalAlignment = Alignment.CenterVertically) {
            Text("Version", style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
            MonoText(remember { model.versionName() })
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Row(Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(horizontal = Metrics.md), verticalAlignment = Alignment.CenterVertically) {
            Text("Core", style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
            MonoText(remember { model.coreVersionName() })
        }
        licences.forEach { (name, text) ->
            Hairline(color = TaktTheme.colors.borderMuted)
            ChoiceRowLike(name, if (open == name) "Hide" else "Licence") { open = if (open == name) null else name }
            if (open == name) {
                Text(
                    text, style = TaktTheme.type.monoSmall, color = TaktTheme.colors.mutedText,
                    modifier = Modifier.padding(horizontal = Metrics.md, vertical = Metrics.sm),
                )
            }
        }
    }
}

@Composable
private fun ChoiceRowLike(title: String, trailing: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).clickable(onClick = onClick).padding(horizontal = Metrics.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
        Text(trailing, style = TaktTheme.type.small, color = TaktTheme.colors.primary)
    }
}

// region Sync

@Composable
private fun SyncSection(sync: SyncController) {
    val status by sync.status.collectAsStateWithLifecycle()
    val credentials by sync.credentials.collectAsStateWithLifecycle()
    val signingIn by sync.isSigningIn.collectAsStateWithLifecycle()
    val error by sync.signInError.collectAsStateWithLifecycle()
    val hint by sync.signedOut.collectAsStateWithLifecycle()
    val signedIn = credentials
    if (signedIn != null) SignedInSync(sync, status, signedIn) else SignedOutSync(sync, signingIn, hint)
    error?.let { ErrorNote(it, Modifier.testTag("sync_error")) }
}

@Composable
private fun ErrorNote(message: String, modifier: Modifier = Modifier) {
    Text(
        message, style = TaktTheme.type.small, color = TaktTheme.colors.danger,
        modifier = modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.danger.copy(alpha = 0.08f), Metrics.control)
            .border(BorderStroke(Metrics.hairline, TaktTheme.colors.danger.copy(alpha = 0.4f)), Metrics.control)
            .padding(Metrics.md),
    )
}

@Composable
private fun InfoNote(message: String, modifier: Modifier = Modifier) {
    Text(
        message, style = TaktTheme.type.small, color = TaktTheme.colors.ink,
        modifier = modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.primary.copy(alpha = 0.08f), Metrics.control)
            .border(BorderStroke(Metrics.hairline, TaktTheme.colors.primary.copy(alpha = 0.4f)), Metrics.control)
            .padding(Metrics.md),
    )
}

@Composable
private fun InfoRow(title: String, modifier: Modifier = Modifier, value: @Composable () -> Unit) {
    Row(
        modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.md),
    ) {
        Text(title, style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
        value()
    }
}

@Composable
private fun SignedInSync(sync: SyncController, status: SyncUiState, credentials: SyncCredentials) {
    val scope = rememberCoroutineScope()
    val account by sync.account.collectAsStateWithLifecycle()
    val choosingPassword by sync.choosingPassword.collectAsStateWithLifecycle()
    var now by remember { mutableStateOf(Instant.now()) }
    var confirmSignOut by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    var changingPassword by remember { mutableStateOf(false) }
    var accountError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) { while (true) { delay(30_000); now = Instant.now() } }
    LaunchedEffect(credentials.deviceId) { accountError = sync.refreshAccount() }
    val email = account?.email ?: credentials.email

    Section("Sync") {
        InfoRow("Signed in as") {
            Text(
                email ?: "this device's account", style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
                modifier = Modifier.weight(2f, fill = false).testTag("sync_email"),
            )
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        InfoRow("Status") {
            Text(
                SyncStatusText.describe(status, now),
                style = TaktTheme.type.small,
                color = if (status is SyncUiState.Failed || status is SyncUiState.SessionExpired) TaktTheme.colors.danger else TaktTheme.colors.mutedText,
                modifier = Modifier.weight(2f, fill = false).testTag("sync_status"),
            )
        }
        if (credentials.serverURL.trimEnd('/') != SyncController.defaultServer.trimEnd('/')) {
            Hairline(color = TaktTheme.colors.borderMuted)
            InfoRow("Server") { MonoText(SyncController.hostOf(credentials.serverURL)) }
        }
        val endpoints by sync.endpoints.collectAsStateWithLifecycle()
        if (!endpoints.usesAccountsOf(SyncController.hostedEndpoints)) {
            Hairline(color = TaktTheme.colors.borderMuted)
            InfoRow("Accounts") { MonoText(SyncController.hostOf(endpoints.supabaseURL)) }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Row(Modifier.padding(Metrics.md), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            PButton("Sync now", icon = PIcons.Sync, enabled = status != SyncUiState.Syncing, modifier = Modifier.testTag("sync_now")) { sync.syncNow() }
            PButton("Sign out", modifier = Modifier.testTag("sync_sign_out")) { confirmSignOut = true }
        }
    }

    val devices = account?.devices.orEmpty()
    CollapsibleSection(
        "Devices",
        key = "devices",
        summary = if (devices.isEmpty()) null else "${devices.size}",
        footer = "Every device signed in to this account. To add one, sign in on it with the same account.",
    ) {
        when {
            devices.isNotEmpty() -> devices.forEachIndexed { index, device ->
                if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
                DeviceRow(device, now)
            }
            accountError != null -> Column(Modifier.padding(Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                Text(accountError!!, style = TaktTheme.type.small, color = TaktTheme.colors.danger)
                PButton("Try again") { scope.launch { accountError = sync.refreshAccount() } }
            }
            else -> Text("Loading…", style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(Metrics.md))
        }
    }

    Section("Account", footer = "Deleting the account removes it and everything it synced from the server. Each device keeps its own copy of your tasks.") {
        Row(Modifier.padding(Metrics.md), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            PButton("Change password", modifier = Modifier.testTag("sync_change_password")) { changingPassword = true }
            PButton("Delete account", destructive = true, modifier = Modifier.testTag("sync_delete_account")) { deleting = true }
        }
    }

    if (confirmSignOut) {
        AlertDialog(
            onDismissRequest = { confirmSignOut = false },
            containerColor = TaktTheme.colors.raised,
            tonalElevation = 0.dp,
            shape = Metrics.card,
            title = { Text("Sign out of sync?", style = TaktTheme.type.heading, color = TaktTheme.colors.ink) },
            text = { Text("Your tasks stay on this device. They stop syncing until you sign in again.", style = TaktTheme.type.body, color = TaktTheme.colors.mutedText) },
            confirmButton = {
                TextButton(onClick = { confirmSignOut = false; sync.signOut() }, modifier = Modifier.testTag("sync_sign_out_confirm")) {
                    Text("Sign out", color = TaktTheme.colors.danger)
                }
            },
            dismissButton = { TextButton(onClick = { confirmSignOut = false }) { Text("Cancel", color = TaktTheme.colors.ink) } },
        )
    }
    if (deleting) DeleteAccountDialog(sync, email) { deleting = false }
    if (changingPassword || choosingPassword) {
        NewPasswordDialog(sync, recovering = choosingPassword) {
            changingPassword = false
            sync.dismissChoosingPassword()
        }
    }
}

@Composable
private fun DeviceRow(device: SyncDeviceInfo, now: Instant) {
    val seen = device.lastSeenAt?.let(SyncController::parseExpiry)
    val detail = buildList {
        device.platform?.takeIf { it.isNotBlank() }?.let { add(platformName(it)) }
        when {
            device.current -> add("this device")
            seen != null && now.epochSecond - seen.epochSecond < 120 -> add("seen just now")
            seen != null -> add("last seen ${Format.day(seen)}, ${Format.time(seen)}")
        }
    }.joinToString(" · ")
    Column(
        Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(horizontal = Metrics.md, vertical = Metrics.sm).testTag("sync_device"),
        verticalArrangement = Arrangement.Center,
    ) {
        Text(device.name?.takeIf { it.isNotBlank() } ?: "Unnamed device", style = TaktTheme.type.body, color = TaktTheme.colors.ink)
        if (detail.isNotEmpty()) Text(detail, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
    }
}

private fun platformName(platform: String): String = when (platform.lowercase()) {
    "ios" -> "iPhone"
    "android" -> "Android"
    "macos", "mac" -> "Mac"
    else -> platform
}

@Composable
private fun DeleteAccountDialog(sync: SyncController, email: String?, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    var working by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!working) onDismiss() },
        containerColor = TaktTheme.colors.raised,
        tonalElevation = 0.dp,
        shape = Metrics.card,
        title = { Text("Delete your account?", style = TaktTheme.type.heading, color = TaktTheme.colors.ink) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                Text(
                    "This removes ${email ?: "the account"} and everything it synced from the server, and signs out every device. " +
                        "Each device keeps its own copy of your tasks. This can't be undone.",
                    style = TaktTheme.type.body, color = TaktTheme.colors.mutedText,
                )
                error?.let { Text(it, style = TaktTheme.type.small, color = TaktTheme.colors.danger) }
            }
        },
        confirmButton = {
            TextButton(
                enabled = !working,
                onClick = {
                    scope.launch {
                        working = true
                        error = sync.deleteAccount()
                        working = false
                        if (error == null) onDismiss()
                    }
                },
                modifier = Modifier.testTag("sync_delete_confirm"),
            ) { Text(if (working) "Deleting…" else "Delete account", color = TaktTheme.colors.danger) }
        },
        dismissButton = { TextButton(enabled = !working, onClick = onDismiss) { Text("Cancel", color = TaktTheme.colors.ink) } },
    )
}

@Composable
private fun NewPasswordDialog(sync: SyncController, recovering: Boolean, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    var password by remember { mutableStateOf("") }
    var working by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = { if (!working) onDismiss() },
        containerColor = TaktTheme.colors.raised,
        tonalElevation = 0.dp,
        shape = Metrics.card,
        title = {
            Text(if (recovering) "Choose a new password" else "Change password", style = TaktTheme.type.heading, color = TaktTheme.colors.ink)
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                Text(
                    "You'll sign in with it on your other devices. It needs at least 8 characters.",
                    style = TaktTheme.type.body, color = TaktTheme.colors.mutedText,
                )
                Field(
                    password, { password = it; error = null }, "New password", Modifier.testTag("sync_new_password"),
                    KeyboardType.Password, secret = true, autofill = ContentType.NewPassword,
                )
                error?.let { Text(it, style = TaktTheme.type.small, color = TaktTheme.colors.danger) }
            }
        },
        confirmButton = {
            TextButton(
                enabled = !working && password.isNotEmpty(),
                onClick = {
                    scope.launch {
                        working = true
                        error = sync.setPassword(password)
                        working = false
                        if (error == null) onDismiss()
                    }
                },
                modifier = Modifier.testTag("sync_new_password_save"),
            ) { Text(if (working) "Saving…" else "Save", color = TaktTheme.colors.primary) }
        },
        dismissButton = { TextButton(enabled = !working, onClick = onDismiss) { Text("Not now", color = TaktTheme.colors.ink) } },
    )
}

@Composable
private fun SignedOutSync(sync: SyncController, signingIn: Boolean, hint: SignedOutHint?) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var email by rememberSaveable(hint?.email) { mutableStateOf(hint?.email.orEmpty()) }
    var password by rememberSaveable { mutableStateOf("") }
    // "Use a different server": blank fields are Takt's own (SyncEndpoints.resolve).
    val endpoints by sync.endpoints.collectAsStateWithLifecycle()
    val hosted = SyncController.hostedEndpoints
    val ownServer = endpoints.serverURL.takeIf { it != hosted.serverURL }.orEmpty()
    val ownAccounts = !endpoints.usesAccountsOf(hosted)
    var server by rememberSaveable(endpoints) { mutableStateOf(ownServer) }
    var supabaseUrl by rememberSaveable(endpoints) { mutableStateOf(if (ownAccounts) endpoints.supabaseURL else "") }
    var supabaseKey by rememberSaveable(endpoints) { mutableStateOf(if (ownAccounts) endpoints.supabaseKey else "") }
    var otherServer by rememberSaveable(endpoints) { mutableStateOf(endpoints != hosted) }
    val requestingReset by sync.isRequestingReset.collectAsStateWithLifecycle()
    val notice by sync.notice.collectAsStateWithLifecycle()
    val ready = !signingIn && email.isNotBlank() && password.isNotEmpty()
    // Switches to the chosen endpoints (signing out of others first) and answers the server, or null.
    suspend fun target(): String? =
        if (otherServer) sync.useEndpoints(server, supabaseUrl, supabaseKey) else sync.useEndpoints("", "", "")
    // Google's client id belongs to Takt's project; a self-hosted one signs in with email or Apple.
    val googleAvailable = GoogleSignIn.isConfigured && !(otherServer && (supabaseUrl.isNotBlank() || supabaseKey.isNotBlank()))

    Section("Sync", footer = "One account keeps your tasks the same on every device. A password needs at least 8 characters.") {
        if (hint?.expired == true) {
            Text(
                "Signed out — sign in again to keep syncing.", style = TaktTheme.type.small, color = TaktTheme.colors.danger,
                modifier = Modifier.padding(start = Metrics.md, end = Metrics.md, top = Metrics.md).testTag("sync_status"),
            )
        }
        Column(Modifier.padding(Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            Field(
                email, { email = it; sync.clearSignInError() }, "Email", Modifier.testTag("sync_email_field"),
                KeyboardType.Email, autofill = ContentType.EmailAddress + ContentType.Username, imeAction = ImeAction.Next,
            )
            Field(
                password, { password = it; sync.clearSignInError() }, "Password", Modifier.testTag("sync_password_field"),
                KeyboardType.Password, secret = true, autofill = ContentType.Password + ContentType.NewPassword, imeAction = ImeAction.Done,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                PButton(if (signingIn) "Signing in…" else "Sign in", primary = true, enabled = ready, modifier = Modifier.testTag("sync_sign_in")) {
                    scope.launch { val target = target() ?: return@launch; if (sync.signIn(target, email, password)) password = "" }
                }
                PButton("Create account", enabled = ready, modifier = Modifier.testTag("sync_sign_up")) {
                    scope.launch { val target = target() ?: return@launch; sync.signUp(target, email, password) }
                }
            }
            Text(
                if (requestingReset) "Sending link…" else "Forgot password?",
                style = TaktTheme.type.small,
                color = if (signingIn || requestingReset) TaktTheme.colors.dimText else TaktTheme.colors.mutedText,
                modifier = Modifier
                    .heightIn(min = Metrics.touchTarget)
                    .clickable(enabled = !signingIn && !requestingReset, role = Role.Button) {
                        scope.launch { val target = target() ?: return@launch; sync.requestPasswordReset(target, email) }
                    }
                    .wrapContentHeight(Alignment.CenterVertically)
                    .testTag("sync_forgot_password"),
            )
            notice?.let { InfoNote(it, Modifier.testTag("sync_notice")) }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Column(Modifier.padding(Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            if (googleAvailable) {
                SignInWithGoogleButton(Modifier.fillMaxWidth().testTag("sync_google"), enabled = !signingIn) {
                    scope.launch { val target = target() ?: return@launch; sync.signInWithGoogle(context, target) }
                }
            }
            SignInWithAppleButton(Modifier.fillMaxWidth().testTag("sync_apple"), enabled = !signingIn) {
                scope.launch { val target = target() ?: return@launch; sync.signInWithApple(target) }
            }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clickable(role = Role.Button) { otherServer = !otherServer }
                .padding(horizontal = Metrics.md)
                .testTag("sync_other_server"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Use a different server", style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
            Icon(
                Icons.Filled.KeyboardArrowDown, contentDescription = null, tint = TaktTheme.colors.mutedText,
                modifier = Modifier.size(20.dp).rotate(if (otherServer) 180f else 0f),
            )
        }
        if (otherServer) {
            Column(
                Modifier.padding(start = Metrics.md, end = Metrics.md, bottom = Metrics.md),
                verticalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Field(
                    server, { server = it; sync.clearSignInError() }, "Sync server (${SyncController.hostOf(hosted.serverURL)})",
                    Modifier.testTag("sync_server_field"), KeyboardType.Uri,
                )
                Field(
                    supabaseUrl, { supabaseUrl = it; sync.clearSignInError() }, "Supabase URL (https://<project>.supabase.co)",
                    Modifier.testTag("sync_supabase_url_field"), KeyboardType.Uri,
                )
                Field(
                    supabaseKey, { supabaseKey = it; sync.clearSignInError() }, "Supabase publishable key",
                    Modifier.testTag("sync_supabase_key_field"), KeyboardType.Ascii,
                )
                Text(
                    "For a server you run yourself, and the Supabase project it trusts for accounts. " +
                        "Leave a field blank to use Takt's. Changing them signs this device out.",
                    style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
                )
                PButton("Check", enabled = !signingIn, modifier = Modifier.testTag("sync_check_server")) {
                    scope.launch { sync.checkEndpoints(server, supabaseUrl, supabaseKey) }
                }
            }
        }
    }
}

@Composable
private fun Field(
    value: String,
    onChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    keyboard: KeyboardType = KeyboardType.Text,
    secret: Boolean = false,
    autofill: ContentType? = null,
    imeAction: ImeAction = ImeAction.Default,
) {
    val colors = TaktTheme.colors
    BasicTextField(
        value = value,
        onValueChange = onChange,
        singleLine = true,
        textStyle = TaktTheme.type.field.copy(color = colors.ink),
        cursorBrush = SolidColor(colors.primary),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        keyboardOptions = KeyboardOptions(
            keyboardType = keyboard,
            autoCorrectEnabled = false,
            capitalization = KeyboardCapitalization.None,
            imeAction = imeAction,
        ),
        modifier = modifier.fillMaxWidth().semantics {
            contentDescription = placeholder
            if (autofill != null) contentType = autofill
        },
        decorationBox = { inner ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .background(colors.paper, Metrics.control)
                    .border(BorderStroke(Metrics.hairline, colors.inputBorder), Metrics.control)
                    .padding(horizontal = Metrics.md),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) Text(placeholder, style = TaktTheme.type.field, color = colors.dimText)
                inner()
            }
        },
    )
}

// endregion
