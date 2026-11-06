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

    pub(super) fn sidebar(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.app.region == Region::Sidebar;
        let new_item = match &self.app.editing {
            Some(Editing::NewList { .. }) => Some("New list"),
            Some(Editing::NewFolder { .. }) => Some("New folder"),
            _ => None,
        };
        div()
            .w(px(self.app.prefs.sidebar_width.clamp(180., 420.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
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
        let left = if let Some(digits) = &self.app.number_jump {
            format!(":{digits}")
        } else if !prefix.is_empty() {
            format!("{prefix}…")
        } else if let Some(message) = &self.app.message {
            message.clone()
        } else {
            match self.app.region {
                Region::Sidebar => "Sidebar".into(),
                Region::Tasks => self.app.view_mode.title().into(),
                Region::Inspector => "Inspector".into(),
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
}
