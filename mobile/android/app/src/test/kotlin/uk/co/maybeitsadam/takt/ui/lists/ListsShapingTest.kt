package uk.co.maybeitsadam.takt.ui.lists

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.BoardColumnsRead
import uk.co.maybeitsadam.takt.core.ListFolder
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskListRole
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WorkspaceItemKind
import uk.co.maybeitsadam.takt.core.WorkspaceKanbanColumn
import uk.co.maybeitsadam.takt.core.WorkspaceListTree
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.workspace.FilterList
import uk.co.maybeitsadam.takt.data.workspace.FilterListMatch
import uk.co.maybeitsadam.takt.data.workspace.FilterListScope
import uk.co.maybeitsadam.takt.data.workspace.ListScopeData
import uk.co.maybeitsadam.takt.data.workspace.SidebarData
import uk.co.maybeitsadam.takt.data.workspace.TaskDecoration
import uniffi.takt_core.FilterDefinition
import uniffi.takt_core.FilterRule

class ListsShapingTest {
    private val zone = ZoneOffset.UTC
    private val today = LocalDate.of(2026, 10, 2)
    private val t0 = Instant.parse("2026-01-01T00:00:00Z")

    private fun list(
        id: String,
        name: String = id,
        folder: String? = null,
        role: TaskListRole? = null,
        archived: Boolean = false,
        order: Int = 0,
    ) = TaskList(id, "w", folder, name, null, order, archived, role, null, null, t0, t0)

    private var order = 0

    private fun task(
        id: String,
        listId: String,
        parent: String? = null,
        status: TaskStatus = TaskStatus.OPEN,
        kind: WorkspaceItemKind? = null,
        due: Instant? = null,
    ) = WorkspaceTask(
        id, listId, parent, id.replaceFirstChar { it.uppercase() }, "", status, order++, due, null, null, null, kind, null, null,
        createdAt = t0, updatedAt = t0,
    )

    private fun scope(
        lists: List<TaskList>,
        tasks: List<WorkspaceTask>,
        decorations: Map<String, TaskDecoration> = emptyMap(),
        dailies: Set<String> = emptySet(),
    ) = ListScopeData(
        lists, lists.associate { l -> l.id to WorkspaceListTree(l.id, tasks.filter { it.listId == l.id }) }, decorations, dailies, lists,
    )

    private val home = list("home")
    /** The core's read of a board with no layout anywhere: the stock five, nothing placed. */
    private val stock = BoardColumnsRead.builtin("home/root")
    private val tasks = listOf(
        task("a", "home"),
        task("a1", "home", parent = "a"),
        task("a2", "home", parent = "a", status = TaskStatus.COMPLETED),
        task("a2x", "home", parent = "a2"),
        task("b", "home", due = Instant.parse("2026-10-01T00:00:00Z")),
    )

    @Test fun foldingHidesABranchButKeepsItsRow() {
        val shape = OutlineShaping.shape(scope(listOf(home), tasks), false, setOf("a"), false, emptySet(), zone, today)
        assertEquals(listOf("a", "b"), shape.rows.map { it.id })
        assertTrue(shape.rows.first().isFolded)
        assertTrue(shape.rows.first().hasChildren)
        assertEquals(setOf("a", "a2"), shape.parentIds)
    }

    @Test fun hidingCompletedDropsTheBranchBeneath() {
        val shape = OutlineShaping.shape(scope(listOf(home), tasks), false, emptySet(), true, emptySet(), zone, today)
        assertEquals(listOf("a", "a1", "b"), shape.rows.map { it.id })
    }

    @Test fun foldedAwayRowsLeaveTheOutline() {
        val shape = OutlineShaping.shape(scope(listOf(home), tasks), false, emptySet(), false, setOf("a2"), zone, today)
        assertEquals(listOf("a", "a1", "b"), shape.rows.map { it.id })
    }

