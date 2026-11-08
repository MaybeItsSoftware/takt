//! The settings page (⌘,), laid out as Zed's: a search and the categories
//! down the left, and on the right the chosen category's settings, each a
//! title, a line on what it does, and its control. The rows and what a
//! change does are `state::settings`'; this draws them and takes the keys.
//!
//! It is a window of its own, as Zed's is, but a second window onto the
//! same `Takt` entity rather than a separate view: its keys run on the same
//! `App`, and the pickers and fields a setting opens (a theme, a server
//! address, a shortcut being recorded) come up over it while it is in front.

use super::icons::icon;
use super::widgets::field_text;
use super::{MONO_FONT, Takt, Tokens};
use crate::state::settings::{Control, SettingRow};
use crate::text_field::{FieldKey, TextField};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, IntoElement, ParentElement, SharedString, Styled,
    div, prelude::*, px,
};

impl Takt {
    /// ⌘,: the page, its search empty, where it was left. The window
    /// follows in `open_requested_windows`.
    pub(super) fn open_settings_page(&mut self) {
        if self.app.settings_page.is_none() {
            self.settings_field = TextField::default();
        }
        self.app.overlay = None;
        self.app.open_overlay(crate::state::Overlay::Settings);
        self.settings_wanted_front = true;
    }

    pub(super) fn is_settings_window(&self, window: &gpui::Window) -> bool {
        self.settings_window
            .is_some_and(|handle| handle.window_id() == window.window_handle().window_id())
    }

