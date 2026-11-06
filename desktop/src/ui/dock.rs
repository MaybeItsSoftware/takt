use super::*;

impl Takt {
    pub(super) fn right_dock(
        &self,
        t: Tokens,
        stops: Vec<Stop>,
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
                self.inspector_pane(t, stops, cx).into_any_element()
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
        stops: Vec<Stop>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.app.region == Region::Inspector;
        let cursor = self.app.inspector_cursor.min(stops.len().saturating_sub(1));
        let editing = match &self.app.editing {
            Some(Editing::InspectorField { stop, .. }) => Some(*stop),
            _ => None,
        };
        let body: Vec<AnyElement> = if stops.is_empty() {
            vec![
                div()
                    .text_color(t.placeholder)
                    .child("Nothing selected")
                    .into_any_element(),
            ]
        } else {
            stops
                .iter()
                .enumerate()
                .map(|(ix, stop)| {
                    let ringed = active && ix == cursor;
                    let open = editing == Some(stop.id);
                    let value: AnyElement = if open {
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.accent)
                            .bg(t.background)
                            .child(field_text(&self.field, "", t, self.input_hook()))
                            .into_any_element()
                    } else {
                        let tone = if stop.empty {
                            t.placeholder
                        } else if stop.kind == StopKind::Action && stop.label.is_empty() {
                            t.muted
                        } else {
                            t.text
                        };
                        let multiline = matches!(stop.kind, StopKind::Text { multiline: true });
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_1()
                            .text_color(tone)
                            .when(stop.id == "title", |d| {
                                d.text_size(px(16.)).font_weight(FontWeight::MEDIUM)
                            })
                            .when(!multiline, |d| d.truncate())
                            .when(multiline && !stop.empty, |d| {
                                d.flex().flex_col().children(
                                    stop.value
                                        .lines()
                                        .map(|line| {
                                            div()
                                                .min_h(px(18.))
                                                .child(SharedString::from(line.to_string()))
                                        })
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .when(!multiline || stop.empty, |d| d.child(stop.value.clone()))
                            .into_any_element()
                    };
                    div()
                        .id(("inspector-stop", ix))
                        .when(stop.group && ix > 0, |d| {
                            d.mt_1().pt_1().border_t_1().border_color(t.border_variant)
                        })
                        .child(
                            div()
                                .id(("inspector-row", ix))
                                .flex()
                                .items_start()
                                .gap_2()
                                .py(px(3.))
                                .px_1()
                                .rounded(px(4.))
                                .border_1()
                                .border_color(if ringed && !open {
                                    t.accent
                                } else {
                                    gpui::transparent_black()
                                })
                                .when(ringed, |d| d.bg(t.hover))
                                .hover(|d| d.bg(t.hover))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.app.region = Region::Inspector;
                                    this.app.inspector_cursor = ix;
                                    let before = this.app.editing.clone();
                                    this.app.activate_inspector();
                                    if this.app.editing != before {
                                        this.begin_editing();
                                    }
                                    this.after_change(cx);
                                    cx.notify();
                                }))
                                .when(!stop.label.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .w(px(84.))
                                            .flex_none()
                                            .text_color(t.muted)
                                            .child(stop.label),
                                    )
                                })
                                .child(value),
                        )
                        .into_any_element()
                })
                .collect()
        };
        div()
            .id("inspector")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .children(body)
    }
}