    @Test fun rowsCarryDecorationsAndAnOverdueLabel() {
        val decorations = mapOf("b" to TaskDecoration(priority = 1, tags = listOf("work"), kanbanColumn = "today"))
        val shape = OutlineShaping.shape(scope(listOf(home), tasks, decorations, setOf("a")), false, emptySet(), false, emptySet(), zone, today)
        val b = shape.rows.first { it.id == "b" }
        assertEquals(1, b.priority)
        assertEquals(listOf("work"), b.tags)
        assertTrue(b.isPlanned)
        assertTrue(b.due!!.isOverdue)
        assertEquals("Yesterday", b.due!!.text)
        assertTrue(shape.rows.first { it.id == "a" }.isDaily)
    }

    @Test fun blockedRowsAreMarkedAndKeepTheReadOrder() {
        val data = scope(listOf(home), tasks).copy(blockedTaskIds = setOf("a1", "b"))
        val rows = OutlineShaping.shape(data, false, emptySet(), false, emptySet(), zone, today).rows
        // The core already sank blocked rows; the shaping draws them where they came.
        assertEquals(listOf("a", "a1", "a2", "a2x", "b"), rows.map { it.id })
        assertEquals(setOf("a1", "b"), rows.filter { it.isBlocked }.map { it.id }.toSet())

        val board = BoardShaping.shape(data, false, stock, emptySet(), false, zone, today)
        val cards = board.columns.flatMap { it.cards }
        assertTrue(cards.first { it.id == "b" }.row.isBlocked)
        assertFalse(cards.first { it.id == "a" }.row.isBlocked)
        assertTrue(cards.first { it.id == "a" }.subtasks.first { it.id == "a1" }.isBlocked)
    }

    @Test fun buriedRowsFadeUnderTheirOwnLabelBelowTheBlockedOnes() {
        // The core's read puts a (blocked) before b (buried); the shaping labels each run.
        val data = scope(listOf(home), tasks).copy(blockedTaskIds = setOf("a"), buriedTaskIds = setOf("b"))
        val shape = OutlineShaping.shape(data, false, emptySet(), false, emptySet(), zone, today)
        assertEquals(listOf("blocked:a", "a", "a1", "a2", "a2x", "buried:b", "b"), shape.entries.map { it.key })
        assertEquals(OutlineEntry.BlockedLabel("b", 0, isBuried = true), shape.entries[5])
        val b = shape.rows.first { it.id == "b" }
        assertTrue(b.isBuried)
        assertFalse(b.isBlocked)
        assertFalse(shape.rows.first { it.id == "a" }.isBuried)

        val column = BoardColumnModel(
            "today", "Today",
            shape.rows.filter { it.depth == 0 }.map { BoardCard(it, null, null, persistentListOf(), 0) }.toImmutableList(),
        )
        assertEquals("a", column.blockedLabelId)
        assertEquals("b", column.buriedLabelId)
    }

    @Test fun theBlockedLabelHeadsEachGroupsBlockedTail() {
        val data = scope(listOf(home), tasks).copy(blockedTaskIds = setOf("a1", "b"))
        val entries = OutlineShaping.shape(data, false, emptySet(), false, emptySet(), zone, today).entries
        // b ends the roots blocked; a1 has its done sibling a2 after it, so it fades unlabelled.
        assertEquals(listOf("a", "a1", "a2", "a2x", "blocked:b", "b"), entries.map { it.key })
        assertEquals(OutlineEntry.BlockedLabel("b", 0), entries[4])

        val column = BoardColumnModel(
            "today", "Today",
            OutlineShaping.shape(data, false, emptySet(), false, emptySet(), zone, today).rows
                .filter { it.depth == 0 }
                .map { BoardCard(it, null, null, persistentListOf(), 0) }
                .toImmutableList(),
        )
        assertEquals("b", column.blockedLabelId)
        assertNull(BoardColumnModel("x", "X", persistentListOf()).blockedLabelId)
    }

