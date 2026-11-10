//! The "Commit periodically" form: the title, how often, until when and
//! whether missed days stack, as rows of controls. Its keys are
//! `App::commitment_key`'s, and the title row's are the text field's;
//! everything here can be clicked too.

use super::{Editing, MONO_FONT, TITLE_BAR_HEIGHT, Takt, Tokens, field_text};
use crate::keys::{Press, spell};
use crate::state::commitment_form::{
    CommitmentField, CommitmentForm, DAY_SHORT, FormKey, PATTERNS, RepeatPattern,
};
use crate::state::date_edit::today;
use crate::text_field::FieldKey;
use gpui::{AnyElement, ClickEvent, Context, SharedString, div, prelude::*, px};

impl Takt {
    /// A key while the form is open. On the title row the field takes what
    /// it types; ↑ ↓ Tab, Return and Esc are the form's everywhere.
    pub(super) fn commitment_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let on_title = self
            .app
            .commitment_form
            .as_ref()
            .is_some_and(|f| f.field == CommitmentField::Title);
        if on_title {
            let outcome = self.field.handle(keystroke);
            if Self::clipboard_key(&mut self.field, &outcome, cx) {
                return true;
            }
            let by = match outcome {
                FieldKey::Text(_) => return false,
                FieldKey::Commit => {
                    self.save_commitment();
                    return true;
                }
                FieldKey::Cancel => {
                    self.app.close_commitment_form();
                    return true;
                }
                FieldKey::Tab { shift: false } | FieldKey::Down => 1,
                FieldKey::Tab { shift: true } | FieldKey::Up => -1,
                FieldKey::Ignored => 0,
                _ => return true,
            };
            if by != 0 {
                if let Some(form) = self.app.commitment_form() {
                    form.move_field(by);
                }
                return true;
            }
        }
        let Some(key) = spell(&Press::from_keystroke(keystroke)) else {
            return true;
        };
        match self.app.commitment_key(&key) {
            FormKey::Save => self.save_commitment(),
            FormKey::Cancel => self.app.close_commitment_form(),
            FormKey::Handled | FormKey::Ignored => {}
        }
        true
    }

    fn save_commitment(&mut self) {
        let title = self.field.text.clone();
        self.app.save_commitment_form(&title);
    }

    fn commitment_clicked(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut CommitmentForm),
    ) {
        cx.stop_propagation();
        if let Some(form) = self.app.commitment_form() {
            change(form);
        }
        cx.notify();
    }

    pub(super) fn commitment_sheet(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.app.commitment_form.clone() else {
            return div().into_any_element();
        };
        let today = today();
        let chip = move |id: SharedString, label: String, on: bool, focused: bool| {
            div()
                .id(id)
                .px_2()
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if on && focused {
                    t.accent
                } else if on {
                    t.muted
                } else {
                    t.border_variant
                })
                .when(on, |d| d.bg(t.selected))
                .hover(|d| d.bg(t.hover))
                .text_size(px(12.))
                .text_color(if on { t.text } else { t.muted })
                .child(label)
        };
        let row = |field: CommitmentField,
                   label: &'static str,
                   body: AnyElement,
                   cx: &mut Context<Self>| {
            let focused = form.field == field;
            div()
                .id(SharedString::from(format!("commitment-row-{label}")))
                .px_3()
                .py(px(6.))
                .flex()
                .items_center()
                .gap_3()
                .border_l_2()
                .border_color(if focused { t.accent } else { t.surface })
                .when(focused, |d| d.bg(t.hover))
                .child(
                    div()
                        .w(px(64.))
                        .flex_none()
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(if focused { t.text } else { t.muted })
                        .child(label.to_uppercase()),
                )
                .child(div().flex_1().min_w_0().child(body))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| f.field = field);
                }))
        };

        // Title: the window's field while it is the row in focus.
        let title = if form.field == CommitmentField::Title {
            div()
                .h(px(26.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(t.accent)
                .bg(t.background)
                .child(field_text(
                    &self.field,
                    "What turns up in Today",
                    t,
                    self.input_hook(),
                ))
        } else {
            let text = self.field.text.clone();
            div()
                .h(px(26.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border_variant)
                .when(text.trim().is_empty(), |d| {
                    d.text_color(t.placeholder).child("What turns up in Today")
                })
                .when(!text.trim().is_empty(), |d| {
                    d.text_color(t.text).child(text)
                })
        };

        let repeat_focus = form.field == CommitmentField::Repeat;
        let repeat = div()
            .flex()
            .flex_wrap()
            .gap_1()
            .children(PATTERNS.iter().enumerate().map(|(ix, pattern)| {
                let pattern = *pattern;
                chip(
                    SharedString::from(format!("commitment-repeat-{ix}")),
                    form.pattern_label(pattern),
                    form.pattern == pattern,
                    repeat_focus,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| {
                        f.choose(pattern);
                        f.field = CommitmentField::Repeat;
                    });
                }))
            }));

        let days_focus = form.field == CommitmentField::Days;
        let days = div()
            .flex()
            .gap_1()
            .children(DAY_SHORT.iter().enumerate().map(|(ix, name)| {
                chip(
                    SharedString::from(format!("commitment-day-{ix}")),
                    (*name).to_string(),
                    form.days[ix],
                    days_focus,
                )
                .min_w(px(40.))
                .justify_center()
                .when(days_focus && form.day_cursor == ix, |d| {
                    d.border_color(t.accent)
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| {
                        f.toggle_day(ix);
                        f.field = CommitmentField::Days;
                    });
                }))
            }));

        let until_focus = form.field == CommitmentField::Until;
        let step = move |id: &'static str, label: &'static str, by: i64, cx: &mut Context<Self>| {
            div()
                .id(id)
                .w(px(24.))
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(t.border_variant)
                .hover(|d| d.bg(t.hover))
                .text_color(t.muted)
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| {
                        f.step_until(by, today);
                        f.field = CommitmentField::Until;
                    });
                }))
        };
        let until_text = form
            .until
            .map(|d| d.format("%a %-d %b %Y").to_string())
            .unwrap_or_else(|| "Pick a day".into());
        let until = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                chip(
                    "commitment-until-none".into(),
                    "No end".into(),
                    form.until.is_none(),
                    until_focus,
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| {
                        f.until = None;
                        f.field = CommitmentField::Until;
                    });
                })),
            )
            .child(step("commitment-until-back", "\u{2039}", -1, cx))
            .child(
                chip(
                    "commitment-until-day".into(),
                    until_text,
                    form.until.is_some(),
                    until_focus,
                )
                .min_w(px(128.))
                .justify_center()
                .font_family(MONO_FONT)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.commitment_clicked(cx, |f| {
                        if f.until.is_none() {
                            f.step_until(7, today);
                        }
                        f.field = CommitmentField::Until;
                    });
                })),
            )
            .child(step("commitment-until-on", "\u{203a}", 1, cx))
            .when(form.until.is_none(), |d| {
                d.child(
                    div()
                        .pl_1()
                        .text_size(px(12.))
                        .text_color(t.placeholder)
                        .child("runs until the task is done"),
                )
            });

        let stacks_focus = form.field == CommitmentField::Stacks;
        let on = form.stacks;
        let stacks = div()
            .id("commitment-stacks")
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .w(px(26.))
                    .h(px(16.))
                    .p(px(2.))
                    .flex()
                    .items_center()
                    .when(on, |d| d.justify_end())
                    .rounded_full()
                    .border_1()
                    .border_color(if on || stacks_focus {
                        t.accent
                    } else {
                        t.border
                    })
                    .bg(if on { t.accent } else { t.hover })
                    .child(div().size(px(10.)).rounded_full().bg(if on {
                        t.background
                    } else {
                        t.muted
                    })),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(if on { t.text } else { t.muted })
                    .child("A missed day stays owed until done"),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.commitment_clicked(cx, |f| {
                    f.stacks = !f.stacks;
                    f.field = CommitmentField::Stacks;
                });
            }));

        let next: Vec<String> = form
            .upcoming(today, 4)
            .into_iter()
            .map(|d| {
                if d == today {
                    "Today".to_string()
                } else {
                    d.format("%a %-d %b").to_string()
                }
            })
            .collect();
        let reading = form.draft(&self.field.text).err();

        let parent = self
            .app
            .store
            .core
            .task(form.task_id.clone())
            .ok()
            .flatten()
            .map(|t| t.title)
            .unwrap_or_default();

        let hint = |keys: &'static str, what: &'static str| {
            div()
                .flex()
                .gap_1()
                .child(div().font_family(MONO_FONT).text_color(t.muted).child(keys))
                .child(div().text_color(t.placeholder).child(what))
        };
        let changes: &'static str = match form.field {
            CommitmentField::Title => "",
            CommitmentField::Repeat => "\u{2190}\u{2192} choose \u{b7} 0\u{2013}9 count",
            CommitmentField::Days => "\u{2190}\u{2192} day \u{b7} space tick",
            CommitmentField::Until => {
                "\u{2190}\u{2192} a day \u{b7} \u{21e7} a week \u{b7} x no end"
            }
            CommitmentField::Stacks => "space switch",
        };
        let hints = div()
            .flex()
            .flex_wrap()
            .gap_x_3()
            .text_size(px(11.))
            .child(hint("tab \u{2191}\u{2193}", "row"))
            .when(!changes.is_empty(), |d| {
                d.child(
                    div()
                        .font_family(MONO_FONT)
                        .text_color(t.muted)
                        .child(changes),
                )
            })
            .child(hint("\u{21a9}", "save"))
            .when(form.editing.is_some(), |d| {
                d.child(hint("\u{2318}\u{232b}", "stop it"))
            })
            .child(hint("esc", "cancel"));

        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .px_3()
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if primary { t.accent } else { t.border })
                .when(primary, |d| d.bg(t.selected))
                .hover(|d| d.bg(t.hover))
                .text_size(px(12.))
                .text_color(t.text)
                .child(label)
        };

        let sheet = div()
            .id("commitment-sheet")
            .occlude()
            .w(px(560.))
            .flex()
            .flex_col()
            .rounded(px(8.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .justify_between()
                    .items_center()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(div().text_color(t.text).child(if form.editing.is_some() {
                        "Commitment"
                    } else {
                        "Commit periodically"
                    }))
                    .child(
                        div()
                            .max_w(px(280.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(t.muted)
                            .child(format!("for \u{201c}{parent}\u{201d}")),
                    ),
            )
            .child(
                div()
                    .py_1()
                    .flex()
                    .flex_col()
                    .child(row(
                        CommitmentField::Title,
                        "Title",
                        title.into_any_element(),
                        cx,
                    ))
                    .child(row(
                        CommitmentField::Repeat,
                        "Repeat",
                        repeat.into_any_element(),
                        cx,
                    ))
                    .when(form.pattern == RepeatPattern::OnDays, |d| {
                        d.child(row(
                            CommitmentField::Days,
                            "On",
                            days.into_any_element(),
                            cx,
                        ))
                    })
                    .child(row(
                        CommitmentField::Until,
                        "Until",
                        until.into_any_element(),
                        cx,
                    ))
                    .child(row(
                        CommitmentField::Stacks,
                        "Missed",
                        stacks.into_any_element(),
                        cx,
                    )),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .gap_2()
                    .text_size(px(12.))
                    .border_t_1()
                    .border_color(t.border_variant)
                    .child(div().text_color(t.placeholder).child("Next"))
                    .child(match &reading {
                        Some(error) => div().text_color(t.error).child(error.clone()),
                        None if next.is_empty() => div()
                            .text_color(t.muted)
                            .child(format!("{}, counted from the day it was made", form.rule())),
                        None => div().text_color(t.text).child(next.join("  \u{b7}  ")),
                    }),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .border_t_1()
                    .border_color(t.border_variant)
                    .child(hints)
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .gap_1()
                            .child(button("commitment-cancel", "Cancel", false).on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.app.close_commitment_form();
                                    this.begin_editing();
                                    cx.notify();
                                }),
                            ))
                            .child(
                                button("commitment-save", "Save", true).on_click(cx.listener(
                                    |this, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        this.save_commitment();
                                        if !matches!(this.app.editing, Some(Editing::Commitment)) {
                                            this.begin_editing();
                                            this.after_change(cx);
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            );

        div()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 8.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(sheet)
            .into_any_element()
    }
}
