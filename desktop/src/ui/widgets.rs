use super::*;

/// What a drawn field needs to take the input method's text: the window's
/// view and its focus.
pub(super) type InputHook<V = Takt> = (gpui::WeakEntity<V>, gpui::FocusHandle);

/// The field's text with its caret, selection and what is being composed,
/// or the placeholder. With a hook, the field registers itself with the
/// input method while it is drawn.
pub(super) fn field_text<V: EntityInputHandler>(
    field: &TextField,
    placeholder: &str,
    t: Tokens,
    hook: Option<InputHook<V>>,
) -> AnyElement {
    let caret = || div().w(px(1.5)).h(px(15.)).flex_none().bg(t.accent);
    let body = if field.text.is_empty() {
        div().flex().items_center().child(caret()).child(
            div()
                .text_color(t.placeholder)
                .child(placeholder.to_string()),
        )
    } else {
        let selection = field.selection();
        let marked = field.marked.clone().unwrap_or(0..0);
        let mut lines = Vec::new();
        let mut line_start = 0;
        for line in field.text.split('\n') {
            let line_end = line_start + line.len();
            let mut cuts = vec![
                line_start,
                line_end,
                selection.start,
                selection.end,
                marked.start,
                marked.end,
                field.cursor,
            ];
            cuts.retain(|cut| (line_start..=line_end).contains(cut));
            cuts.sort_unstable();
            cuts.dedup();
            let mut pieces: Vec<AnyElement> = Vec::new();
            for pair in cuts.windows(2) {
                let (from, to) = (pair[0], pair[1]);
                if from == field.cursor && selection.is_empty() {
                    pieces.push(caret().into_any_element());
                }
                let selected = from >= selection.start && to <= selection.end && from < to;
                let composing = from >= marked.start && to <= marked.end && from < to;
                pieces.push(
                    div()
                        .when(selected, |d| d.bg(t.accent.opacity(0.3)))
                        .when(composing, |d| d.underline())
                        .child(SharedString::from(field.text[from..to].to_string()))
                        .into_any_element(),
                );
            }
            if field.cursor == line_end && selection.is_empty() {
                pieces.push(caret().into_any_element());
            }
            if pieces.is_empty() {
                pieces.push(div().h(px(18.)).into_any_element());
            }
            lines.push(
                div()
                    .flex()
                    .items_center()
                    .whitespace_nowrap()
                    .min_h(px(18.))
                    .children(pieces)
                    .into_any_element(),
            );
            line_start = line_end + 1;
        }
        div().flex().flex_col().children(lines)
    };
    let Some((view, focus)) = hook else {
        return body.into_any_element();
    };
    div()
        .relative()
        .child(body)
        .child(
            gpui::canvas(
                |bounds, _, _| bounds,
                move |bounds, _, window, cx| {
                    if let Some(view) = view.upgrade() {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, view),
                            cx,
                        );
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
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
    title_greyed(task, false, t)
}

/// The title, stepped back when the task is blocked or buried.
pub(super) fn title_greyed(task: &TaskRow, greyed: bool, t: Tokens) -> impl IntoElement {
    let done = task.status != "open";
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when(done, |d| d.text_color(t.placeholder).line_through())
        .when(greyed && !done, |d| d.text_color(t.placeholder))
        .child(task.title.clone())
}

/// What the details say about a task: priority, waiting on, tags, a link,
/// blocked or buried.
pub(super) fn detail_chips(app: &crate::state::App, task: &TaskRow, t: Tokens) -> Vec<AnyElement> {
    let mut chips: Vec<AnyElement> = Vec::new();
    let small = |text: String, colour: Hsla| {
        div()
            .text_size(px(11.))
            .text_color(colour)
            .flex_none()
            .child(text)
            .into_any_element()
    };
    if app.details.blocked.contains(&task.id) && task.status == "open" {
        chips.push(small("blocked".into(), t.warning));
    } else if app.details.buried.contains(&task.id) && task.status == "open" {
        chips.push(small("later".into(), t.placeholder));
    }
    if let Some(meta) = app.meta(&task.id) {
        if let Some(who) = &meta.waiting_on {
            chips.push(small(format!("⌛ {who}"), t.muted));
        }
        for tag in meta.tags.iter().take(3) {
            chips.push(small(format!("#{tag}"), t.accent));
        }
        if meta.recurrence.is_some() {
            chips.push(small("↻".into(), t.placeholder));
        }
        if meta.linked {
            chips.push(small("↗".into(), t.placeholder));
        }
        if let Some(priority) = meta.priority {
            let colour = match priority {
                1 => t.error,
                2 => t.warning,
                p if p < 0 => t.placeholder,
                _ => t.muted,
            };
            let label = if priority < 0 {
                format!("↓{}", -priority)
            } else {
                format!("P{priority}")
            };
            chips.push(small(label, colour));
        }
    }
    chips
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
