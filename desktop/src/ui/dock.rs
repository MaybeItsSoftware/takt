use super::*;

impl Takt {
    pub(super) fn right_dock(
        &self,
        t: Tokens,
        snapshot: Option<EditorSnapshot>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = self.app.prefs.right_dock_tab.clone();
        let done = tab == "done";
        let tab_button = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            let current = tab == id;
            div()
                .id(id)
                .px_2()
                .py_1()
                .text_size(px(12.))
                .text_color(if current { t.text } else { t.muted })
                .border_b_1()
                .border_color(if current {
                    t.accent
                } else {
                    gpui::transparent_black()
                })
                .hover(|d| d.text_color(t.text))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.app.prefs.right_dock_tab = id.into();
                    this.app.prefs.save();
                    cx.notify();
                }))
                .child(label)
        };
        div()
            .w(px(self.app.prefs.right_dock_width.clamp(240., 520.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_l_1()
            .border_color(if self.app.region == Region::Inspector {
                t.accent
            } else {
                t.border
            })
            .child(
                div()
                    .flex()
                    .gap_1()
                    .px_2()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .child(tab_button("inspector", "Inspector", cx))
                    .child(tab_button("done", "Done", cx)),
            )
            .child(if done {
                self.done_rail(t, cx).into_any_element()
            } else {
                self.inspector_pane(t, snapshot).into_any_element()
            })
    }

    pub(super) fn done_rail(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut last_day = String::new();
        for (ix, task) in self.app.done.iter().enumerate() {
            let day = task.completed_at_ms.map(format_day).unwrap_or_default();
            if day != last_day {
                rows.push(
                    div()
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(day.clone())
                        .into_any_element(),
                );
                last_day = day;
            }
            let selected = self.app.selected_task.as_deref() == Some(task.id.as_str());
            let id = task.id.clone();
            let list = self
                .app
                .list(&task.list_id)
                .map(|list| list.name.clone())
                .unwrap_or_default();
            rows.push(
                div()
                    .id(("done-row", ix))
                    .mx_1()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .when(selected, |d| d.bg(t.selected))
                    .when(!selected, |d| d.hover(|d| d.bg(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.app.selected_task = Some(id.clone());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(check(task, t))
                            .child(title(task, t)),
                    )
                    .child(
                        div()
                            .pl(px(20.))
                            .text_size(px(11.))
                            .text_color(t.placeholder)
                            .child(list),
                    )
                    .into_any_element(),
            );
        }
        if rows.is_empty() {
            rows.push(
                div()
                    .p_3()
                    .text_color(t.placeholder)
                    .child("Nothing finished in the last fortnight.")
                    .into_any_element(),
            );
        }
        div()
            .id("done-rail")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py_1()
            .children(rows)
    }

    pub(super) fn inspector_pane(
        &self,
        t: Tokens,
        snapshot: Option<EditorSnapshot>,
    ) -> impl IntoElement {
        let body: AnyElement = match (snapshot, self.app.selected()) {
            (Some(snapshot), Some(task)) => {
                let list = self
                    .app
                    .list(&task.list_id)
                    .map(|list| list.name.clone())
                    .unwrap_or_default();
                let field = |label: &str, value: String| {
                    div()
                        .flex()
                        .gap_2()
                        .py_1()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(
                            div()
                                .w(px(80.))
                                .flex_none()
                                .text_color(t.placeholder)
                                .child(label.to_string()),
                        )
                        .child(div().flex_1().min_w_0().child(value))
                };
                let dash = || "—".to_string();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .pb_2()
                            .child(snapshot.title.clone()),
                    )
                    .child(field("List", list))
                    .child(field("Status", status_title(&task.status).into()))
                    .child(field(
                        "Due",
                        snapshot.due_at_ms.map(format_day).unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Estimate",
                        snapshot
                            .estimate_seconds
                            .map(format_duration)
                            .unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Priority",
                        snapshot
                            .metadata
                            .priority
                            .map(|p| format!("P{p}"))
                            .unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Tags",
                        if snapshot.metadata.tags.is_empty() {
                            dash()
                        } else {
                            snapshot.metadata.tags.join(", ")
                        },
                    ))
                    .child(field(
                        "Repeats",
                        snapshot
                            .metadata
                            .recurrence_rule
                            .clone()
                            .unwrap_or_else(dash),
                    ))
                    .child(
                        div()
                            .pt_2()
                            .text_color(if snapshot.notes.is_empty() {
                                t.placeholder
                            } else {
                                t.text
                            })
                            .child(if snapshot.notes.is_empty() {
                                "No notes".to_string()
                            } else {
                                snapshot.notes.clone()
                            }),
                    )
                    .into_any_element()
            }
            _ => div()
                .text_color(t.placeholder)
                .child("Nothing selected")
                .into_any_element(),
        };
        div()
            .id("inspector")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .child(body)
    }
}
