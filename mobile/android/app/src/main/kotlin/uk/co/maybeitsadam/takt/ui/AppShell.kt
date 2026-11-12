package uk.co.maybeitsadam.takt.ui

import androidx.activity.compose.BackHandler
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.statusBars
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.ui.lists.Sidebar
import uk.co.maybeitsadam.takt.ui.lists.SidebarKeys
import uk.co.maybeitsadam.takt.ui.lists.DetailKey
import android.content.Intent
import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.app.QuickAddRequest
import uk.co.maybeitsadam.takt.ui.actions.RecurrenceClashSheet
import uk.co.maybeitsadam.takt.ui.commands.CommandPalette
import uk.co.maybeitsadam.takt.ui.commands.CommandRegistry
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.LocalCommandRegistry
import uk.co.maybeitsadam.takt.ui.commands.PaletteCommand
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.VerticalHairline
import uk.co.maybeitsadam.takt.ui.focus.FocusScreen
import uk.co.maybeitsadam.takt.ui.history.HistorySheet
import uk.co.maybeitsadam.takt.ui.inspector.TaskInspector
import uk.co.maybeitsadam.takt.ui.lists.FilterListScreen
import uk.co.maybeitsadam.takt.ui.lists.ListScreen
import uk.co.maybeitsadam.takt.ui.lists.ListsScreen
import uk.co.maybeitsadam.takt.ui.navigation.AppNavigator
import uk.co.maybeitsadam.takt.ui.navigation.FocusRoute
import uk.co.maybeitsadam.takt.ui.navigation.FilterListRoute
import uk.co.maybeitsadam.takt.ui.navigation.ListRoute
import uk.co.maybeitsadam.takt.ui.navigation.ListsRoute
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.navigation.ReviewRoute
import uk.co.maybeitsadam.takt.ui.navigation.SearchRoute
import uk.co.maybeitsadam.takt.ui.navigation.SettingsRoute
import uk.co.maybeitsadam.takt.ui.navigation.Shell
import uk.co.maybeitsadam.takt.ui.navigation.Tab
import uk.co.maybeitsadam.takt.ui.navigation.TodayRoute
import uk.co.maybeitsadam.takt.ui.navigation.WindowLayout
import uk.co.maybeitsadam.takt.ui.quickadd.QuickAddSheet
import uk.co.maybeitsadam.takt.ui.habits.HabitSheet
import uk.co.maybeitsadam.takt.ui.review.ReviewScreen
import uk.co.maybeitsadam.takt.ui.search.SearchScreen
import uk.co.maybeitsadam.takt.settings.SupabaseAccounts
import uk.co.maybeitsadam.takt.ui.settings.SettingsScreen
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.today.TodayScreen
import uk.co.maybeitsadam.takt.ui.undo.SnackAction

/**
 * Intents the shell understands: quick add (tile, launcher shortcut), a tab
 * to open, and the browser coming back from a sign-in (`takt://auth-callback`).
 */
object ShellIntents {
    const val ACTION_QUICK_ADD = "uk.co.maybeitsadam.priority.QUICK_ADD"
    const val EXTRA_TAB = "uk.co.maybeitsadam.priority.TAB"
    const val EXTRA_TASK_ID = "uk.co.maybeitsadam.priority.TASK_ID"
}

private class NavControllerNavigator(private val nav: NavHostController) : AppNavigator {
    override fun openTab(tab: Tab) {
        nav.navigate(tab.route) {
            popUpTo(nav.graph.findStartDestination().id) { saveState = true }
            launchSingleTop = true
            restoreState = true
        }
    }

    override fun openList(listId: String?, revealTaskId: String?, root: Boolean) {
        nav.navigate(ListRoute(listId, revealTaskId, root)) {
            if (root) popUpTo(nav.graph.findStartDestination().id)
            launchSingleTop = true
        }
    }

    override fun openFilterList(id: String, root: Boolean) {
        nav.navigate(FilterListRoute(id, root)) {
            if (root) popUpTo(nav.graph.findStartDestination().id)
            launchSingleTop = true
        }
    }

    override fun openSettings() {
        nav.navigate(SettingsRoute) { launchSingleTop = true }
    }

    override fun back() {
        nav.popBackStack()
    }
}

