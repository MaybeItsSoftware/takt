package uk.co.maybeitsadam.takt.ui.components

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

/** The app's ways of writing times, durations and dates. The theme's mono face renders the results. */
object Format {
    private val time = DateTimeFormatter.ofPattern("HH:mm", Locale.UK)
    private val dayThisYear = DateTimeFormatter.ofPattern("EEE d MMM", Locale.UK)
    private val dayOtherYear = DateTimeFormatter.ofPattern("d MMM yyyy", Locale.UK)

    /** `25m`, `1h`, `1h 30m`. */
    fun duration(seconds: Int): String {
        val minutes = maxOf(0, seconds) / 60
        if (minutes < 60) return "${minutes}m"
        val rest = minutes % 60
        return if (rest == 0) "${minutes / 60}h" else "${minutes / 60}h ${rest}m"
    }

    /**
     * A typed length of time in seconds, as the Mac reads it: "25" and "25m"
     * are minutes, "1h", "1.5h" and "1h 20m" hours and minutes. Blank is
     * zero; anything else is null.
     */
    fun parseSpan(text: String): Int? {
        val compact = text.trim().lowercase().replace(" ", "")
        if (compact.isEmpty()) return 0
        compact.toDoubleOrNull()?.let { return if (it >= 0) Math.round(it * 60).toInt() else null }
        var seconds = 0.0
        val number = StringBuilder()
        var any = false
        for (c in compact) {
            if (c.isDigit() || c == '.') {
                number.append(c)
                continue
            }
            val value = number.toString().toDoubleOrNull() ?: return null
            number.clear()
            seconds += value * when (c) {
                'h' -> 3600.0
                'm' -> 60.0
                else -> return null
            }
            any = true
        }
        if (number.isNotEmpty()) seconds += (number.toString().toDoubleOrNull() ?: return null) * 60
        return if (any) Math.round(seconds).toInt() else null
    }

    /** `12:05` or `1:02:05` for a running clock. */
    fun clock(seconds: Int): String {
        val s = maxOf(0, seconds)
        val h = s / 3600
        val m = (s % 3600) / 60
        val sec = s % 60
        return if (h > 0) "%d:%02d:%02d".format(h, m, sec) else "%02d:%02d".format(m, sec)
    }

    fun time(instant: Instant, zone: ZoneId = ZoneId.systemDefault()): String = time.format(instant.atZone(zone))

    /** Today, Tomorrow, Yesterday, `Fri 3 Oct`, or `3 Oct 2027`. */
    fun day(instant: Instant, zone: ZoneId = ZoneId.systemDefault(), today: LocalDate = LocalDate.now(zone)): String =
        day(instant.atZone(zone).toLocalDate(), today)

    fun day(date: LocalDate, today: LocalDate = LocalDate.now()): String = when (date) {
        today -> "Today"
        today.plusDays(1) -> "Tomorrow"
        today.minusDays(1) -> "Yesterday"
        else -> (if (date.year == today.year) dayThisYear else dayOtherYear).format(date)
    }

    /** A due date with its time when it has one other than midnight. */
    fun due(instant: Instant, zone: ZoneId = ZoneId.systemDefault()): String {
        val local = instant.atZone(zone)
        val dayText = day(local.toLocalDate(), LocalDate.now(zone))
        return if (local.hour == 0 && local.minute == 0) dayText else "$dayText ${time.format(local)}"
    }
}