    @Test fun aWaitingRowShowsItsTagAndFollowUpOnlyWhileInWaitingOn() {
        val at = Instant.parse("2026-10-03T14:00:00Z")
        val decorations = mapOf(
            "b" to TaskDecoration(kanbanColumn = "waiting-on", waitingOn = "Sam", followUpAt = at),
            "a" to TaskDecoration(kanbanColumn = "today", waitingOn = "Legal", followUpAt = at),
        )
        val rows = OutlineShaping.shape(scope(listOf(home), tasks, decorations), false, emptySet(), false, emptySet(), zone, today).rows
        val b = rows.first { it.id == "b" }
        assertEquals("Sam", b.waitingOn)
        assertEquals("↻ Tomorrow 14:00", b.followUp)
        val a = rows.first { it.id == "a" }
        assertNull(a.waitingOn)
        assertNull(a.followUp)
    }

    @Test fun everythingHasAHeaderPerListWithRows() {
        val work = list("work")
        val all = tasks + task("w", "work")
        val shape = OutlineShaping.shape(scope(listOf(home, work, list("empty")), all), true, emptySet(), false, emptySet(), zone, today)
        val headers = shape.entries.filterIsInstance<OutlineEntry.Header>()
        assertEquals(listOf("home", "work"), headers.map { it.listId })
        // a, a1, b, and a2x (open beneath a closed task).
        assertEquals(4, headers.first().count)
    }

    @Test fun ancestorsAreFoundForReveal() {
        assertEquals(setOf("a", "a2"), OutlineShaping.ancestors(scope(listOf(home), tasks), "a2x"))
        assertTrue(OutlineShaping.ancestors(scope(listOf(home), tasks), "a").isEmpty())
    }

    @Test fun dropsLandAmongSiblingsOnly() {
        val rows = OutlineShaping.shape(scope(listOf(home), tasks), false, emptySet(), false, emptySet(), zone, today).rows
        // rows: a, a1, a2, a2x, b
        assertEquals(OutlineDrop.Before("a"), OutlineShaping.planDrop(rows, 4, 0))
        assertEquals(OutlineDrop.ToEnd, OutlineShaping.planDrop(rows, 0, 5))
        assertEquals(OutlineDrop.Before("a1"), OutlineShaping.planDrop(rows, 2, 1))
        // Where it already is.
        assertNull(OutlineShaping.planDrop(rows, 1, 2))
        // Into another parent's children: b joins a, before a2 (the iPhone's
        // rule, now the core's; this used to be refused).
        assertEquals(OutlineDrop.Reparent("home", "a", "a2"), OutlineShaping.planDrop(rows, 4, 2))
    }

    @Test fun unplacedCardsGoToTheFirstColumnAndSubtasksFollowTheirParent() {
        val decorations = mapOf("b" to TaskDecoration(kanbanColumn = "today"), "a1" to TaskDecoration(kanbanColumn = "today"))
        val board = BoardShaping.shape(scope(listOf(home), tasks, decorations), false, stock, emptySet(), false, zone, today)
        assertEquals("home/root", board.key)
        assertEquals(WorkspaceKanbanColumn.blitzitDefaults.map { it.id }, board.columns.map { it.id })
        val backlog = board.columns.first()
        assertEquals(listOf("a"), backlog.cards.map { it.id })
        assertEquals(listOf("a1", "a2", "a2x"), backlog.cards.first().subtasks.map { it.id })
        val todayColumn = board.columns.first { it.id == "today" }
        // a1 is filed apart from its parent, so it is a card of its own there.
        assertEquals(listOf("b", "a1"), todayColumn.cards.map { it.id })
        assertEquals("A", todayColumn.cards.last().parentTitle)
    }

