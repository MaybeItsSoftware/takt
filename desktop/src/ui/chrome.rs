use super::*;

impl Takt {
    pub(super) fn title_bar(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let modes = [
            ViewMode::Today,
            ViewMode::Board,
            ViewMode::Outline,
            ViewMode::Matrix,
        ];
        div()
            .h(px(TITLE_BAR_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(80.))
            .pr_2()
            .bg(t.chrome)
            .border_b_1()
            .border_color(t.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(t.text).child(self.app.scope_title()))
                    .when(self.app.prefs.hide_completed, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child("finished hidden"),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .children(modes.into_iter().enumerate().map(|(ix, mode)| {
                        let current = self.app.view_mode == mode;
                        div()
                            .id(("mode", ix))
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .text_size(px(12.))
                            .text_color(if current { t.text } else { t.muted })
                            .when(current, |d| d.bg(t.selected))
                            .hover(|d| d.bg(t.hover))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.app.select_view_mode(mode);
                                this.after_change(cx);
                                cx.notify();
                            }))
                            .child(mode.title())
                    })),
            )
    }

    pub(super) fn left_dock(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.app.prefs.left_dock_tab == "agent";
        let tab = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            let on = self.app.prefs.left_dock_tab == id;
            div()
                .id(id)
                .px_2()
                .py_1()
                .text_size(px(12.))
                .text_color(if on { t.text } else { t.muted })
                .border_b_1()
                .border_color(if on {
                    t.accent
                } else {
                    gpui::transparent_black()
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.app.prefs.left_dock_tab = id.into();
                    if id == "agent" {
                        this.app.region = Region::Agent;
                        this.app.editing = Some(Editing::AgentMessage);
                        this.begin_editing();
                    } else if this.app.region == Region::Agent {
                        this.app.region = Region::Sidebar;
                        this.app.editing = None;
                    }
                    this.app.prefs.save();
                    cx.notify();
                }))
                .child(label)
        };
        div()
            .w(px(
                self.app.prefs.sidebar_width.clamp(180., 420.) * if agent { 1.5 } else { 1. }
            ))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_r_1()
            .border_color(
                if matches!(self.app.region, Region::Sidebar | Region::Agent) {
                    t.accent
                } else {
                    t.border
                },
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .px_2()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(tab("lists", "Lists", cx))
                    .child(tab("agent", "Agent", cx)),
            )
            .child(if agent {
                self.agent_pane(t).into_any_element()
            } else {
                self.sidebar(t, cx).into_any_element()
            })
            .into_any_element()
    }

    pub(super) fn agent_pane(&self, t: Tokens) -> impl IntoElement {
        use crate::state::agent::{Client, Entry};
        let client = self
            .app
            .agent
            .client
            .unwrap_or_else(|| Client::from_pref(&self.app.prefs.agent_client));
        let typing = matches!(self.app.editing, Some(Editing::AgentMessage));
        let entries = self
            .app
            .agent
            .entries
            .iter()
            .enumerate()
            .map(|(ix, entry)| {
                let base = div().id(("agent-entry", ix)).px_3().py_1();
                match entry {
                    Entry::User(text) => base
                        .bg(t.background)
                        .border_l_2()
                        .border_color(t.accent)
                        .child(text.clone())
                        .into_any_element(),
                    Entry::Assistant(text) => base.child(text.clone()).into_any_element(),
                    Entry::Tool {
                        name,
                        summary,
                        write,
                        failed,
                        ..
                    } => base
                        .mx_2()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(match failed {
                            Some(true) => t.error,
                            _ if *write => t.success,
                            _ => t.border_variant,
                        })
                        .text_size(px(11.))
                        .child(
                            div()
                                .font_family(MONO_FONT)
                                .text_color(if *write { t.text } else { t.muted })
                                .child(format!(
                                    "{}{name}{}",
                                    if *write { "✎ " } else { "· " },
                                    match failed {
                                        None => "…",
                                        Some(true) => " — failed",
                                        Some(false) if *write => " — done, ⌘Z undoes",
                                        Some(false) => "",
                                    }
                                )),
                        )
                        .when(!summary.is_empty(), |d| {
                            d.child(div().text_color(t.placeholder).child(summary.clone()))
                        })
                        .into_any_element(),
                    Entry::Notice { text, error } => base
                        .text_size(px(11.))
                        .text_color(if *error { t.error } else { t.muted })
                        .child(text.clone())
                        .into_any_element(),
                }
            });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(client.name())
                    .child(if self.app.agent.working { "working…" } else { "⌘N new thread" }),
            )
            .child(
                div()
                    .id("agent-entries")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .when(self.app.agent.entries.is_empty(), |d| {
                        d.child(
                            div()
                                .p_3()
                                .text_color(t.placeholder)
                                .child("Ask about your tasks, or ask for a change. Changes apply at once; ⌘Z takes each back."),
                        )
                    })
                    .children(entries),
            )
            .child(
                div()
                    .m_2()
                    .px_2()
                    .py_1()
                    .min_h(px(54.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if typing { t.accent } else { t.border })
                    .bg(t.background)
                    .child(if typing {
                        field_text(&self.field, "Message…", t, self.input_hook()).into_any_element()
                    } else {
                        div()
                            .text_color(t.placeholder)
                            .child("Message… (click or ⇧⌘A)")
                            .into_any_element()
                    }),
            )
    }

    pub(super) fn sidebar(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.app.region == Region::Sidebar;
        let new_item = match &self.app.editing {
            Some(Editing::NewList { .. }) => Some("New list"),
            Some(Editing::NewFolder { .. }) => Some("New folder"),
            _ => None,
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when_some(new_item, |d, placeholder| {
                d.child(self.field_row(t, placeholder, 0., true))
            })
            .child(
                uniform_list(
                    "sidebar",
                    self.app.sidebar_rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|ix| this.sidebar_row(ix, active, t, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.sidebar_scroll.clone())
                .flex_1()
                .py_1(),
            )
    }

    pub(super) fn sidebar_row(
        &self,
        ix: usize,
        active: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.app.sidebar_rows[ix].clone();
        let current = self.app.row_is_current(&row);
        let cursor = active
            && self
                .app
                .sidebar_cursor_row()
                .is_some_and(|cursor| cursor.id == row.id);
        if let Some(Editing::SidebarName { row_id }) = &self.app.editing
            && *row_id == row.id
        {
            return self
                .field_row(t, "Name", row.depth as f32 * 14., true)
                .into_any_element();
        }
        let (icon, count) = match &row.kind {
            RowKind::Today => ("◎", None),
            RowKind::Everything => ("∞", None),
            RowKind::List(id) => (
                if self.app.inbox().is_some_and(|inbox| &inbox.id == id) {
                    "⇣"
                } else {
                    "≡"
                },
                self.app.task_counts.get(id).copied(),
            ),
            RowKind::NestedList(_) => ("↳", None),
            RowKind::Folder(id) => (
                if self.app.prefs.expanded_folders.contains(id) {
                    "▾"
                } else {
                    "▸"
                },
                None,
            ),
            RowKind::FilterList(_) => ("⌕", None),
        };
        let custom = match &row.kind {
            RowKind::List(id) | RowKind::NestedList(id) | RowKind::FilterList(id) => {
                self.app.prefs.icons.get(id).cloned()
            }
            _ => None,
        };
        let icon: SharedString = custom.unwrap_or_else(|| icon.to_string()).into();
        let title = self.row_title(&row.kind);
        let target = row.clone();
        div()
            .id(("sidebar-row", ix))
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(8. + row.depth as f32 * 14.))
            .pr_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if cursor {
                t.accent
            } else {
                gpui::transparent_black()
            })
            .when(current, |d| d.bg(t.selected))
            .when(!current, |d| d.hover(|d| d.bg(t.hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.app.apply_sidebar_cursor(&target);
                this.app.region = Region::Sidebar;
                this.after_change(cx);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(t.placeholder)
                    .text_size(px(11.))
                    .child(icon),
            )
            .child(div().flex_1().min_w_0().truncate().child(title))
            .when_some(count.filter(|c| *c > 0), |d, count| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(count.to_string()),
                )
            })
            .into_any_element()
    }

    pub(super) fn status_bar(&self, t: Tokens) -> impl IntoElement {
        let prefix = self.app.sequence.prefix().to_string();
        let pending = !prefix.is_empty() || self.app.pending_delete.is_some();
        let celebrated = self
            .app
            .celebration
            .as_ref()
            .filter(|(_, at)| crate::state::store::now_ms() - at < 5000)
            .map(|(title, _)| format!("✓ Finished \u{201c}{title}\u{201d}"));
        let left = if let Some(text) = celebrated {
            text
        } else if let Some(digits) = &self.app.number_jump {
            format!(":{digits}")
        } else if !prefix.is_empty() {
            format!("{prefix}…")
        } else if let Some(message) = &self.app.message {
            message.clone()
        } else {
            match self.app.region {
                Region::Sidebar => "Sidebar".into(),
                Region::Agent => "Agent".into(),
                Region::Tasks => self.app.view_mode.title().into(),
                Region::Inspector => "Inspector".into(),
                Region::Conditions => "Conditions".into(),
                Region::Done => "Done".into(),
                Region::Timeline => "Timeline".into(),
            }
        };
        let right = [
            self.app
                .undo_label
                .as_ref()
                .map(|label| format!("⌘Z {label}")),
            self.last_key_ms.map(|ms| format!("{ms:.2} ms")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("   ");
        let focus = self.app.focus_status();
        div()
            .h(px(24.))
            .flex_none()
            .px_3()
            .gap_3()
            .flex()
            .items_center()
            .justify_between()
            .bg(t.chrome)
            .border_t_1()
            .border_color(t.border)
            .text_size(px(11.))
            .text_color(t.muted)
            .child(
                div()
                    .truncate()
                    .flex_1()
                    .min_w_0()
                    .when(pending, |d| d.text_color(t.warning))
                    .child(left),
            )
            .when_some(focus, |d, (title, clock, over)| {
                d.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(div().text_color(t.accent).child("●"))
                        .child(
                            div()
                                .max_w(px(260.))
                                .truncate()
                                .text_color(t.text)
                                .child(title),
                        )
                        .child(
                            div()
                                .font_family(MONO_FONT)
                                .text_color(if over { t.warning } else { t.text })
                                .child(clock),
                        ),
                )
            })
            .child(div().font_family(MONO_FONT).child(right))
    }

    pub(super) fn overlay_view(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let placeholder = match self.app.overlay {
            Some(Overlay::CommandPalette) => "Run a command…",
            Some(Overlay::ListNavigator) => "Go to a list…",
            Some(Overlay::Search) => "Search tasks…",
            Some(Overlay::KeyboardReference) => "Filter keys…",
            Some(Overlay::Picker(ref picker)) => App::picker_prompt(picker),
            Some(Overlay::Settings) => "Settings…",
            Some(Overlay::Diagnostics) => "Diagnostics…",
            None => "",
        };
        let items = self.overlay.items.clone();
        let selected = self.overlay.index;
        div()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 8.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("overlay")
                    .occlude()
                    .w(px(560.))
                    .max_h(px(440.))
                    .flex()
                    .flex_col()
                    .rounded(px(8.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .shadow_lg()
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(t.border_variant)
                            .child(field_text(
                                &self.overlay.field,
                                placeholder,
                                t,
                                self.input_hook(),
                            )),
                    )
                    .child(
                        uniform_list(
                            "overlay-items",
                            items.len(),
                            cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                                range
                                    .map(|ix| {
                                        let item = &items[ix];
                                        div()
                                            .id(("overlay-item", ix))
                                            .h(px(ROW_HEIGHT + 2.))
                                            .mx_1()
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .rounded(px(4.))
                                            .when(ix == selected, |d| d.bg(t.selected))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(item.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(t.placeholder)
                                                    .child(item.detail.clone()),
                                            )
                                            .child(
                                                div()
                                                    .font_family(MONO_FONT)
                                                    .text_size(px(11.))
                                                    .text_color(t.muted)
                                                    .child(item.keys.clone()),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .h(px(380.))
                        .py_1(),
                    ),
            )
    }

    /// A text field drawn as a row: the draft task, a new list's name.
    pub(super) fn field_row(
        &self,
        t: Tokens,
        placeholder: &str,
        indent: f32,
        _focused: bool,
    ) -> impl IntoElement {
        div()
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(8. + indent))
            .pr_3()
            .flex()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(t.accent)
            .bg(t.background)
            .child(field_text(&self.field, placeholder, t, self.input_hook()))
    }

    pub(super) fn field_inline(&self, t: Tokens, placeholder: &str) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .child(field_text(&self.field, placeholder, t, self.input_hook()))
    }

    /// The rules editor: the rules as lines, what they read as, and every
    /// field the core knows. ⌘↩ saves, Esc leaves it.
    pub(super) fn rules_sheet(&self, t: Tokens) -> impl IntoElement {
        let Some(Editing::Rules(target)) = self.app.editing.clone() else {
            return div();
        };
        let title = target.title();
        let (reading, help) = if target.is_form() {
            (
                self.app.form_describe(&target, &self.field.text),
                App::form_help(&target),
            )
        } else {
            (
                crate::state::rules::describe(&self.field.text),
                crate::state::rules::catalogue_help(),
            )
        };
        div()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 8.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("rules-sheet")
                    .occlude()
                    .w(px(620.))
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
                            .border_b_1()
                            .border_color(t.border_variant)
                            .child(div().text_color(t.text).child(title))
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(11.))
                                    .text_color(t.muted)
                                    .child("⌘↩ save · esc cancel"),
                            ),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .min_h(px(120.))
                            .font_family(MONO_FONT)
                            .child(field_text(&self.field, "show open", t, self.input_hook())),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_t_1()
                            .border_color(t.border_variant)
                            .text_color(t.accent)
                            .child(reading),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_t_1()
                            .border_color(t.border_variant)
                            .font_family(MONO_FONT)
                            .text_size(px(11.))
                            .text_color(t.placeholder)
                            .flex()
                            .flex_col()
                            .children(help),
                    ),
            )
    }
}