    /// The window kept to the state: opened (or brought forward) while the
    /// page is open, closed once it isn't.
    pub(super) fn sync_settings_window(&mut self, cx: &mut Context<Self>) {
        let wanted = self.app.settings_page.is_some();
        let front = std::mem::take(&mut self.settings_wanted_front);
        match (wanted, self.settings_window) {
            (true, Some(handle)) => {
                if front {
                    let _ = handle.update(cx, |_, window, _| window.activate_window());
                }
            }
            (true, None) => {
                // Once this update ends: the window's first draw reads this
                // entity.
                let takt = cx.entity();
                cx.defer(move |cx| open_window(&takt, cx));
            }
            (false, Some(handle)) => {
                self.settings_window = None;
                self.settings_active = false;
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                });
            }
            (false, None) => {}
        }
    }

    /// A key while the page is up and nothing is open over it. False for a
    /// printable key, which the input method types into the search.
    pub(super) fn settings_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let query = self.settings_field.text.clone();
        if keystroke.modifiers.control {
            match keystroke.key.as_str() {
                "n" | "j" => {
                    self.app.step_settings_cursor(1, &query);
                    self.reveal_settings_cursor();
                    return true;
                }
                "p" | "k" => {
                    self.app.step_settings_cursor(-1, &query);
                    self.reveal_settings_cursor();
                    return true;
                }
                _ => {}
            }
        }
        let outcome = self.settings_field.handle(keystroke);
        if Self::clipboard_key(&mut self.settings_field, &outcome, cx) {
            self.settings_searched();
            return true;
        }
        match outcome {
            FieldKey::Text(_) => return false,
            FieldKey::Copy | FieldKey::Cut | FieldKey::Paste => {}
            FieldKey::Commit => {
                self.app.change_settings_cursor(&query);
                if self.app.overlay.is_some() {
                    self.open_overlay_field();
                }
            }
            FieldKey::Cancel => {
                if query.is_empty() {
                    self.app.close_settings_page();
                } else {
                    self.settings_field = TextField::default();
                    self.settings_searched();
                }
            }
            FieldKey::Down => {
                self.app.step_settings_cursor(1, &query);
                self.reveal_settings_cursor();
            }
            FieldKey::Up => {
                self.app.step_settings_cursor(-1, &query);
                self.reveal_settings_cursor();
            }
            FieldKey::Tab { shift } => {
                self.settings_field = TextField::default();
                self.app.step_settings_category(if shift { -1 } else { 1 });
                self.settings_scroll.set_offset(Default::default());
            }
            FieldKey::Edited => self.settings_searched(),
            // ⌘K and the like still reach the commands behind the page.
            FieldKey::Ignored if keystroke.modifiers.platform => {
                return self.command_key(keystroke, cx);
            }
            FieldKey::Ignored => {}
        }
        true
    }

    /// `TAKT_DESKTOP_SMOKE_SETTINGS`: ⌘, through the window's own key
    /// handler, a walk through the categories with Tab, a search, and a
    /// switch flipped there and back. What it saw, for the log.
    pub fn smoke_settings(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Vec<String> {
        let press =
            |this: &mut Self, key: &str, window: &mut gpui::Window, cx: &mut Context<Self>| {
                let event = gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).expect("a key"),
                    is_held: false,
                };
                // ⌘, through the window's own handler; the rest as the
                // settings window's handler takes them, which is not open
                // until this update ends.
                if key.starts_with("cmd-") {
                    this.key_down(&event, window, cx);
                } else {
                    this.settings_key(&event.keystroke, cx);
                }
            };
        let mut said = Vec::new();
        self.app.overlay = None;
        self.app.editing = None;
        press(self, "cmd-,", window, cx);
        said.push(format!("open {}", self.app.settings_page.is_some()));
        let categories = self.app.settings_categories().len();
        let mut seen = Vec::new();
        for _ in 0..categories {
            seen.push(format!(
                "{}:{}",
                self.app.settings_category(),
                self.app.settings_shown("").len()
            ));
            press(self, "tab", window, cx);
        }
        said.push(seen.join(" "));
        self.settings_field = TextField::new("sound");
        self.settings_searched();
        let found: Vec<String> = self
            .app
            .settings_shown("sound")
            .iter()
            .map(|row| row.id.clone())
            .collect();
        said.push(format!("search sound: {}", found.join(" ")));
        if let Some(ix) = found.iter().position(|id| id == "focus-sound") {
            let before = self.app.prefs.focus_sound;
            for _ in 0..ix {
                press(self, "down", window, cx);
            }
            press(self, "enter", window, cx);
            let flipped = self.app.prefs.focus_sound != before;
            press(self, "enter", window, cx);
            said.push(format!(
                "flip {flipped}, back {}, still open {}",
                self.app.prefs.focus_sound == before,
                self.app.settings_page.is_some()
            ));
        }
        said
    }

    /// The second half, once the window is up: Esc twice through the
    /// settings window's own handler clears the search and closes it.
    pub fn smoke_settings_close(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> String {
        let in_window = self.is_settings_window(window);
        for _ in 0..2 {
            let event = gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("escape").expect("a key"),
                is_held: false,
            };
            self.key_down(&event, window, cx);
        }
        format!(
            "window up {in_window}, closed by esc {}",
            self.app.settings_page.is_none() && self.settings_window.is_none()
        )
    }

    pub fn settings_window_handle(&self) -> Option<gpui::WindowHandle<Takt>> {
        self.settings_window
    }

    /// The search changed: what it finds, from the top.
    pub(super) fn settings_searched(&mut self) {
        if let Some(page) = self.app.settings_page.as_mut() {
            page.cursor = 0;
        }
        self.settings_scroll.set_offset(Default::default());
    }

    fn reveal_settings_cursor(&self) {
        if let Some(page) = &self.app.settings_page {
            self.settings_scroll.scroll_to_item(page.cursor);
        }
    }

    /// A setting clicked: it is the one under the keyboard, and it changes.
    fn settings_clicked(&mut self, ix: usize, id: &str, cx: &mut Context<Self>) {
        if let Some(page) = self.app.settings_page.as_mut() {
            page.cursor = ix;
        }
        let editing = self.app.editing.clone();
        self.app.change_setting(id);
        if self.app.overlay.is_some() {
            self.open_overlay_field();
        }
        if self.app.editing != editing {
            self.begin_editing();
        }
        self.after_change(cx);
        self.open_requested_windows(cx);
        cx.notify();
    }

    pub(super) fn settings_page(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-page")
            .flex_1()
            .min_h_0()
            .flex()
            .bg(t.background)
            .child(self.settings_nav(t, cx))
            .child(self.settings_content(t, cx))
    }

    fn settings_nav(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.settings_field.text.trim().is_empty();
        let current = self.app.settings_category();
        let categories = self.app.settings_categories();
        let hint = |key: &'static str, what: &'static str| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .px_1()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(t.border_variant)
                        .font_family(MONO_FONT)
                        .text_size(px(11.))
                        .text_color(t.muted)
                        .child(key),
                )
                .child(
                    div()
                        .font_family(MONO_FONT)
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(what),
                )
        };
        div()
            .w(px(240.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
            .child(
                div().px_2().pb_2().pt(px(40.)).child(
                    div()
                        .h(px(30.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(t.accent)
                        .bg(t.background)
                        .child(icon("search", 14.).text_color(t.muted))
                        .child(div().flex_1().min_w_0().overflow_hidden().child(field_text(
                            &self.settings_field,
                            "Search settings…",
                            t,
                            self.input_hook(),
                        ))),
                ),
            )
            .child(
                div()
                    .id("settings-categories")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(categories.into_iter().map(|(name, glyph)| {
                        let chosen = !searching && name == current;
                        div()
                            .id(SharedString::from(format!("settings-category-{name}")))
                            .h(px(28.))
                            .px_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(4.))
                            .when(chosen, |d| d.bg(t.selected))
                            .when(!chosen, |d| d.hover(|d| d.bg(t.hover)))
                            .child(icon(glyph, 14.).text_color(if chosen {
                                t.text
                            } else {
                                t.muted
                            }))
                            .child(
                                div()
                                    .text_color(if chosen { t.text } else { t.muted })
                                    .child(name),
                            )
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.settings_field = TextField::default();
                                this.app.choose_settings_category(name);
                                this.settings_scroll.set_offset(Default::default());
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(t.border_variant)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(hint("tab", "Next category"))
                    .child(hint("esc", "Close settings")),
            )
    }

    fn settings_content(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.settings_field.text.clone();
        let searching = !query.trim().is_empty();
        let rows = self.app.settings_shown(&query);
        let cursor = self
            .app
            .settings_page
            .as_ref()
            .map_or(0, |p| p.cursor)
            .min(rows.len().saturating_sub(1));
        let heading: SharedString = if searching {
            match rows.len() {
                0 => "Nothing found".into(),
                1 => "1 setting".into(),
                n => format!("{n} settings").into(),
            }
        } else {
            self.app.settings_category().into()
        };
        let body: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.setting_row(ix, ix == cursor, row, searching, t, cx))
            .collect();
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_8()
                    .pt(px(40.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .px_1()
                                    .rounded(px(4.))
                                    .bg(t.hover)
                                    .text_color(t.muted)
                                    .child("Settings"),
                            )
                            .child(div().text_color(t.text).child("This Mac")),
                    ),
            )
            .child(
                div()
                    .px_8()
                    .pt_4()
                    .pb_2()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(heading),
                    )
                    // What a change said: the status bar is behind the page.
                    .when_some(self.app.message.clone(), |d, said| {
                        d.child(div().pt_1().text_color(t.accent).child(said))
                    }),
            )
            .child(
                div()
                    .id("settings-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_scroll)
                    .px_6()
                    .pb_6()
                    .flex()
                    .flex_col()
                    .children(body),
            )
    }

    fn setting_row(
        &self,
        ix: usize,
        here: bool,
        row: &SettingRow,
        searching: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id.clone();
        let description = row.description();
        div()
            .id(SharedString::from(format!("setting-{}", row.id)))
            .px_2()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_6()
            .border_b_1()
            .border_color(t.border_variant)
            .relative()
            .when(here, |d| {
                d.bg(t.hover).child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(t.accent),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings_clicked(ix, &id, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .when(searching, |d| {
                        d.child(
                            div()
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(t.placeholder)
                                .child(row.group.to_uppercase()),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(t.text)
                            .child(row.title.clone()),
                    )
                    .when(!description.is_empty(), |d| {
                        d.child(div().text_color(t.muted).child(description))
                    }),
            )
            .child(control(row.control(), t))
            .into_any_element()
    }
}

/// A setting's control, drawn; the row takes the click.
fn control(control: Control, t: Tokens) -> AnyElement {
    let note = |text: String| {
        div()
            .max_w(px(260.))
            .truncate()
            .text_color(t.muted)
            .child(text)
    };
    let button = || {
        div()
            .h(px(26.))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border)
            .bg(t.surface)
            .hover(|d| d.bg(t.hover))
    };
    let side = div().flex_none().flex().items_center().gap_3();
    match control {
        Control::Switch { on, note: said } => side
            .when(!said.is_empty(), |d| d.child(note(said)))
            .child(
                div()
                    .w(px(34.))
                    .h(px(20.))
                    .p(px(2.))
                    .flex()
                    .items_center()
                    .when(on, |d| d.justify_end())
                    .rounded_full()
                    .border_1()
                    .border_color(if on { t.accent } else { t.border })
                    .bg(if on { t.accent } else { t.hover })
                    .child(div().size(px(14.)).rounded_full().bg(if on {
                        t.background
                    } else {
                        t.muted
                    })),
            )
            .into_any_element(),
        Control::Button { label, note: said } => side
            .when(!said.is_empty(), |d| d.child(note(said)))
            .child(button().child(label))
            .into_any_element(),
        Control::Choice(value) => side
            .child(
                button()
                    .max_w(px(320.))
                    .child(div().truncate().child(if value.is_empty() {
                        "Choose…".to_string()
                    } else {
                        value
                    }))
                    .child(icon("chevrons-up-down", 12.).text_color(t.muted)),
            )
            .into_any_element(),
    }
}

/// Opens the settings window onto `takt`, in front, with the keyboard.
fn open_window(takt: &gpui::Entity<Takt>, cx: &mut gpui::App) {
    let bounds = gpui::Bounds::centered(None, gpui::size(px(960.), px(680.)), cx);
    let root = takt.clone();
    let Ok(handle) = cx.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: true,
                traffic_light_position: Some(gpui::point(px(14.), px(14.))),
            }),
            window_min_size: Some(gpui::size(px(640.), px(420.))),
            ..Default::default()
        },
        move |_, _| root,
    ) else {
        return;
    };
    let focus = takt.read(cx).focus.clone();
    takt.update(cx, |this, _| {
        this.settings_window = Some(handle);
        this.settings_active = true;
    });
    let _ = handle.update(cx, |_, window, cx| {
        window.focus(&focus);
        cx.observe_window_activation(window, |this, window, cx| {
            this.settings_active = window.is_window_active();
            cx.notify();
        })
        .detach();
    });
    cx.activate(true);
    takt.update(cx, |_, cx| cx.notify());
}