    @Test fun everythingDrawsASubtaskOnceOnItsParentsCard() {
        val open = listOf(task("p", "home"), task("p1", "home", parent = "p"), task("p1x", "home", parent = "p1"), task("q", "home"))
        val board = BoardShaping.shape(scope(listOf(home), open), true, stock, emptySet(), false, zone, today)
        val drawn = board.columns.flatMap { it.cards }
        assertEquals(listOf("p", "q"), drawn.map { it.id })
        assertEquals(listOf("p1", "p1x"), drawn.first().subtasks.map { it.id })
        assertEquals(2, BoardShaping.matrix(board).unplaced.size)
    }

    @Test fun everythingKeepsASubtaskFiledInAnotherColumnAsItsOwnCardThere() {
        val open = listOf(task("p", "home"), task("p1", "home", parent = "p"), task("p2", "home", parent = "p"))
        val decorations = mapOf("p1" to TaskDecoration(kanbanColumn = "today"))
        val board = BoardShaping.shape(scope(listOf(home), open, decorations), true, stock, emptySet(), false, zone, today)
        assertEquals(listOf("p"), board.columns.first().cards.map { it.id })
        val todayColumn = board.columns.first { it.id == "today" }
        assertEquals(listOf("p1"), todayColumn.cards.map { it.id })
        assertEquals("P", todayColumn.cards.single().parentTitle)
        assertEquals(1, board.columns.sumOf { c -> c.cards.count { it.id == "p1" } })
        // The matrix counts the board's own cards, not the cross-column one.
        assertEquals(listOf("p"), BoardShaping.matrix(board).unplaced.map { it.id })
    }

    @Test fun renamingAColumnKeepsItsRulesAndAFiledOnlyColumnJoinsTheLayout() {
        val rule = FilterDefinition("all", "open", emptyList())
        val layout = listOf(WorkspaceKanbanColumn("a", "A", rule), WorkspaceKanbanColumn("b", "B"))
        assertEquals(listOf(WorkspaceKanbanColumn("a", "Now", rule), layout[1]), BoardShaping.renamingColumn(layout, "a", " Now "))
        assertEquals(layout + WorkspaceKanbanColumn("c", "Later"), BoardShaping.renamingColumn(layout, "c", "Later"))
        assertNull(BoardShaping.renamingColumn(layout, "b", "B"))
        assertNull(BoardShaping.renamingColumn(layout, "b", "  "))
    }

    @Test fun movingAColumnStepsPastItsDrawnNeighbour() {
        val layout = listOf("a", "hidden", "b", "c").map { WorkspaceKanbanColumn(it, it) }
        val drawn = listOf("a", "b", "c")
        assertEquals(listOf("hidden", "b", "a", "c"), BoardShaping.movingColumn(layout, drawn, "a", 1)?.map { it.id })
        assertEquals(listOf("a", "hidden", "c", "b"), BoardShaping.movingColumn(layout, drawn, "c", -1)?.map { it.id })
        assertNull(BoardShaping.movingColumn(layout, drawn, "a", -1))
        assertNull(BoardShaping.movingColumn(layout, drawn, "c", 1))
    }

    @Test fun aFiledOnlyColumnJoinsTheLayoutToMove() {
        val layout = listOf("a", "b").map { WorkspaceKanbanColumn(it, it) }
        val drawn = listOf("a", "b", "filed", "also")
        val titles = mapOf("filed" to "Filed", "also" to "Also")
        assertEquals(
            listOf(WorkspaceKanbanColumn("a", "a"), WorkspaceKanbanColumn("filed", "Filed"), WorkspaceKanbanColumn("b", "b")),
            BoardShaping.movingColumn(layout, drawn, "filed", -1, titles),
        )
        assertEquals(
            listOf("a", "b", "also", "filed"),
            BoardShaping.movingColumn(layout, drawn, "filed", 1, titles)?.map { it.id },
        )
        assertNull(BoardShaping.movingColumn(layout, drawn, "filed", -1))
    }

