//! The repeat selector floating under the inspector's repeat row: "Every ‹3›
//! ‹days›", the units to step through, and when it would next come round.
//! Its keys are `App::repeat_key`'s; everything here can be clicked too.

use super::{MONO_FONT, Takt, Tokens};
use crate::state::repeat::{RepeatEdit, RepeatField, UNITS};
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*, px};

impl Takt {
    fn repeat_clicked(&mut self, cx: &mut Context<Self>) {
        self.after_change(cx);
        cx.notify();
    }

    pub(super) fn repeat_panel(
        &self,
        edit: &RepeatEdit,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let field = |id: &'static str, label: String, focused: bool| {
            div()
                .id(id)
                .px_2()
                .h(px(26.))
                .min_w(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if focused { t.accent } else { t.border_variant })
                .when(focused, |d| d.bg(t.selected))
                .hover(|d| d.bg(t.hover))
                .text_size(px(13.))
                .child(label)
        };
        let counting = edit.unit.counts();
        let mut sentence = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().text_color(t.muted).child("Every"));
        if counting {
            sentence = sentence.child(
                field(
                    "repeat-count",
                    edit.count.to_string(),
                    edit.field == RepeatField::Count,
                )
                .font_family(MONO_FONT)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if let Some(edit) = this.app.repeat_edit() {
                        edit.field = RepeatField::Count;
                    }
                    this.repeat_clicked(cx);
                })),
            );
        }
        sentence = sentence.child(
            field(
                "repeat-unit",
                edit.unit.label(if counting { edit.count } else { 1 }),
                edit.field == RepeatField::Unit,
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if let Some(edit) = this.app.repeat_edit() {
                    edit.field = RepeatField::Unit;
                }
                this.repeat_clicked(cx);
            })),
        );

        let units = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .children(UNITS.iter().enumerate().map(|(ix, unit)| {
                let unit = *unit;
                let on = unit == edit.unit;
                div()
                    .id(("repeat-unit-choice", ix))
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
                    .text_color(if on { t.text } else { t.muted })
                    .child(unit.short())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(edit) = this.app.repeat_edit() {
                            edit.choose(unit);
                        }
                        this.repeat_clicked(cx);
                    }))
            }));

        let due = self
            .app
            .store
            .core
            .task(edit.task_id.clone())
            .ok()
            .flatten()
            .and_then(|task| task.due_at_ms);
        let next: Vec<String> = edit
            .upcoming(due, &self.app.store.zone, 4)
            .into_iter()
            .filter_map(|ms| {
                use chrono::{Local, TimeZone};
                Local.timestamp_millis_opt(ms).single()
            })
            .map(|time| time.format("%a %-d %b").to_string())
            .collect();
        let upcoming = div()
            .flex()
            .gap_2()
            .text_size(px(12.))
            .child(div().text_color(t.placeholder).child("Next"))
            .child(div().text_color(t.text).child(next.join("  \u{b7}  ")));

        let hint = |keys: &'static str, what: &'static str| {
            div()
                .flex()
                .gap_1()
                .child(div().font_family(MONO_FONT).text_color(t.muted).child(keys))
                .child(div().text_color(t.placeholder).child(what))
        };
        let hints = div()
            .flex()
            .flex_wrap()
            .gap_x_3()
            .text_size(px(11.))
            .child(hint("\u{2191}\u{2193}", "change"))
            .when(counting, |d| {
                d.child(hint("\u{2190}\u{2192}", "count / unit"))
            })
            .child(hint("0\u{2013}9", "count"))
            .child(hint("d w", "days, weeks"))
            .child(hint("\u{21a9}", "set"))
            .when(edit.had_rule, |d| {
                d.child(hint("\u{232b}", "stop repeating"))
            })
            .child(hint("esc", "cancel"));

        div()
            .id("repeat-panel")
            .occlude()
            .w(px(300.))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .bg(t.background)
            .child(sentence)
            .child(units)
            .when(!next.is_empty(), |d| d.child(upcoming))
            .child(hints)
            .into_any_element()
    }
}
