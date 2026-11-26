//! The Sync page drawn as an account screen rather than a list: which
//! server (Takt Sync or your own) as two cards to choose between, then a
//! sign-in card or, signed in, the account's. Each control stands for one
//! of the page's setting rows, so ↑↓, Return, the palette and search reach
//! exactly what they reach on any other page; only the drawing differs.

use super::icons::icon;
use super::{Takt, Tokens};
use crate::state::settings::SettingRow;
use crate::ui::zoom::px;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};

/// The group the page's own rows are in; the shared lists follow as rows.
pub const SYNC: &str = "Sync";

impl Takt {
    /// The Sync rows, `(index on the page, row)`, as cards; `cursor` is the
    /// row under the keyboard.
    pub(super) fn sync_panel(
        &self,
        rows: &[(usize, &SettingRow)],
        cursor: usize,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let find = |id: &str| find(rows, id);
        let hosted = self.app.sync_hosted();
        let mut panel = div().pt_4().flex().flex_col().gap_4();

        if let Some((ix, _)) = find("sync-self-hosted") {
            let here = ix == cursor;
            let card = |own: bool, cx: &mut Context<Self>| {
                let chosen = own != hosted;
                let (glyph, title, line) = if own {
                    (
                        "server",
                        "Your own server",
                        "Run sync-server yourself. Free, with every feature.",
                    )
                } else {
                    (
                        "cloud",
                        "Takt Sync",
                        "Takt's server. £1 a year, one purchase for every device.",
                    )
                };
                div()
                    .id(SharedString::from(format!("sync-profile-{own}")))
                    .flex_1()
                    .min_w_0()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if chosen { t.accent } else { t.border })
                    .bg(if chosen { t.surface } else { t.background })
                    .when(!chosen, |d| d.hover(|d| d.bg(t.hover)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(icon(glyph, 18.).text_color(if chosen {
                                t.accent
                            } else {
                                t.muted
                            }))
                            .child(radio(chosen, t)),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .child(title),
                    )
                    .child(div().text_size(px(12.)).text_color(t.muted).child(line))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if chosen {
                            if let Some(page) = this.app.settings_page.as_mut() {
                                page.cursor = ix;
                            }
                            cx.notify();
                        } else {
                            this.settings_clicked(ix, "sync-self-hosted", cx);
                        }
                    }))
            };
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(micro("Sync with", t))
                    .child(
                        div()
                            .p(px(3.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(if here {
                                t.focus
                            } else {
                                gpui::transparent_black()
                            })
                            .flex()
                            .gap_3()
                            .child(card(false, cx))
                            .child(card(true, cx)),
                    ),
            );
        }

        let server = find("sync-server");
        let signed_in = find("sync-now").is_some();
        let mut account = div()
            .rounded(px(8.))
            .border_1()
            .border_color(t.border)
            .bg(t.surface)
            .flex()
            .flex_col();

        if let Some((ix, row)) = server {
            account = account.child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(icon("server", 14.).text_color(t.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if row.value.is_empty() {
                                t.placeholder
                            } else {
                                t.text
                            })
                            .child(if row.value.is_empty() {
                                "No address yet".to_string()
                            } else {
                                row.value.clone()
                            }),
                    )
                    .child(button(
                        "Change",
                        ix,
                        "sync-server",
                        ix == cursor,
                        Tone::Plain,
                        t,
                        cx,
                    )),
            );
        }

        if signed_in {
            account = account.child(self.signed_in(rows, cursor, t, cx));
        } else {
            account = account.child(self.sign_in(rows, hosted, cursor, t, cx));
        }
        panel.child(account).into_any_element()
    }

    fn sign_in(
        &self,
        rows: &[(usize, &SettingRow)],
        hosted: bool,
        cursor: usize,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let find = |id: &str| find(rows, id);
        let status = find("sync-google")
            .map(|(_, r)| r.value.clone())
            .filter(|v| !v.is_empty() && v != "Off");
        let wide = |label: &'static str,
                    glyph: Option<&'static str>,
                    id: &'static str,
                    cx: &mut Context<Self>| {
            find(id).map(|(ix, _)| {
                let here = ix == cursor;
                div()
                    .id(SharedString::from(format!("sync-{id}")))
                    .h(px(38.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if here { t.accent } else { t.border })
                    .bg(t.background)
                    .hover(|d| d.bg(t.hover))
                    .when_some(glyph, |d, glyph| {
                        d.child(icon(glyph, 14.).text_color(t.muted))
                    })
                    .child(
                        div()
                            .text_color(t.text)
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.settings_clicked(ix, id, cx);
                    }))
            })
        };
        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t.text)
                            .child(if hosted {
                                "Sign in to Takt Sync"
                            } else {
                                "Sign in to your server"
                            }),
                    )
                    .child(div().text_size(px(13.)).text_color(t.muted).child(
                        "One workspace on this Mac, your iPhone and Android, kept in step as you work.",
                    )),
            )
            .when_some(status, |d, status| {
                d.child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(t.warning.opacity(0.4))
                        .bg(t.warning.opacity(0.1))
                        .text_size(px(12.))
                        .text_color(t.text)
                        .child(status),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(wide("Continue with Apple", None, "sync-apple", cx))
                    .children(wide("Continue with Google", None, "sync-google", cx))
                    .children(wide("Continue with email", Some("mail"), "sync-email", cx)),
            )
            .child(div().text_size(px(12.)).text_color(t.muted).child(
                "This is your Takt account. Google Calendar, Tasks and Drive connect separately, under Plugins → Google.",
            ))
            .child(
                div()
                    .pt_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .children(find("sync-create").map(|(ix, _)| {
                        link("Create an account", ix, "sync-create", ix == cursor, t, cx)
                    }))
                    .children(find("sync-reset").map(|(ix, _)| {
                        link("Forgot your password?", ix, "sync-reset", ix == cursor, t, cx)
                    })),
            )
            .into_any_element()
    }

    fn signed_in(
        &self,
        rows: &[(usize, &SettingRow)],
        cursor: usize,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let find = |id: &str| find(rows, id);
        let status = self.app.sync.status();
        let email = status.email.clone().unwrap_or_else(|| "Signed in".into());
        let (now_ix, now_row) = find("sync-now").expect("signed in has sync-now");
        let state = now_row
            .value
            .split_once(" · ")
            .map_or(now_row.value.clone(), |(_, phase)| phase.to_string());
        let initial = email
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        let line = |label: &'static str,
                    value: String,
                    action: Option<(&'static str, &'static str)>,
                    cx: &mut Context<Self>| {
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .gap_4()
                .border_t_1()
                .border_color(t.border_variant)
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(t.muted)
                        .child(label),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(t.text)
                        .child(value),
                )
                .children(action.and_then(|(label, id)| {
                    find(id).map(|(ix, _)| button(label, ix, id, ix == cursor, Tone::Plain, t, cx))
                }))
        };
        let mut card = div().flex().flex_col().child(
            div()
                .p_4()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .size(px(36.))
                        .flex_none()
                        .rounded_full()
                        .bg(t.accent.opacity(0.15))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(t.accent)
                        .font_weight(FontWeight::MEDIUM)
                        .child(initial),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .truncate()
                                .text_color(t.text)
                                .font_weight(FontWeight::MEDIUM)
                                .child(email),
                        )
                        .child(div().text_size(px(12.)).text_color(t.muted).child(state)),
                )
                .child(button(
                    "Sync now",
                    now_ix,
                    "sync-now",
                    now_ix == cursor,
                    Tone::Accent,
                    t,
                    cx,
                )),
        );
        if let Some((_, row)) = find("sync-devices") {
            card = card.child(line(
                "Devices",
                row.value.clone(),
                Some(("Refresh", "sync-devices")),
                cx,
            ));
        }
        if let Some((_, row)) = find("sync-subscription") {
            card = card.child(line(
                "Takt Sync",
                row.value.clone(),
                Some(("Refresh", "sync-subscription")),
                cx,
            ));
        }
        if let Some((_, row)) = find("sync-new-password") {
            let label = if row.title.starts_with("Choose") {
                "Choose"
            } else {
                "Change"
            };
            card = card.child(line(
                "Password",
                "••••••••".into(),
                Some((label, "sync-new-password")),
                cx,
            ));
        }
        card.child(
            div()
                .px_4()
                .py_3()
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(t.border_variant)
                .children(find("sync-sign-out").map(|(ix, _)| {
                    button(
                        "Sign out",
                        ix,
                        "sync-sign-out",
                        ix == cursor,
                        Tone::Plain,
                        t,
                        cx,
                    )
                }))
                .children(find("sync-delete").map(|(ix, _)| {
                    button(
                        "Delete account…",
                        ix,
                        "sync-delete",
                        ix == cursor,
                        Tone::Danger,
                        t,
                        cx,
                    )
                })),
        )
        .into_any_element()
    }
}