    @Test fun aWaitingBadgeNeedsTheWaitingColumnAndChasesOnlyAnOpenTask() {
        val zone = ZoneOffset.UTC
        val today = LocalDate.of(2026, 10, 20)
        val at = Instant.parse("2026-10-21T09:00:00Z")
        val waiting = uk.co.maybeitsadam.takt.data.workspace.TaskWaitingDetails(waitingOn = "Sam", followUpAt = at, column = "waiting-on")
        assertEquals(WaitingBadge("Sam", "↻ Tomorrow 09:00"), WaitingBadge.of(waiting, isOpen = true, today, zone))
        assertEquals(WaitingBadge("Sam", null), WaitingBadge.of(waiting, isOpen = false, today, zone))
        assertNull(WaitingBadge.of(waiting.copy(column = "backlog"), isOpen = true, today, zone))
        assertNull(WaitingBadge.of(waiting.copy(waitingOn = null), isOpen = false, today, zone))
        assertNull(WaitingBadge.of(null, isOpen = true, today, zone))
    }

    @Test fun aFoldedCardDrawsNoSubtasks() {
        val board = BoardShaping.shape(scope(listOf(home), tasks), false, stock, setOf("a"), false, zone, today)
        val card = board.columns.first().cards.first()
        assertTrue(card.subtasks.isEmpty())
        assertEquals(3, card.subtaskCount)
    }

    @Test fun theMatrixPlacesBoardCardsByQuadrant() {
        val decorations = mapOf(
            "a" to TaskDecoration(matrixUrgency = 1, matrixImportance = 1),
            "b" to TaskDecoration(matrixUrgency = 0, matrixImportance = 0),
        )
        val board = BoardShaping.shape(
            scope(listOf(home), tasks + task("c", "home"), decorations), false, stock, emptySet(), false, zone, today,
        )
        val matrix = BoardShaping.matrix(board)
        assertEquals(MatrixCell.DO_NOW, matrix.cellOf("a"))
        assertEquals(MatrixCell.ELIMINATE, matrix.cellOf("b"))
        assertEquals(listOf("c"), matrix.unplaced.map { it.id })
    }

    @Test fun aQuadrantReadsRankedCardsFirstThenTheRestInBoardOrder() {
        val decorations = mapOf(
            "a" to TaskDecoration(matrixUrgency = 1, matrixImportance = 1),
            "b" to TaskDecoration(matrixUrgency = 1, matrixImportance = 1, matrixRank = 1),
            "c" to TaskDecoration(matrixUrgency = 1, matrixImportance = 1, matrixRank = 0),
        )
        val board = BoardShaping.shape(
            scope(listOf(home), tasks + task("c", "home"), decorations), false, stock, emptySet(), false, zone, today,
        )
        val doNow = BoardShaping.matrix(board).quadrants.first { it.cell == MatrixCell.DO_NOW }
        assertEquals(listOf("c", "b", "a"), doNow.cards.map { it.id })
    }

    @Test fun theColumnsDrawnAreTheCoresAndACardFiledInAMissingOneGoesFirst() {
        val read = BoardColumnsRead(
            "home/root",
            layout = listOf(WorkspaceKanbanColumn("now", "Now")),
            source = "default",
            columns = listOf(WorkspaceKanbanColumn("now", "Now"), WorkspaceKanbanColumn("today", "Today")),
        )
        val decorations = mapOf("a" to TaskDecoration(kanbanColumn = "today"), "b" to TaskDecoration(kanbanColumn = "gone"))
        val board = BoardShaping.shape(scope(listOf(home), tasks, decorations), false, read, emptySet(), false, zone, today)
        assertEquals(listOf("now", "today"), board.columns.map { it.id })
        assertEquals(listOf("b"), board.columns.first().cards.map { it.id })
        assertEquals(listOf("a"), board.columns.last().cards.map { it.id })
        // Edits save the layout, not the columns drawn for cards filed elsewhere.
        assertEquals(listOf("now"), board.layout.map { it.id })
    }