/**
 * The window: the sidebar (a drawer on phones, always open on wide screens),
 * the screens, and the shared surfaces over them — the inspector, quick add,
 * the history sheet, the command palette and the undo snackbar.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppShell(container: AppContainer, pendingIntent: Intent?, onIntentHandled: () -> Unit) {
    val nav = rememberNavController()
    val navigator = remember(nav) { NavControllerNavigator(nav) }
    val registry = remember { CommandRegistry() }
    var historyOpen by remember { mutableStateOf(false) }
    var paletteOpen by remember { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }
    val haptics = LocalHapticFeedback.current
    val inspectorTaskId by container.inspector.taskId.collectAsStateWithLifecycle()
    val quickAdd by container.quickAdd.request.collectAsStateWithLifecycle()
    val habitRequest by container.habits.request.collectAsStateWithLifecycle()
    val recurrenceClash by container.clashes.pending.collectAsStateWithLifecycle()

    LaunchedEffect(container) {
        container.undo.messages.collect { message ->
            val result = snackbar.showSnackbar(
                message = message.text,
                actionLabel = message.action?.label,
                withDismissAction = false,
                duration = if (message.action != null) SnackbarDuration.Short else SnackbarDuration.Short,
            )
            if (result == SnackbarResult.ActionPerformed) {
                haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                when (message.action) {
                    SnackAction.UNDO -> container.undo.undo()
                    SnackAction.REDO -> container.undo.redo()
                    null -> Unit
                }
            }
        }
    }

    val joining by container.listsPrompt.join.collectAsStateWithLifecycle()
    joining?.let { text ->
        uk.co.maybeitsadam.takt.ui.lists.JoinListSheet(text, onDismiss = container.listsPrompt::clearJoin)
    }

    LaunchedEffect(pendingIntent) {
        val intent = pendingIntent ?: return@LaunchedEffect
        when {
            intent.action == ShellIntents.ACTION_QUICK_ADD -> container.quickAdd.open(QuickAddRequest())
            SupabaseAccounts.isRedirect(intent.data) -> {
                container.sync.handleAuthRedirect(intent.data!!)
                navigator.openSettings()
            }
            intent.data?.scheme == "takt" && intent.data?.host == "join" -> container.listsPrompt.requestJoin(intent.data.toString())
            else -> {
                intent.getStringExtra(ShellIntents.EXTRA_TAB)?.let { name ->
                    Tab.entries.firstOrNull { it.name.equals(name, ignoreCase = true) }?.let(navigator::openTab)
                }
                intent.getStringExtra(ShellIntents.EXTRA_TASK_ID)?.let(container.inspector::open)
            }
        }
        onIntentHandled()
    }

    val globalCommands = remember(container, navigator) {
        listOf(
            KeyCommand.GO_TODAY.palette { navigator.openTab(Tab.TODAY) },
            KeyCommand.GO_EVERYTHING.palette { navigator.openList(null) },
            KeyCommand.GO_FOCUS.palette { navigator.openTab(Tab.FOCUS) },
            KeyCommand.GO_TIMELINE.palette { navigator.openTab(Tab.REVIEW) },
            KeyCommand.GO_SEARCH.palette { navigator.openTab(Tab.SEARCH) },
            PaletteCommand("goLists", "Go to Lists", "Go", "") { navigator.openTab(Tab.LISTS) },
            KeyCommand.TASK_NEW.palette { container.quickAdd.open(QuickAddRequest(listId = container.quickAdd.contextListId.value)) },
            PaletteCommand("habitNew", "New habit", "Task", "") { container.habits.open(null) },
            KeyCommand.WINDOW_UNDO.palette { container.undo.undo() },
            KeyCommand.WINDOW_REDO.palette { container.undo.redo() },
            KeyCommand.WINDOW_HISTORY.palette { historyOpen = true },
            KeyCommand.WINDOW_SETTINGS.palette { navigator.openSettings() },
        )
    }

    val chords = remember(container, navigator) {
        listOf<Pair<KeyCommand, () -> Unit>>(
            KeyCommand.WINDOW_UNDO to { container.undo.undo() },
            KeyCommand.WINDOW_REDO to { container.undo.redo() },
            KeyCommand.GO_COMMAND_PALETTE to { paletteOpen = true },
            KeyCommand.TASK_NEW to { container.quickAdd.open(QuickAddRequest(listId = container.quickAdd.contextListId.value)) },
            KeyCommand.GO_SEARCH to { navigator.openTab(Tab.SEARCH) },
            KeyCommand.GO_TODAY to { navigator.openTab(Tab.TODAY) },
            KeyCommand.GO_EVERYTHING to { navigator.openList(null) },
            KeyCommand.GO_FOCUS to { navigator.openTab(Tab.FOCUS) },
            KeyCommand.GO_TIMELINE to { navigator.openTab(Tab.REVIEW) },
            KeyCommand.WINDOW_SETTINGS to { navigator.openSettings() },
        )
    }

    BoxWithConstraints(
        Modifier
            .fillMaxSize()
            .semantics { testTagsAsResourceId = true }
            .background(TaktTheme.colors.paper)
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                val match = chords.firstOrNull { it.first.matchesShortcut(event) } ?: return@onPreviewKeyEvent false
                match.second()
                true
            },
    ) {
        val layout = WindowLayout(maxWidth.value.toInt())
        val drawer = rememberDrawerState(DrawerValue.Closed)
        val drawerScope = rememberCoroutineScope()
        val shell = remember(container, navigator, layout, drawer) {
            Shell(
                container, navigator, layout,
                showHistory = { historyOpen = true },
                showPalette = { paletteOpen = true },
                openSidebar = if (layout.isWide) null else ({ drawerScope.launch { drawer.open() } }),
            )
        }
        CompositionLocalProvider(LocalShell provides shell, LocalCommandRegistry provides registry) {
            val backStack by nav.currentBackStackEntryAsState()
            val destination = backStack?.destination
            val currentTab = Tab.entries.firstOrNull { tab -> destination?.hasRoute(tab.route::class) == true }
                ?: if (destination?.hasRoute(ListRoute::class) == true || destination?.hasRoute(FilterListRoute::class) == true) Tab.LISTS else null
            // A list has its own add field; Today has only this.
            val showFab = currentTab == Tab.TODAY
            val sidebarKey = backStack?.let { entry ->
                when {
                    destination?.hasRoute(ListRoute::class) == true -> entry.toRoute<ListRoute>().listId ?: ""
                    destination?.hasRoute(FilterListRoute::class) == true -> DetailKey.filterKey(entry.toRoute<FilterListRoute>().id)
                    currentTab == Tab.TODAY -> SidebarKeys.TODAY
                    currentTab != null && currentTab != Tab.LISTS -> SidebarKeys.tab(currentTab)
                    else -> null
                }
            }

            val content: @Composable (Modifier) -> Unit = { modifier ->
                Row(modifier.fillMaxSize()) {
                    Box(Modifier.weight(1f).fillMaxHeight()) {
                        AppNavHost(nav)
                        if (showFab && !layout.isWide) {
                            QuickAddFab(
                                Modifier.align(Alignment.BottomEnd).padding(Metrics.lg),
                            ) {
                                // From Today the task is planned for today, so it lands where it was typed.
                                val fromToday = currentTab == Tab.TODAY
                                container.quickAdd.open(
                                    QuickAddRequest(listId = if (fromToday) null else container.quickAdd.contextListId.value, plansToday = fromToday),
                                )
                            }
                        }
                    }
                    val taskId = inspectorTaskId
                    if (layout.inspectorAsPane && taskId != null) {
                        VerticalHairline()
                        Box(Modifier.width(Metrics.inspectorWidth).fillMaxHeight().background(TaktTheme.colors.paper)) {
                            TaskInspector(taskId = taskId, asSheet = false, onClose = container.inspector::close)
                        }
                    }
                }
            }

            if (layout.isWide) {
                Row(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.navigationBars)) {
                    Sidebar(
                        sidebarKey, onDone = {},
                        modifier = Modifier.width(Metrics.sidebarWidth).windowInsetsPadding(WindowInsets.statusBars),
                    )
                    VerticalHairline()
                    content(Modifier.weight(1f))
                }
            } else {
                BackHandler(enabled = drawer.isOpen) { drawerScope.launch { drawer.close() } }
                ModalNavigationDrawer(
                    drawerState = drawer,
                    // Opened by the title bar's button; a swipe only closes it, so rows keep their own swipes.
                    gesturesEnabled = drawer.isOpen,
                    scrimColor = TaktTheme.colors.ink.copy(alpha = 0.32f),
                    drawerContent = {
                        ModalDrawerSheet(
                            drawerState = drawer,
                            modifier = Modifier.width(Metrics.sidebarWidth),
                            drawerShape = RectangleShape,
                            drawerContainerColor = TaktTheme.colors.altRow,
                            drawerTonalElevation = 0.dp,
                            windowInsets = WindowInsets.systemBars,
                        ) {
                            Sidebar(sidebarKey, onDone = { drawerScope.launch { drawer.close() } })
                        }
                    },
                ) {
                    content(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.navigationBars))
                }
            }

            SnackbarHost(
                snackbar,
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .windowInsetsPadding(WindowInsets.navigationBars)
                    .padding(bottom = Metrics.lg, start = Metrics.md, end = Metrics.md),
            ) { data ->
                Snackbar(
                    snackbarData = data,
                    shape = Metrics.popover,
                    containerColor = TaktTheme.colors.ink,
                    contentColor = TaktTheme.colors.paper,
                    actionColor = if (TaktTheme.colors.isDark) TaktTheme.colors.primary else TaktTheme.colors.dimText,
                )
            }

            val taskId = inspectorTaskId
            if (!layout.inspectorAsPane && taskId != null) {
                val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
                ModalBottomSheet(
                    onDismissRequest = container.inspector::close,
                    sheetState = sheetState,
                    containerColor = TaktTheme.colors.altRow,
                    tonalElevation = 0.dp,
                    shape = Metrics.sheetTop,
                    dragHandle = { SheetHandle() },
                ) {
                    TaskInspector(taskId = taskId, asSheet = true, onClose = container.inspector::close)
                }
            }

            quickAdd?.let { request ->
                QuickAddSheet(request = request, onDismiss = container.quickAdd::close)
            }

            habitRequest?.let { request ->
                HabitSheet(request = request, onDismiss = container.habits::close)
            }

            if (historyOpen) HistorySheet(onDismiss = { historyOpen = false })

            // Last, so it sits over the inspector sheet a save asked it from.
            recurrenceClash?.let { pending ->
                RecurrenceClashSheet(pending, onChoose = container.clashes::answer)
            }

            if (paletteOpen) {
                CommandPalette(
                    commands = globalCommands + registry.commands,
                    onDismiss = { paletteOpen = false },
                )
            }
        }
    }
}

@Composable
private fun AppNavHost(nav: NavHostController) {
    NavHost(
        navController = nav,
        startDestination = TodayRoute,
        enterTransition = { fadeIn() },
        exitTransition = { fadeOut() },
        popEnterTransition = { EnterTransition.None },
        popExitTransition = { ExitTransition.None },
    ) {
        composable<TodayRoute> { TodayScreen() }
        composable<ListsRoute> { ListsScreen() }
        composable<ListRoute> { entry -> ListScreen(route = entry.toRoute()) }
        composable<FilterListRoute> { entry -> FilterListScreen(route = entry.toRoute()) }
        composable<FocusRoute> { FocusScreen() }
        composable<ReviewRoute> { ReviewScreen() }
        composable<SearchRoute> { SearchScreen() }
        composable<SettingsRoute> { SettingsScreen() }
    }
}

/**
 * The quick-add button: a 44dp square of the accent at the popover radius,
 * flat, with no shadow (Compose grows its hit area to 48).
 */
@Composable
private fun QuickAddFab(modifier: Modifier = Modifier, onClick: () -> Unit) {
    Box(
        modifier
            .size(44.dp)
            .clip(Metrics.popover)
            .background(TaktTheme.colors.primary)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = "Add a task" }
            .testTag("quick_add_fab"),
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Filled.Add, null, tint = TaktTheme.colors.onAccent, modifier = Modifier.size(20.dp))
    }
}

/** The bottom sheet's grabber: a short rule in the border colour, quiet as the Mac's chrome. */
@Composable
fun SheetHandle() {
    Box(Modifier.fillMaxWidth().padding(vertical = Metrics.sm), contentAlignment = Alignment.Center) {
        Box(Modifier.width(32.dp).height(3.dp).clip(Metrics.pill).background(TaktTheme.colors.border))
    }
}
