package uk.co.maybeitsadam.takt.data.drive

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class GoogleDriveFilesTest {
    @Test
    fun thePickersAnswerNamesTheFiles() {
        assertEquals(listOf("1a", "2b", "3c"), GoogleDriveFiles.pickedIds("1a,2b, 3c,"))
        assertEquals(emptyList<String>(), GoogleDriveFiles.pickedIds(null))
    }

    @Test
    fun aPickLinksByDrivesOwnAddress() {
        val sheet = GoogleDriveFiles.read(
            "1",
            """{"name":"Budget","mimeType":"application/vnd.google-apps.spreadsheet","webViewLink":"https://docs.google.com/spreadsheets/d/1/edit"}""",
        )
        assertEquals(PickedDriveFile("https://docs.google.com/spreadsheets/d/1/edit", "Budget", "application/vnd.google-apps.spreadsheet"), sheet)
        val folder = GoogleDriveFiles.read("2", """{"name":"Trips","mimeType":"application/vnd.google-apps.folder"}""")
        assertEquals("https://drive.google.com/drive/folders/2", folder?.url)
        val pdf = GoogleDriveFiles.read("3", """{"name":"","webViewLink":"javascript:x"}""")
        assertEquals("https://drive.google.com/file/d/3/view", pdf?.url)
        assertEquals("Untitled", pdf?.name)
        assertNull(GoogleDriveFiles.read("../x", "{}"))
        assertNull(GoogleDriveFiles.read("4", "nope"))
        assertFalse(GoogleDriveFiles.isId("é1"))
    }
}