    @Test fun aPlacementOverridesTheFiledColumn() {
        val rules = FilterDefinition("all", "all", listOf(FilterRule("tag", "has", "urgent")))
        val urgent = WorkspaceKanbanColumn("urgent", "Urgent", rules)
        val read = BoardColumnsRead(
            "home/root",
            layout = listOf(urgent) + WorkspaceKanbanColumn.blitzitDefaults,
            source = "own",
            columns = listOf(urgent) + WorkspaceKanbanColumn.blitzitDefaults,
            // b matches the rule column; a is clamped to an entered card's floor.
            placements = mapOf("b" to "urgent", "a" to "this-week"),
        )
        val decorations = mapOf("b" to TaskDecoration(kanbanColumn = "today"), "a" to TaskDecoration(kanbanColumn = "backlog"))
        val board = BoardShaping.shape(scope(listOf(home), tasks, decorations), false, read, emptySet(), false, zone, today)
        val byColumn = board.columns.associate { c -> c.id to c.cards.map { it.id } }
        assertEquals(listOf("b"), byColumn["urgent"])
        assertEquals(listOf("a"), byColumn["this-week"])
        assertEquals(emptyList<String>(), byColumn["today"])
        assertEquals(emptyList<String>(), byColumn["backlog"])
        assertTrue(board.columns.first().isRuleColumn)
        assertFalse(board.columns.last().isRuleColumn)
        // Its unfiled subtasks stay on the card, wherever it is drawn.
        assertEquals(listOf("a1", "a2", "a2x"), board.columns.first { it.id == "this-week" }.cards.single().subtasks.map { it.id })
    }

    @Test fun columnIdsAreSluggedAndUnique() {
        val columns = listOf(WorkspaceKanbanColumn("waiting-on", "Waiting on"))
        assertEquals("waiting-on-2", BoardShaping.uniqueColumnId("Waiting  on!", columns))
        assertEquals("column", BoardShaping.uniqueColumnId("!!", columns))
    }

    @Test fun theTreeListsInboxFoldersAndArchived() {
        val inbox = list("inbox", "Inbox", role = TaskListRole.INBOX)
        val inFolder = list("x", "X", folder = "f")
        val loose = list("y", "Y", order = 1)
        val gone = list("z", "Z", archived = true)
        val folder = ListFolder("f", "w", null, "Folder", 0, t0, t0)
        val nestedTasks = listOf(task("n", "y", kind = WorkspaceItemKind.LIST), task("n1", "y", parent = "n"), task("open", "x"))
        val data = SidebarData(
            listOf(folder),
            listOf(inbox, inFolder, loose, gone),
            listOf(inbox, inFolder, loose).associate { l -> l.id to WorkspaceListTree(l.id, nestedTasks.filter { it.listId == l.id }) },
        )
        val state = ListsTreeShaping.shape(data, emptySet(), showArchived = true)
        assertEquals(
            listOf("row:everything", "list:inbox", "folder:f", "list:x", "list:y", "nested:y:n", "row:archived", "archived:z"),
            state.rows.map { it.key },
        )
        assertEquals(2, (state.rows[0] as TreeRow.Everything).openCount)
        assertEquals(1, (state.rows.first { it.key == "nested:y:n" } as TreeRow.NestedRow).openCount)

        val collapsed = ListsTreeShaping.shape(data, setOf("f"), showArchived = false)
        assertFalse(collapsed.rows.any { it.key == "list:x" })
        assertFalse(collapsed.rows.any { it.key == "archived:z" })
    }

    private fun filterList(id: String, name: String = id) = FilterList(
        id = id, workspaceId = "w", name = name, icon = null, colorHex = null,
        definition = FilterDefinition("all", "open", emptyList()), summary = "Open tasks", sortOrder = 0, isBuiltin = false,
    )