impl Takt {
    pub(super) fn focus_prompt_sheet(&self, t: Tokens) -> impl IntoElement {
        let Some(prompt) = self.app.focus_prompt.clone() else {
            return div();
        };
        let selected = prompt.selected();
        let note_open = matches!(self.app.editing, Some(Editing::FocusNote));
        let minutes = prompt.seconds / 60;
        div()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 48.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("focus-prompt")
                    .occlude()
                    .w(px(480.))
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .rounded(px(8.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .child(div().text_size(px(15.)).child(if prompt.time_is_up {
                        "Time's up. How did it go?"
                    } else {
                        "How did it go?"
                    }))
                    .child(div().text_color(t.muted).child(format!(
                        "{minutes} minutes on \u{201c}{}\u{201d}",
                        prompt.title
                    )))
                    .when(prompt.scores, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_size(px(28.))
                                        .text_color(t.accent)
                                        .child(format!("×{:.1}", prompt.multiplier())),
                                )
                                .child(div().text_color(t.muted).child(format!(
                                    "{:.1} pts · ↑↓ to change · r resets · \\ scores nothing",
                                    prompt.points()
                                ))),
                        )
                    })
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if note_open {
                                t.accent
                            } else {
                                t.border_variant
                            })
                            .child(if note_open {
                                field_text(&self.field, "A note for the task", t, self.input_hook())
                                    .into_any_element()
                            } else if prompt.note.is_empty() {
                                div()
                                    .text_color(t.placeholder)
                                    .child("n adds a note to the task")
                                    .into_any_element()
                            } else {
                                div().child(prompt.note.clone()).into_any_element()
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .children(prompt.choices().into_iter().map(|choice| {
                                let on = choice == selected;
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(if on { t.accent } else { t.border_variant })
                                    .when(on, |d| d.bg(t.selected))
                                    .child(format!("{}  {}", choice.title(&prompt), choice.key()))
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(t.placeholder)
                            .child("←→ choose · ↩ takes it · esc keeps working"),
                    ),
            )
    }
}
