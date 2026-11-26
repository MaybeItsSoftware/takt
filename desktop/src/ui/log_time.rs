//! The minute selector floating under the inspector's Log time row: the
//! minutes with − and + beside them, a row of common lengths, and the keys.
//! Its keys are `App::log_key`'s; everything here can be clicked too.

use super::icons::icon;
use super::{MONO_FONT, Takt, Tokens};
use crate::state::log_edit::{LogEdit, PRESETS, minutes_text};
use crate::ui::zoom::px;
use gpui::{AnyElement, ClickEvent, Context, div, prelude::*};

impl Takt {
    pub(super) fn log_panel(
        &self,
        edit: &LogEdit,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = |id: &'static str| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border_variant)
                .hover(|d| d.bg(t.hover))
        };
        let stepper = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                button("log-less")
                    .child(icon("minus", 13.).text_color(t.muted))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.log_key("down");
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(t.accent)
                    .bg(t.selected)
                    .font_family(MONO_FONT)
                    .text_size(px(14.))
                    .child(edit.label()),
            )
            .child(
                button("log-more")
                    .child(icon("plus", 13.).text_color(t.muted))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.app.log_key("up");
                        cx.notify();
                    })),
            );
        let presets = div().flex().flex_wrap().gap_1().children(PRESETS.map(|m| {
            let on = edit.minutes == m;
            div()
                .id(("log-preset", m as usize))
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
                .child(minutes_text(m))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(edit) = this.app.log_edit_mut() {
                        edit.choose(m);
                    }
                    cx.notify();
                }))
        }));
        let log = div()
            .id("log-commit")
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(t.accent)
            .text_color(t.background)
            .text_size(px(13.))
            .hover(|d| d.opacity(0.9))
            .child(format!("Log {}", edit.label()))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.app.commit_log();
                this.after_change(cx);
                cx.notify();
            }));
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
            .child(hint("\u{2191}\u{2193}", "5 min"))
            .child(hint("\u{21e7}\u{2191}\u{2193}", "15 min"))
            .child(hint("\u{2190}\u{2192}", "presets"))
            .child(hint("0\u{2013}9", "minutes"))
            .child(hint("\u{21a9}", "log"))
            .child(hint("esc", "cancel"));
        div()
            .id("log-panel")
            .occlude()
            .w(px(260.))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .bg(t.background)
            .child(stepper)
            .child(presets)
            .child(log)
            .child(hints)
            .into_any_element()
    }
}
