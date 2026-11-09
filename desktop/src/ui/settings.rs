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
use crate::platform::hotkeys::Hotkey;
use crate::state::Editing;
use crate::state::plugins::{GOOGLE_ACCOUNT, GOOGLE_SWITCH, Plugin};
use crate::state::settings::{self as page, Control, SettingRow, SettingsMenu};
use crate::text_field::{FieldKey, TextField};
use gpui::{
    AnyElement, ClickEvent, Context, Corner, FontWeight, IntoElement, ParentElement, SharedString,
    Styled, anchored, deferred, div, point, prelude::*, px,
};
use std::time::{Duration, Instant};

/// The width every dropdown and text control on the page is drawn at, so
/// their values line up down the page.
const CONTROL_WIDTH: f32 = 220.;
/// A control's height: buttons, dropdowns, fields and shortcuts alike.
const CONTROL_HEIGHT: f32 = 26.;
const MENU_ROW: f32 = 26.;

/// The open dropdown's side of the view: its filter, where its options
/// are scrolled, and which one a click outside just closed (so the click
/// that lands on its own control doesn't open it again).
#[derive(Default)]
pub(super) struct MenuView {
    pub(super) field: TextField,
    scroll: gpui::ScrollHandle,
    dismissed: Option<(String, Instant)>,
}

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
        if self.app.setting_menu().is_some() {
            return self.settings_menu_key(keystroke, cx);
        }
        let query = self.settings_field.text.clone();
        // ⌘1–⌘9 a category, ⌥⌘1–⌥⌘9 a plugin's page, ⇧⌥⌘1–⇧⌥⌘9 its
        // switch.
        let m = &keystroke.modifiers;
        if m.platform
            && m.alt
            && m.shift
            && !m.control
            && let Some(digit) = digit_key(keystroke)
            && self.app.toggle_plugin_by_key(digit)
        {
            return true;
        }
        if m.platform
            && !m.control
            && !m.shift
            && let Some(digit) = digit_key(keystroke)
            && self.app.choose_settings_by_key(digit, m.alt)
        {
            self.settings_field = TextField::default();
            self.settings_searched();
            self.settings_scroll.set_offset(Default::default());
            return true;
        }
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
                let before = self.app.setting_menu().cloned();
                self.app.change_settings_cursor(&query);
                self.settings_menu_opened(before);
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

    /// A key while a setting's dropdown is open: ↑↓ (and ⌃N ⌃P) move,
    /// Return chooses, Esc clears the filter and then closes, and what is
    /// typed filters. False for a printable key, which the input method
    /// types into the filter.
    fn settings_menu_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        let query = self.settings_menu.field.text.clone();
        if keystroke.modifiers.control {
            let by = match keystroke.key.as_str() {
                "n" | "j" => Some(1),
                "p" | "k" => Some(-1),
                _ => None,
            };
            if let Some(by) = by {
                self.step_settings_menu(by);
                return true;
            }
        }
        let outcome = self.settings_menu.field.handle(keystroke);
        if Self::clipboard_key(&mut self.settings_menu.field, &outcome, cx) {
            self.settings_menu_filtered();
            return true;
        }
        match outcome {
            FieldKey::Text(_) => return false,
            FieldKey::Copy | FieldKey::Cut | FieldKey::Paste | FieldKey::Ignored => {}
            FieldKey::Tab { .. } => {}
            FieldKey::Commit => {
                self.app.choose_setting_menu(&query);
                self.settings_menu.field = TextField::default();
                self.setting_chosen(cx);
            }
            FieldKey::Cancel => {
                if query.is_empty() {
                    self.app.close_setting_menu();
                } else {
                    self.settings_menu.field = TextField::default();
                    self.settings_menu_filtered();
                }
            }
            FieldKey::Down => self.step_settings_menu(1),
            FieldKey::Up => self.step_settings_menu(-1),
            FieldKey::Edited => self.settings_menu_filtered(),
        }
        true
    }

    fn step_settings_menu(&mut self, by: isize) {
        let query = self.settings_menu.field.text.clone();
        self.app.step_setting_menu(by, &query);
        if let Some(menu) = self.app.setting_menu() {
            self.settings_menu.scroll.scroll_to_item(menu.cursor);
        }
    }

    /// The dropdown's filter changed: what it leaves, from the top.
    pub(super) fn settings_menu_filtered(&mut self) {
        self.app.setting_menu_filtered();
        self.settings_menu.scroll.set_offset(Default::default());
    }

    /// After a row was changed: a dropdown it opened starts unfiltered,
    /// scrolled to the value in force.
    fn settings_menu_opened(&mut self, before: Option<SettingsMenu>) {
        let now = self.app.setting_menu().cloned();
        if let Some(menu) = now
            && before.as_ref().map(|m| &m.row) != Some(&menu.row)
        {
            self.settings_menu.field = TextField::default();
            self.settings_menu.scroll.set_offset(Default::default());
            self.settings_menu.scroll.scroll_to_item(menu.cursor);
        }
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
                if key == "cmd-," {
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
        // The dropdowns: Return opens one on the value in force, typing
        // filters it, Esc clears the filter and then closes it unchanged.
        self.settings_field = TextField::default();
        self.settings_searched();
        self.app.choose_settings_category("Appearance");
        for (id, typed) in [("appearance", ""), ("theme", "one")] {
            let Some(ix) = self.app.settings_shown("").iter().position(|r| r.id == id) else {
                continue;
            };
            let before = (
                self.app.prefs.appearance.clone(),
                self.app.prefs.theme.clone(),
            );
            if let Some(page) = self.app.settings_page.as_mut() {
                page.cursor = ix;
            }
            press(self, "enter", window, cx);
            let open = self.app.setting_menu().is_some_and(|m| m.row == id);
            let every = self.app.setting_menu_options("").len();
            self.settings_menu.field = TextField::new(typed);
            self.settings_menu_filtered();
            let filtered = self.app.setting_menu_options(typed).len();
            press(self, "down", window, cx);
            if !typed.is_empty() {
                press(self, "escape", window, cx);
            }
            press(self, "escape", window, cx);
            said.push(format!(
                "{id} dropdown open {open}, {every} options, {filtered} for {typed:?}, esc closes {}, unchanged {}",
                self.app.setting_menu().is_none() && self.app.settings_page.is_some(),
                (self.app.prefs.appearance.clone(), self.app.prefs.theme.clone()) == before
            ));
        }
        let pages: Vec<String> = self
            .app
            .settings_plugin_pages()
            .into_iter()
            .map(|plugin| {
                self.app.choose_settings_category(plugin.name());
                format!("{}:{}", plugin.id(), self.app.settings_shown("").len())
            })
            .collect();
        said.push(format!("plugin pages: {}", pages.join(" ")));
        // The sidebar's chords, as the window's handler takes them.
        press(self, "cmd-3", window, cx);
        let third = self.app.settings_category();
        press(self, "alt-cmd-2", window, cx);
        let second_plugin = self.app.settings_category();
        let plugin = self.app.settings_plugin_pages().first().copied();
        let before = plugin.map(|p| self.app.plugin_on(p));
        press(self, "shift-alt-cmd-1", window, cx);
        let flipped = plugin.map(|p| self.app.plugin_on(p)) != before;
        press(self, "shift-alt-cmd-1", window, cx);
        let back = plugin.map(|p| self.app.plugin_on(p)) == before;
        said.push(format!(
            "keys: cmd-3 {third}, alt-cmd-2 {second_plugin}, switch flipped {flipped} and back {back}"
        ));
        // The window draws Sync's account screen before it closes.
        self.app.choose_settings_category("Sync");
        said.push(format!(
            "sync page: {}",
            self.app
                .settings_shown("")
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        ));
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

    /// After a dropdown's choice: a field it asked for opens (Self-hosted
    /// asks for the address), and what it changed is applied.
    fn setting_chosen(&mut self, cx: &mut Context<Self>) {
        if matches!(self.app.editing, Some(Editing::Ask(_))) {
            self.begin_editing();
        }
        self.after_change(cx);
    }

    /// A setting clicked: it is the one under the keyboard, and it changes.
    pub(super) fn settings_clicked(&mut self, ix: usize, id: &str, cx: &mut Context<Self>) {
        if let Some(page) = self.app.settings_page.as_mut() {
            page.cursor = ix;
        }
        // The click on a dropdown's own control, which its mouse-down
        // outside has just closed: it stays closed.
        if let Some((row, at)) = self.settings_menu.dismissed.take()
            && row == id
            && at.elapsed() < Duration::from_millis(600)
        {
            cx.notify();
            return;
        }
        let editing = self.app.editing.clone();
        let menu = self.app.setting_menu().cloned();
        self.app.change_setting(id);
        self.settings_menu_opened(menu);
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
        let mut plugins_headed = false;
        let google_folded = self.google_folded;
        let keys = self.app.settings_category_keys();
        let show_keys = !self.app.prefs.hide_key_hints;
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
                    .children(categories.into_iter().filter_map(|(name, glyph)| {
                        let chord = keys
                            .iter()
                            .find(|(entry, _)| *entry == name)
                            .map(|(_, chord)| chord.clone())
                            .filter(|_| show_keys);
                        let plugin = page::plugin_named(name).filter(|p| p.home().is_none());
                        let google = name == GOOGLE_ACCOUNT;
                        let under_google = plugin.is_some_and(|p| p.is_google());
                        // Folded, Google's plugins hide, but not the one shown.
                        if under_google && google_folded && name != current {
                            return None;
                        }
                        let in_plugins = plugin.is_some() || google;
                        let first_plugin = in_plugins && !plugins_headed;
                        plugins_headed |= in_plugins;
                        let chosen = !searching && name == current;
                        let entry = div()
                            .id(SharedString::from(format!("settings-category-{name}")))
                            .h(px(28.))
                            .px_2()
                            .when(under_google, |d| d.pl(px(30.)))
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(4.))
                            .when(chosen, |d| d.bg(t.selected))
                            .when(!chosen, |d| d.hover(|d| d.bg(t.hover)))
                            .when(google, |d| {
                                d.child(
                                    div()
                                        .id("settings-google-fold")
                                        .flex_none()
                                        .rounded(px(4.))
                                        .hover(|d| d.bg(t.hover))
                                        .child(
                                            icon(
                                                if google_folded {
                                                    "chevron-right"
                                                } else {
                                                    "chevron-down"
                                                },
                                                14.,
                                            )
                                            .text_color(t.muted),
                                        )
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            cx.stop_propagation();
                                            this.google_folded = !this.google_folded;
                                            cx.notify();
                                        })),
                                )
                            })
                            .when(plugin.is_none() && !google, |d| {
                                d.child(icon(glyph, 14.).text_color(if chosen {
                                    t.text
                                } else {
                                    t.muted
                                }))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(if chosen { t.text } else { t.muted })
                                    .child(name),
                            )
                            .when_some(chord, |d, chord| {
                                d.child(super::widgets::key_hint(chord, t))
                            })
                            .when_some(plugin, |d, plugin| {
                                d.child(self.sidebar_switch(plugin, t, cx))
                            })
                            .when(google, |d| d.child(self.google_switch(t, cx)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.settings_field = TextField::default();
                                this.app.choose_settings_category(name);
                                if google {
                                    this.google_folded = false;
                                }
                                this.settings_scroll.set_offset(Default::default());
                                cx.notify();
                            }));
                        Some(
                            div()
                                .flex()
                                .flex_col()
                                .when(first_plugin, |d| {
                                    d.child(
                                        div()
                                            .px_2()
                                            .pt_4()
                                            .pb_1()
                                            .text_size(px(10.))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(t.placeholder)
                                            .child("PLUGINS"),
                                    )
                                })
                                .child(entry),
                        )
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
                    .child(hint("⌘1–9", "A category"))
                    .child(hint("⌥⌘1–9", "A plugin"))
                    .child(hint("⇧⌥⌘1–9", "Switch it on or off"))
                    .child(hint("tab", "Next category"))
                    .child(hint("esc", "Close settings")),
            )
    }

    /// A plugin's switch in the sidebar: a click flips it without leaving
    /// the page shown.
    fn sidebar_switch(&self, plugin: Plugin, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let on = self.app.plugin_on(plugin);
        Self::switch(plugin.id(), on, t)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.app.toggle_plugin(plugin);
                cx.notify();
            }))
            .into_any_element()
    }

    /// Google's switch beside it in the sidebar, over all its plugins.
    fn google_switch(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        Self::switch(GOOGLE_SWITCH, self.app.google_on(), t)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.app.toggle_google();
                cx.notify();
            }))
            .into_any_element()
    }

    fn switch(id: &str, on: bool, t: Tokens) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(format!("settings-plugin-switch-{id}")))
            .flex_none()
            .w(px(26.))
            .h(px(16.))
            .p(px(2.))
            .flex()
            .items_center()
            .when(on, |d| d.justify_end())
            .rounded_full()
            .border_1()
            .border_color(if on { t.accent } else { t.border })
            .bg(if on { t.accent } else { t.hover })
            .child(
                div()
                    .size(px(10.))
                    .rounded_full()
                    .bg(if on { t.background } else { t.muted }),
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
        // A section's heading above its first row: a small group shown
        // under a bigger category (Shared lists under Sync).
        let category = self.app.settings_category();
        let mut body: Vec<AnyElement> = Vec::new();
        let mut section: Option<&'static str> = None;
        // Sync's own rows are an account screen rather than a list.
        let sync_page = !searching && category == super::sync_page::SYNC;
        if sync_page {
            let own: Vec<(usize, &SettingRow)> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.group == super::sync_page::SYNC)
                .collect();
            body.push(self.sync_panel(&own, cursor, t, cx));
        }
        for (ix, row) in rows.iter().enumerate() {
            if sync_page && row.group == super::sync_page::SYNC {
                continue;
            }
            if !searching && row.group != category && section != Some(row.group) {
                body.push(
                    div()
                        .px_2()
                        .pt_6()
                        .pb_1()
                        .text_size(px(10.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(t.placeholder)
                        .child(row.group.to_uppercase())
                        .into_any_element(),
                );
            }
            section = Some(row.group);
            body.push(self.setting_row(ix, ix == cursor, row, searching, t, cx));
        }
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

    #[allow(clippy::too_many_arguments)]
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
        let plugin = !searching && row.id.starts_with("plugin:");
        div()
            .id(SharedString::from(format!("setting-{}", row.id)))
            .pr_2()
            .pl(px(8.))
            .py(px(12.))
            .min_h(px(44.))
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
                            .when(plugin, |d| d.font_weight(FontWeight::MEDIUM))
                            .text_color(t.text)
                            .child(row.title.clone()),
                    )
                    .when(!description.is_empty(), |d| {
                        d.child(div().text_color(t.muted).child(description))
                    }),
            )
            .child(self.setting_control(row, t, cx))
            .into_any_element()
    }

    /// A setting's control, drawn, and its dropdown when that is open; the
    /// row takes the click.
    fn setting_control(&self, row: &SettingRow, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let open = self.app.setting_menu().is_some_and(|m| m.row == row.id);
        let hotkey = match row.id.as_str() {
            "hotkey-capture" => Some(Hotkey::QuickCapture),
            "hotkey-panel" => Some(Hotkey::FocusPanel),
            "hotkey-window" => Some(Hotkey::ShowWindow),
            _ => None,
        };
        let recording = matches!(&self.app.editing, Some(Editing::Hotkey(h)) if Some(*h) == hotkey);
        let drawn = control(row.control(), open, recording, t);
        if !open {
            return drawn;
        }
        div()
            .relative()
            .flex_none()
            .child(drawn)
            .child(
                // Its top right corner under the control's bottom right,
                // kept inside the window.
                div().absolute().right_0().bottom_0().child(
                    deferred(
                        anchored()
                            .anchor(Corner::TopRight)
                            .offset(point(px(0.), px(4.)))
                            .snap_to_window_with_margin(px(8.))
                            .child(self.settings_menu_view(t, cx)),
                    )
                    .with_priority(1),
                ),
            )
            .into_any_element()
    }

    /// The open dropdown: a filter once the list is long, and the options,
    /// the one in force ticked and the one under the keyboard lit.
    fn settings_menu_view(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = self.app.setting_menu().cloned() else {
            return div().into_any_element();
        };
        let query = self.settings_menu.field.text.clone();
        let options = self.app.setting_menu_options(&query);
        let every = self
            .app
            .setting_options(&menu.row)
            .map_or(0, |all| all.len());
        let filtering = every >= page::MENU_FILTER_FROM || !query.is_empty();
        let cursor = menu.cursor.min(options.len().saturating_sub(1));
        let row_id = menu.row.clone();
        let items: Vec<AnyElement> = options
            .into_iter()
            .enumerate()
            .map(|(ix, option)| {
                let here = ix == cursor;
                let row = menu.row.clone();
                let value = option.value.clone();
                div()
                    .id(("settings-option", ix))
                    .h(px(MENU_ROW))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(4.))
                    .when(here, |d| d.bg(t.selected))
                    .when(!here, |d| d.hover(|d| d.bg(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.app.choose_setting_option(&row, &value);
                        this.settings_menu.field = TextField::default();
                        this.setting_chosen(cx);
                        cx.notify();
                    }))
                    .child(div().w(px(14.)).flex_none().when(option.current, |d| {
                        d.child(icon("check", 14.).text_color(t.accent))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(t.text)
                            .child(option.label),
                    )
                    .when(!option.detail.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(t.placeholder)
                                .child(option.detail),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let empty = items.is_empty();
        div()
            .id("settings-menu")
            .occlude()
            .w(px(CONTROL_WIDTH + 40.))
            .flex()
            .flex_col()
            .rounded(px(6.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(move |this, _: &gpui::MouseDownEvent, _, cx| {
                this.app.close_setting_menu();
                this.settings_menu.dismissed = Some((row_id.clone(), Instant::now()));
                cx.notify();
            }))
            .when(filtering, |d| {
                d.child(
                    div()
                        .h(px(30.))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(icon("search", 12.).text_color(t.muted))
                        .child(div().flex_1().min_w_0().overflow_hidden().child(field_text(
                            &self.settings_menu.field,
                            "Filter…",
                            t,
                            self.input_hook(),
                        ))),
                )
            })
            .child(
                div()
                    .id("settings-menu-options")
                    .max_h(px(MENU_ROW * 11. + 8.))
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_menu.scroll)
                    .p_1()
                    .flex()
                    .flex_col()
                    .when(empty, |d| {
                        d.child(
                            div()
                                .h(px(MENU_ROW))
                                .px_2()
                                .flex()
                                .items_center()
                                .text_color(t.placeholder)
                                .child(if query.trim().is_empty() {
                                    "Nothing to choose yet"
                                } else {
                                    "Nothing matches"
                                }),
                        )
                    })
                    .children(items),
            )
            .into_any_element()
    }
}

/// A setting's control, drawn: a switch, a button, a dropdown showing its
/// value, a field showing its text, or a shortcut's keys. Every boxed
/// control is the same height, and dropdowns and fields the same width.
fn control(control: Control, open: bool, recording: bool, t: Tokens) -> AnyElement {
    let note = |text: String| {
        div()
            .max_w(px(260.))
            .truncate()
            .text_color(t.muted)
            .child(text)
    };
    let boxed = |lit: bool| {
        div()
            .h(px(CONTROL_HEIGHT))
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(if lit { t.accent } else { t.border })
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
            .child(
                boxed(false)
                    .px_3()
                    .bg(t.surface)
                    .hover(|d| d.bg(t.hover))
                    .child(label),
            )
            .into_any_element(),
        Control::Select(value) => side
            .child(
                boxed(open)
                    .w(px(CONTROL_WIDTH))
                    .pl_2()
                    .pr(px(6.))
                    .justify_between()
                    .bg(if open { t.hover } else { t.surface })
                    .hover(|d| d.bg(t.hover))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if value.is_empty() {
                                t.placeholder
                            } else {
                                t.text
                            })
                            .child(if value.is_empty() {
                                "Choose…".to_string()
                            } else {
                                value
                            }),
                    )
                    .child(icon("chevrons-up-down", 12.).text_color(t.muted)),
            )
            .into_any_element(),
        Control::Text(value) => side
            .child(
                boxed(false)
                    .w(px(CONTROL_WIDTH))
                    .px_2()
                    .bg(t.background)
                    .hover(|d| d.border_color(t.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if value.is_empty() {
                                t.placeholder
                            } else {
                                t.text
                            })
                            .child(if value.is_empty() {
                                "Not set".to_string()
                            } else {
                                value
                            }),
                    ),
            )
            .into_any_element(),
        Control::Shortcut(keys) => {
            let off = keys == "Off";
            side.child(
                boxed(recording)
                    .px_2()
                    .min_w(px(96.))
                    .justify_center()
                    .bg(t.surface)
                    .hover(|d| d.bg(t.hover))
                    .font_family(MONO_FONT)
                    .text_size(px(12.))
                    .text_color(if recording || off { t.muted } else { t.text })
                    .child(if recording {
                        "Press keys…".to_string()
                    } else {
                        keys
                    }),
            )
            .into_any_element()
        }
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

/// The digit 1–9 a chord is on, whatever the modifiers made of it.
fn digit_key(keystroke: &gpui::Keystroke) -> Option<u32> {
    let mut chars = keystroke.key.chars();
    let digit = chars.next()?.to_digit(10)?;
    (chars.next().is_none() && digit > 0).then_some(digit)
}
