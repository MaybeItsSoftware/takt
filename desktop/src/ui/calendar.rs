//! The calendar that floats under the inspector's Start and Due rows: a
//! month of days to click or walk, and under it the time, typed. `Tab`
//! goes from the days to the time (and, for Scheduled, on to how long it
//! takes); its keys are `App::date_key`'s.

use super::icons::icon;
use super::{MONO_FONT, Takt, Tokens};
use crate::state::date_edit::{DEFAULT_START, DateEdit, DateFocus, clock, parse_slot, today};
use crate::state::inspector::parse_span;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*, px};

/// The Monday on or before the first of `day`'s month: where the grid
/// starts.
fn grid_start(day: NaiveDate) -> NaiveDate {
    let first = day.with_day(1).unwrap_or(day);
    first - Duration::days(first.weekday().num_days_from_monday() as i64)
}

/// When a Scheduled slot as typed would end, for the line beside it.
fn slot_end(edit: &DateEdit) -> Option<String> {
    let (start, range) = parse_slot(&edit.time).ok()?;
    let seconds = range.or_else(|| parse_span(&edit.length).filter(|s| *s > 0))?;
    let end = start.unwrap_or(DEFAULT_START) as i64 + seconds / 60;
    (end < 24 * 60).then(|| clock(end as u32))
}

impl Takt {
    /// After a click in the calendar: what any change needs.
    fn calendar_clicked(&mut self, cx: &mut Context<Self>) {
        self.after_change(cx);
        cx.notify();
    }

    pub(super) fn date_panel(
        &self,
        edit: &DateEdit,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = today();
        let shown = edit.day;
        let start = grid_start(shown);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .pb_1()
            .child(
                div()
                    .id("cal-prev")
                    .p_1()
                    .rounded(px(4.))
                    .hover(|d| d.bg(t.hover))
                    .text_color(t.muted)
                    .child(icon("chevron-left", 14.))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.step_date_month(-1);
                        this.calendar_clicked(cx);
                    })),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(shown.format("%B %Y").to_string()),
            )
            .child(
                div()
                    .id("cal-next")
                    .p_1()
                    .rounded(px(4.))
                    .hover(|d| d.bg(t.hover))
                    .text_color(t.muted)
                    .child(icon("chevron-right", 14.))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.step_date_month(1);
                        this.calendar_clicked(cx);
                    })),
            );
        let weekdays = div().flex().children(
            [
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
            ]
            .into_iter()
            .map(|w| {
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .child(w.to_string()[..2].to_string())
            }),
        );
        let weeks = (0..6usize).map(|week| {
            div().flex().children((0..7usize).map(|weekday| {
                let ix = week * 7 + weekday;
                let day = start + Duration::days(ix as i64);
                let chosen = day == shown;
                let in_month = day.month() == shown.month();
                div()
                    .id(("cal-day", ix))
                    .flex_1()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .text_size(px(12.))
                    .text_color(if chosen {
                        t.background
                    } else if in_month {
                        t.text
                    } else {
                        t.placeholder
                    })
                    .when(chosen, |d| d.bg(t.accent))
                    .when(!chosen && day == now, |d| {
                        d.border_1().border_color(t.accent)
                    })
                    .when(!chosen, |d| d.hover(|d| d.bg(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.app.pick_date_day(day);
                        this.calendar_clicked(cx);
                    }))
                    .child(day.day().to_string())
            }))
        });
        let field = |id: &'static str,
                     caption: &'static str,
                     text: &str,
                     placeholder: &'static str,
                     focus: DateFocus| {
            let on = edit.focus == focus;
            let selected = on && edit.fresh && !text.is_empty();
            let value = if text.is_empty() {
                div().text_color(t.placeholder).child(placeholder)
            } else {
                div()
                    .text_color(if selected { t.background } else { t.text })
                    .when(selected, |d| d.bg(t.accent).rounded(px(2.)))
                    .child(text.to_string())
            };
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .h(px(24.))
                .px_2()
                .rounded(px(4.))
                .border_1()
                .border_color(if on { t.accent } else { t.border_variant })
                .when(on, |d| d.bg(t.selected))
                .hover(|d| d.bg(t.hover))
                .child(div().text_size(px(10.)).text_color(t.muted).child(caption))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w(px(44.))
                        .font_family(MONO_FONT)
                        .text_size(px(12.))
                        .child(value)
                        .when(on && !selected, |d| {
                            d.child(div().w(px(1.)).h(px(14.)).bg(t.accent))
                        }),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(edit) = this.app.date_edit() {
                        edit.focus_on(focus);
                    }
                    this.calendar_clicked(cx);
                }))
        };
        let scheduling = edit.scheduling();
        let time_row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_1()
            .pt_2()
            .mt_1()
            .border_t_1()
            .border_color(t.border_variant)
            .child(field(
                "cal-time",
                "Time",
                &edit.time,
                if scheduling { "09:00" } else { "all day" },
                DateFocus::Time,
            ))
            .when(scheduling, |d| {
                d.child(field(
                    "cal-length",
                    "For",
                    &edit.length,
                    "no estimate",
                    DateFocus::Length,
                ))
            })
            .when_some(scheduling.then(|| slot_end(edit)).flatten(), |d, end| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .font_family(MONO_FONT)
                        .text_color(t.muted)
                        .child(format!("until {end}")),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .id("cal-clear")
                    .px_1()
                    .rounded(px(4.))
                    .text_size(px(12.))
                    .text_color(t.error)
                    .hover(|d| d.bg(t.hover))
                    .child("Clear")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.clear_date();
                        this.calendar_clicked(cx);
                    })),
            );
        let hint = (!self.app.prefs.hide_key_hints).then(|| {
            let keys = match edit.focus {
                DateFocus::Day => {
                    "\u{21b5} set  \u{21e5} time  \u{2190}\u{2192}\u{2191}\u{2193} day  [ ] month  t today  \u{232b} clear"
                }
                DateFocus::Time if scheduling => {
                    "\u{21b5} set  \u{21e5} length  \u{2191}\u{2193} \u{b1}15m  9 \u{b7} 9:30 \u{b7} 2pm \u{b7} 9-10:30"
                }
                DateFocus::Time => {
                    "\u{21b5} set  \u{21e7}\u{21e5} days  \u{2191}\u{2193} \u{b1}15m  9 \u{b7} 9:30 \u{b7} 2pm \u{b7} empty all day"
                }
                DateFocus::Length => {
                    "\u{21b5} set  \u{21e7}\u{21e5} time  \u{2191}\u{2193} \u{b1}15m  45 \u{b7} 45m \u{b7} 1h 30m"
                }
            };
            div()
                .pt_2()
                .text_size(px(10.))
                .font_family(MONO_FONT)
                .text_color(t.placeholder)
                .child(keys)
        });
        div()
            .id("date-panel")
            .occlude()
            .w(px(280.))
            .p_2()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .bg(t.background)
            .flex()
            .flex_col()
            .child(header)
            .child(weekdays)
            .children(weeks)
            .child(time_row)
            .children(hint)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_starts_on_the_monday_before_the_first() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        // 1 November 2026 is a Sunday.
        assert_eq!(
            grid_start(day),
            NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
        );
        assert_eq!(clock(9 * 60 + 5), "09:05");
    }
}