/// The page's row `id`, with its index.
fn find<'a>(rows: &[(usize, &'a SettingRow)], id: &str) -> Option<(usize, &'a SettingRow)> {
    rows.iter().find(|(_, r)| r.id == id).copied()
}

#[derive(Clone, Copy)]
enum Tone {
    Plain,
    Accent,
    Danger,
}

fn button(
    label: &'static str,
    ix: usize,
    id: &'static str,
    here: bool,
    tone: Tone,
    t: Tokens,
    cx: &mut Context<Takt>,
) -> gpui::Stateful<gpui::Div> {
    let colour = match tone {
        Tone::Plain => t.text,
        Tone::Accent => t.accent,
        Tone::Danger => t.error,
    };
    div()
        .id(SharedString::from(format!("sync-button-{id}")))
        .flex_none()
        .h(px(28.))
        .px_3()
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(if here {
            t.focus
        } else {
            match tone {
                Tone::Plain => t.border,
                _ => colour.opacity(0.4),
            }
        })
        .bg(match tone {
            Tone::Plain => t.background,
            _ => colour.opacity(0.1),
        })
        .hover(|d| d.bg(t.hover))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colour)
        .child(label)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.settings_clicked(ix, id, cx);
        }))
}

fn link(
    label: &'static str,
    ix: usize,
    id: &'static str,
    here: bool,
    t: Tokens,
    cx: &mut Context<Takt>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(format!("sync-link-{id}")))
        .px_1()
        .rounded(px(4.))
        .border_1()
        .border_color(if here {
            t.focus
        } else {
            gpui::transparent_black()
        })
        .text_size(px(12.))
        .text_color(t.muted)
        .hover(|d| d.text_color(t.text))
        .child(label)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.settings_clicked(ix, id, cx);
        }))
}

fn radio(on: bool, t: Tokens) -> gpui::Div {
    div()
        .size(px(14.))
        .rounded_full()
        .border_1()
        .border_color(if on { t.accent } else { t.border })
        .flex()
        .items_center()
        .justify_center()
        .when(on, |d| {
            d.child(div().size(px(6.)).rounded_full().bg(t.accent))
        })
}

fn micro(label: &'static str, t: Tokens) -> gpui::Div {
    div()
        .px_1()
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .text_color(t.placeholder)
        .child(label.to_uppercase())
}