    @Test fun filterListsSitUnderTheInboxAboveYourLists() {
        val inbox = list("inbox", "Inbox", role = TaskListRole.INBOX)
        val loose = list("y", "Y")
        val data = SidebarData(
            emptyList(),
            listOf(inbox, loose),
            listOf(inbox, loose).associate { it.id to WorkspaceListTree(it.id, emptyList()) },
            filterLists = listOf(filterList("b", "Calls"), filterList("a", "Errands")),
        )
        val state = ListsTreeShaping.shape(data, emptySet(), showArchived = false)
        assertEquals(listOf("row:everything", "list:inbox", "filter:b", "filter:a", "list:y"), state.rows.map { it.key })
        val calls = state.rows[2] as TreeRow.FilterListRow
        assertEquals("Calls", calls.name)
        assertEquals("Open tasks", calls.summary)
    }

    @Test fun prioritisedListsHeadYourListsInRankOrderAndStayInTheirFolders() {
        val inbox = list("inbox", "Inbox", role = TaskListRole.INBOX)
        val inFolder = list("x", "X", folder = "f").copy(priorityRank = 2)
        val loose = list("y", "Y", order = 1).copy(priorityRank = 1)
        val plain = list("p", "P", order = 2)
        val folder = ListFolder("f", "w", null, "Folder", 0, t0, t0)
        val lists = listOf(inbox, inFolder, loose, plain)
        val data = SidebarData(
            listOf(folder),
            lists,
            lists.associate { it.id to WorkspaceListTree(it.id, emptyList()) },
            filterLists = listOf(filterList("a", "Errands")),
        )
        val state = ListsTreeShaping.shape(data, emptySet(), showArchived = false)
        assertEquals(
            listOf(
                "row:everything", "list:inbox", "filter:a", "row:priorities", "priority:y", "priority:x",
                "folder:f", "list:x", "list:y", "list:p",
            ),
            state.rows.map { it.key },
        )
        val first = state.rows.first { it.key == "priority:y" } as TreeRow.ListRow
        assertEquals(1, first.priority)
        assertEquals(2, first.priorityCount)
        assertEquals(2, (state.rows.first { it.key == "list:x" } as TreeRow.ListRow).priority)
        assertNull((state.rows.first { it.key == "list:p" } as TreeRow.ListRow).priority)

        // Nothing prioritised, no heading.
        val none = ListsTreeShaping.shape(
            data.copy(lists = listOf(inbox, plain)), emptySet(), showArchived = false,
        )
        assertFalse(none.rows.any { it.key == "row:priorities" })
    }

    @Test fun aFilterListsMatchesAreGroupedByListInTheirOrder() {
        val work = list("work", "Work")
        val home = list("home", "Home")
        val scope = FilterListScope(
            filterList("calls", "Calls"),
            listOf(FilterListMatch(task("a", "work"), work), FilterListMatch(task("b", "work"), work), FilterListMatch(task("c", "home"), home)),
            blockedTaskIds = setOf("b"),
        )
        val state = FilterListShaping.shape(scope)
        assertEquals(listOf("Work", "Home"), state.sections.map { it.list.name })
        assertEquals(listOf("a", "b"), state.sections[0].tasks.map { it.id })
        assertEquals(3, state.count)
        assertEquals(setOf("b"), state.blockedTaskIds)
        assertTrue(FilterListShaping.shape(FilterListScope(null)).isMissing)
    }

    @Test fun theDetailKeyTellsAFilterListFromAList() {
        val key = DetailKey.filterKey("F1")
        assertEquals("F1", DetailKey.filterListId(key))
        assertEquals(key, DetailKey.selection(key))
        assertNull(DetailKey.filterListId(DetailKey.encode("L1", "T1")))
        assertEquals("L1", DetailKey.selection(DetailKey.encode("L1", "T1")))
        assertEquals("", DetailKey.selection(DetailKey.encode(null)))
    }
}
