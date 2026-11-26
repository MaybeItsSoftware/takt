package uk.co.maybeitsadam.takt.ui.conditions

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups

class ConditionPickerTest {
    private fun condition(id: String, name: String, category: String? = null, archived: Boolean = false) = TaskCondition(
        id = id, workspaceId = "w", name = name, isLocation = category == ConditionGroups.LOCATION, isArchived = archived,
        createdAt = Instant.EPOCH, updatedAt = Instant.EPOCH, category = category,
    )

    private val home = condition("home", "Home", ConditionGroups.LOCATION)
    private val campus = condition("campus", "Campus", ConditionGroups.LOCATION)
    private val laptop = condition("laptop", "Laptop", "Equipment")
    private val mat = condition("mat", "Floor space")
    private val lab = condition("lab", "Lab", archived = true)
    private val all = listOf(home, campus, laptop, mat, lab)
    private val places = setOf("home", "campus")

    @Test
    fun aTickIsAGroupOfItsOwnButASecondPlaceJoinsTheFirst() {
        var groups = RequirementToggle.toggling("laptop", null, places)
        assertEquals(listOf(listOf("laptop")), groups)
        groups = RequirementToggle.toggling("home", groups, places)
        groups = RequirementToggle.toggling("campus", groups, places)
        assertEquals(listOf(listOf("laptop"), listOf("home", "campus")), groups)
        groups = RequirementToggle.toggling("mat", groups, places)
        assertEquals(listOf(listOf("laptop"), listOf("home", "campus"), listOf("mat")), groups)
    }

    @Test
    fun untickingTakesItOutOfEveryGroupAndLeavesNothingAsNull() {
        val groups = listOf(listOf("home", "campus"), listOf("laptop"))
        assertEquals(listOf(listOf("campus"), listOf("laptop")), RequirementToggle.toggling("home", groups, places))
        assertNull(RequirementToggle.toggling("laptop", listOf(listOf("laptop")), places))
        assertTrue(RequirementToggle.requires("campus", groups))
        assertFalse(RequirementToggle.requires("mat", groups))
    }

    /**
     * A device condition heads the list under Device. Saying where you are,
     * it cannot be ticked; a task can still require it, so there it can.
     */
    @Test
    fun aDeviceConditionIsFixedOnlyWhenSayingWhereYouAre() {
        val pad = condition("pad", "iPad", "Equipment").copy(deviceKind = "tablet")
        val live = listOf(home, laptop, pad)
        val saying = ConditionPicker.list(ConditionGroups.all(live), live, live, "", isOn = { false }, exclusiveRadio = true, devicesFixed = true)
        assertEquals("Device", saying.sections.first().title)
        assertTrue(saying.sections.first().rows.single().isFixed)
        assertFalse(saying.sections.drop(1).flatMap { it.rows }.any { it.isFixed })
        val requiring = ConditionPicker.list(ConditionGroups.all(live), live, live, "", isOn = { false }, exclusiveRadio = false)
        assertFalse(requiring.sections.flatMap { it.rows }.any { it.isFixed })
    }

    @Test
    fun withNothingTypedTheRowsSitUnderTheCoresHeadings() {
        val live = all.filter { !it.isArchived }
        val list = ConditionPicker.list(ConditionGroups.all(live), live, all, "", isOn = { it == "home" }, exclusiveRadio = true)
        assertFalse(list.isSearching)
        assertNull(list.newName)
        assertEquals(listOf("Location", "Equipment", "Other"), list.sections.map { it.title })
        val location = list.sections.first().rows
        assertEquals(listOf("Home", "Campus"), location.map { it.condition.name })
        assertTrue(location.all { it.isExclusive })
        assertTrue(location.first().isOn)
        assertFalse(list.sections[1].rows.single().isExclusive)
        assertNull(location.first().category)
    }

    @Test
    fun placesAreRadiosOnlyWhenSayingWhereYouAre() {
        val live = all.filter { !it.isArchived }
        val list = ConditionPicker.list(ConditionGroups.all(live), live, all, "", isOn = { false }, exclusiveRadio = false)
        assertTrue(list.sections.flatMap { it.rows }.none { it.isExclusive })
    }

    @Test
    fun typingRanksOneFlatListAndNamesEachRowsCategory() {
        val live = all.filter { !it.isArchived }
        val list = ConditionPicker.list(ConditionGroups.all(live), live, all, "lap", isOn = { false }, exclusiveRadio = true)
        assertTrue(list.isSearching)
        val rows = list.sections.single().rows
        assertNull(list.sections.single().title)
        assertEquals("Laptop", rows.first().condition.name)
        assertEquals("Equipment", rows.first().category)
        assertEquals("lap", list.newName)
    }

    @Test
    fun aNameAlreadyTakenIsNotOfferedAgainArchivedOrNot() {
        val live = all.filter { !it.isArchived }
        assertNull(ConditionPicker.list(emptyList(), live, all, " laptop ", isOn = { false }, exclusiveRadio = false).newName)
        val archived = ConditionPicker.list(emptyList(), live, all, "LAB", isOn = { false }, exclusiveRadio = false)
        assertNull(archived.newName)
        assertTrue(archived.isEmpty)
        assertEquals("Desk", ConditionPicker.list(emptyList(), live, all, "Desk", isOn = { false }, exclusiveRadio = false).newName)
    }

    @Test
    fun summariesReadInWords() {
        val name = { id: String -> all.first { it.id == id }.name }
        assertEquals("Anytime", ConditionPicker.requirementSummary(null, name))
        assertEquals("Home or Campus + Laptop", ConditionPicker.requirementSummary(listOf(listOf("home", "campus"), listOf("laptop")), name))
        assertEquals("Campus", ConditionPicker.alternatives("home", listOf(listOf("home", "campus")), name))
        assertNull(ConditionPicker.alternatives("laptop", listOf(listOf("laptop")), name))
        assertEquals("Home · Laptop", ConditionPicker.nowSummary(all, setOf("laptop", "home")))
        assertNull(ConditionPicker.nowSummary(all, emptySet()))
        assertEquals("not Home + Campus or not Laptop", ConditionPicker.requirementSummary(listOf(listOf("!home"), listOf("campus", "!laptop")), name))
        assertEquals("not Laptop", ConditionPicker.alternatives("campus", listOf(listOf("campus", "!laptop")), name))
    }

    /** "Not" ticks a condition as not holding, turns one round, and unticks a "not"; ticking one off works either way. */
    @Test
    fun notTicksAConditionAsNotHolding() {
        val places = setOf("home", "campus")
        assertEquals(listOf(listOf("!home")), RequirementToggle.negating("home", null))
        assertEquals(listOf(listOf("campus"), listOf("!home")), RequirementToggle.negating("home", listOf(listOf("campus", "home"))))
        assertNull(RequirementToggle.negating("home", listOf(listOf("!home"))))
        assertTrue(RequirementToggle.requires("home", listOf(listOf("!home"))))
        assertTrue(RequirementToggle.requirement("home", listOf(listOf("!home")))!!.isNegated)
        assertNull(RequirementToggle.toggling("home", listOf(listOf("!home")), places))
        // A place after a "not" place is not an alternative to it.
        assertEquals(listOf(listOf("!home"), listOf("campus")), RequirementToggle.toggling("campus", listOf(listOf("!home")), places))
    }
}
