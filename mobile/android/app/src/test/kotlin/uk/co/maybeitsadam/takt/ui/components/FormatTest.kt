package uk.co.maybeitsadam.takt.ui.components

import org.junit.Assert.assertEquals
import org.junit.Test

class FormatTest {
    @Test
    fun spansReadAsPeopleTypeThem() {
        assertEquals(1500, Format.parseSpan("25"))
        assertEquals(1500, Format.parseSpan("25m"))
        assertEquals(4800, Format.parseSpan("1h 20m"))
        assertEquals(4800, Format.parseSpan("1h20"))
        assertEquals(5400, Format.parseSpan("1.5h"))
        assertEquals(0, Format.parseSpan(""))
        assertEquals(null, Format.parseSpan("soon"))
    }
}
