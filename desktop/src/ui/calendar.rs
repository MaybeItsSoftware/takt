//! The calendar that opens under the inspector's start and due rows: a
//! month of days to click, the days people mostly want as one click, and
//! the times. Nothing in it is typed; its keys are `App::date_key`'s.

use super::icons::icon;
use super::{MONO_FONT, Takt, Tokens};
use crate::state::inspector::{DATE_TIMES, DateEdit, today};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*, px};

fn clock(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The Monday on or before the first of `day`'s month: where the grid
/// starts.
fn grid_start(day: NaiveDate) -> NaiveDate {
    let first = day.with_day(1).unwrap_or(day);
    first - Duration::days(first.weekday().num_days_from_monday() as i64)
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
        let chip = |id: (&'static str, usize), label: String, on: bool| {
            div()
                .id(id)
                .px_2()
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if on { t.accent } else { t.border_variant })
                .when(on, |d| d.bg(t.selected))
                .hover(|d| d.bg(t.hover))
                .text_size(px(12.))
                .child(label)
        };
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
        let next_monday = now + Duration::days(7 - now.weekday().num_days_from_monday() as i64);
        let quick = [
            ("Today", now),
            ("Tomorrow", now + Duration::days(1)),
            ("Next week", next_monday),
        ];
        let quick_row = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .pt_2()
            .children(quick.into_iter().enumerate().map(|(ix, (label, day))| {
                chip(("cal-quick", ix), label.into(), false).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.app.pick_date_day(day);
                        this.calendar_clicked(cx);
                    },
                ))
            }))
            .child(
                chip(("cal-clear", 0), "Clear".into(), false)
                    .text_color(t.error)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.clear_date();
                        this.calendar_clicked(cx);
                    })),
            );
        let mut times: Vec<Option<u32>> = Vec::new();
        if edit.stop == "due" {
            times.push(None);
        }
        times.extend(DATE_TIMES.iter().copied().map(Some));
        if let Some(own) = edit.time
            && !DATE_TIMES.contains(&own)
        {
            times.push(Some(own));
            times.sort_by_key(|t| t.unwrap_or(0));
        }
        let time_row =
            div()
                .flex()
                .flex_wrap()
                .gap_1()
                .pt_1()
                .children(times.into_iter().enumerate().map(|(ix, time)| {
                    let label = time.map(clock).unwrap_or_else(|| "Any time".into());
                    chip(("cal-time", ix), label, edit.time == time)
                        .font_family(MONO_FONT)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.app.pick_date_time(time);
                            this.calendar_clicked(cx);
                        }))
                }));
        let hint = (!self.app.prefs.hide_key_hints).then(|| {
            div()
                .pt_2()
                .text_size(px(10.))
                .font_family(MONO_FONT)
                .text_color(t.placeholder)
                .child("↵ set  ←→↑↓ day  [ ] month  -/= time  t today  ⌫ clear")
        });
        div()
            .id("date-panel")
            .mt_1()
            .mb_1()
            .ml(px(22.))
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
            .child(quick_row)
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
