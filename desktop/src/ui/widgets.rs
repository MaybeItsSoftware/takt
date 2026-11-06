use super::*;

/// The field's text with its cursor, or the placeholder.
pub(super) fn field_text(field: &TextField, placeholder: &str, t: Tokens) -> impl IntoElement {
    let cursor = || div().w(px(1.5)).h(px(15.)).bg(t.accent);
    if field.text.is_empty() {
        return div().flex().items_center().child(cursor()).child(
            div()
                .text_color(t.placeholder)
                .child(placeholder.to_string()),
        );
    }
    if field.all_selected {
        return div()
            .flex()
            .items_center()
            .child(div().bg(t.accent.opacity(0.3)).child(field.text.clone()));
    }
    let (before, after) = field.split();
    div()
        .flex()
        .items_center()
        .whitespace_nowrap()
        .child(SharedString::from(before.to_string()))
        .child(cursor())
        .child(SharedString::from(after.to_string()))
}

pub(super) fn check(task: &TaskRow, t: Tokens) -> impl IntoElement {
    let (glyph, colour): (&str, Hsla) = match task.status.as_str() {
        "completed" => ("●", t.success),
        "cancelled" => ("✕", t.placeholder),
        _ if task.item_kind.as_deref() == Some("list") => ("≡", t.muted),
        _ => ("○", t.muted),
    };
    div()
        .w(px(12.))
        .flex_none()
        .text_size(px(11.))
        .text_color(colour)
        .child(glyph)
}

pub(super) fn title(task: &TaskRow, t: Tokens) -> impl IntoElement {
    let done = task.status != "open";
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when(done, |d| d.text_color(t.placeholder).line_through())
        .child(task.title.clone())
}

pub(super) fn task_chips(
    task: &TaskRow,
    counted: Option<(usize, usize)>,
    t: Tokens,
) -> Vec<AnyElement> {
    let mut chips: Vec<AnyElement> = due_chip(task, t).into_iter().collect();
    if let Some((done, total)) = counted.filter(|(_, total)| *total > 0) {
        chips.push(
            div()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .font_family(MONO_FONT)
                .child(format!("{done}/{total}"))
                .into_any_element(),
        );
    }
    if !task.notes.trim().is_empty() {
        chips.push(
            div()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child("¶")
                .into_any_element(),
        );
    }
    chips
}

pub(super) fn due_chip(task: &TaskRow, t: Tokens) -> Option<AnyElement> {
    let due = task.due_at_ms?;
    let overdue = task.status == "open" && due < start_of_today_ms();
    Some(
        div()
            .text_size(px(11.))
            .text_color(if overdue { t.error } else { t.placeholder })
            .child(format_day(due))
            .into_any_element(),
    )
}

pub(super) fn start_of_today_ms() -> i64 {
    use chrono::{Local, TimeZone};
    let midnight = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or(0, |time| time.timestamp_millis())
}

/// "Today", "Tomorrow", "Mon 3 Nov".
pub(super) fn format_day(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    let today = Local::now().date_naive();
    let day = time.date_naive();
    match (day - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        _ => day.format("%a %-d %b").to_string(),
    }
}

pub(super) fn format_duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

pub(super) fn status_title(status: &str) -> &'static str {
    match status {
        "completed" => "Done",
        "cancelled" => "Invalidated",
        _ => "Open",
    }
}

pub(super) fn reason_title(reason: &str) -> &'static str {
    match reason {
        "running" => "running",
        "planned" => "planned",
        "overdue" => "overdue",
        "dueToday" => "due today",
        "startsToday" => "starts today",
        "daily" => "daily",
        "dueSoon" => "due soon",
        "today" => "today",
        "importance" => "important",
        "priority" => "priority",
        "order" => "next in order",
        "condition" => "fits now",
        "started" => "started",
        "deadlineRisk" => "deadline at risk",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(format_duration(25 * 60), "25m");
        assert_eq!(format_duration(2 * 3600), "2h");
        assert_eq!(format_duration(90 * 60), "1h 30m");
    }
}
