use super::*;

impl Takt {
    /// A strip of tabs; clicking one shows it.
    fn tab_strip(
        &self,
        t: Tokens,
        tabs: &[(&'static str, &'static str)],
        current: &str,
        bottom: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(t.border_variant)
            .children(tabs.iter().map(|(id, label)| {
                let id = *id;
                let on = current == id;
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
                    .hover(|d| d.text_color(t.text))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if bottom {
                            this.app.show_bottom_tab(id);
                        } else {
                            this.app.show_right_tab(id, true);
                        }
                        cx.notify();
                    }))
                    .child(*label)
            }))
    }

    pub(super) fn right_dock(
        &self,
        t: Tokens,
        stops: Vec<Stop>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = self.app.prefs.right_dock_tab.clone();
        let focused = matches!(self.app.region, Region::Inspector | Region::Conditions);
        div()
            .w(px(self.app.prefs.right_dock_width.clamp(240., 520.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_l_1()
            .border_color(if focused { t.accent } else { t.border })
            .child(self.tab_strip(
                t,
                &[("inspector", "Inspector"), ("conditions", "Conditions")],
                &tab,
                false,
                cx,
            ))
            .child(if tab == "conditions" {
                self.conditions_pane(t, cx).into_any_element()
            } else {
                self.inspector_pane(t, stops, cx).into_any_element()
            })
    }

    pub(super) fn bottom_dock(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.app.prefs.bottom_dock_tab.clone();
        let focused = matches!(self.app.region, Region::Done | Region::Timeline);
        div()
            .h(px(260.))
            .flex_none()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_t_1()
            .border_color(if focused { t.accent } else { t.border })
            .child(self.tab_strip(
                t,
                &[
                    ("progress", "Progress"),
                    ("done", "Done"),
                    ("timeline", "Timeline"),
                ],
                &tab,
                true,
                cx,
            ))
            .child(match tab.as_str() {
                "timeline" => self.timeline_pane(t, cx).into_any_element(),
                "progress" => self.progress_pane(t).into_any_element(),
                _ => self.done_rail(t, cx).into_any_element(),
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
            let selected = self.app.done_cursor.as_deref() == Some(task.id.as_str());
            let cursor = selected && self.app.region == Region::Done;
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
                    .border_1()
                    .border_color(if cursor {
                        t.accent
                    } else {
                        gpui::transparent_black()
                    })
                    .when(selected, |d| d.bg(t.selected))
                    .when(!selected, |d| d.hover(|d| d.bg(t.hover)))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        this.app.done_cursor = Some(id.clone());
                        this.app.region = Region::Done;
                        if event.click_count() >= 2 {
                            this.app.reveal_done();
                        }
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

impl Takt {
    pub(super) fn conditions_pane(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.app.region == Region::Conditions;
        let cursor = self.app.condition_cursor();
        let editing_time = matches!(self.app.editing, Some(Editing::ConditionTime));
        let editing_name = match &self.app.editing {
            Some(Editing::ConditionName(id)) => Some(id.clone()),
            _ => None,
        };
        let editing_category = matches!(self.app.editing, Some(Editing::CategoryName(_)));
        let mut rows: Vec<AnyElement> = Vec::new();
        for (ix, (row, view)) in self.app.condition_views().into_iter().enumerate() {
            if let Some(heading) = &view.heading {
                rows.push(
                    div()
                        .px_3()
                        .pt_3()
                        .pb_1()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(heading.clone())
                        .into_any_element(),
                );
            }
            let here = row == cursor;
            let field = (editing_time && row == crate::state::conditions::Row::Time)
                || (editing_name.as_ref().is_some_and(|id| match (&row, id) {
                    (crate::state::conditions::Row::Add, None) => true,
                    (crate::state::conditions::Row::Condition(c), Some(id)) => c == id,
                    _ => false,
                }))
                || (editing_category && here);
            let target = row.clone();
            rows.push(
                div()
                    .id(("condition-row", ix))
                    .h(px(ROW_HEIGHT))
                    .mx_1()
                    .px_2()
                    .when(view.indent, |d| d.pl(px(18.)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if here && active {
                        t.accent
                    } else {
                        gpui::transparent_black()
                    })
                    .when(here, |d| d.bg(t.selected))
                    .when(!here, |d| d.hover(|d| d.bg(t.hover)))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        this.app.conditions.cursor = Some(target.clone());
                        this.app.region = Region::Conditions;
                        if event.click_count() >= 2 {
                            this.app.run("conditionsToggle", Some("space"));
                        }
                        cx.notify();
                    }))
                    .when_some(view.on, |d, on| {
                        d.child(
                            div()
                                .w(px(12.))
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(if on { t.success } else { t.placeholder })
                                .child(if on { "☑" } else { "☐" }),
                        )
                    })
                    .child(if field {
                        self.field_inline(t, "Name").into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if view.dim { t.muted } else { t.text })
                            .child(view.label.clone())
                            .into_any_element()
                    })
                    .when(!field, |d| {
                        d.child(
                            div()
                                .max_w(px(180.))
                                .truncate()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child(view.value.clone()),
                        )
                    })
                    .into_any_element(),
            );
        }
        if let Some((id, tasks)) = &self.app.conditions.tasks_for {
            let name = self
                .app
                .condition(id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            rows.push(
                div()
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(format!("Tasks that need {name}"))
                    .into_any_element(),
            );
            for task in tasks {
                rows.push(
                    div()
                        .px_3()
                        .py(px(2.))
                        .truncate()
                        .child(task.title.clone())
                        .into_any_element(),
                );
            }
        }
        div()
            .id("conditions")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py_1()
            .children(rows)
    }

    pub(super) fn timeline_pane(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let timeline = &self.app.timeline;
        let active = self.app.region == Region::Timeline;
        let Some(shaped) = timeline.shaped.clone() else {
            return div().p_3().text_color(t.placeholder).child("No focus yet.");
        };
        let layout = &shaped.layout;
        let span_minutes = ((layout.end_ms - layout.start_ms) as f32 / 60_000.).max(60.);
        let lanes = layout.lane_count.max(1);
        let hours = (span_minutes / 60.).ceil() as usize;
        let start_hour = {
            use chrono::{Local, TimeZone, Timelike};
            Local
                .timestamp_millis_opt(layout.start_ms)
                .single()
                .map_or(0, |time| time.hour() as usize)
        };
        let day = if self.app.timeline_is_today() {
            "Today".to_string()
        } else {
            format_day(timeline.day_ms)
        };
        let summaries = timeline.summaries();
        let palette = [t.accent, t.success, t.warning, t.error, t.muted];
        let colour_of = |task: &Option<String>| {
            let ix = summaries
                .iter()
                .position(|(_, id, _, _)| id == task)
                .unwrap_or(0);
            palette[ix % palette.len()]
        };
        let chart = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_2()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .children((0..hours).map(|h| {
                        div()
                            .flex_1()
                            .border_l_1()
                            .border_color(t.border_variant)
                            .pl_1()
                            .child(format!("{:02}", (start_hour + h) % 24))
                    })),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .children(layout.placements.iter().filter_map(|placement| {
                        let block = timeline.blocks.get(placement.block as usize)?;
                        let left = placement.offset_minutes as f32 / span_minutes;
                        let width = (placement.minutes as f32 / span_minutes).max(0.004);
                        let top = placement.lane as f32 * 26.;
                        Some(
                            div()
                                .absolute()
                                .left(gpui::relative(left))
                                .w(gpui::relative(width))
                                .top(px(top + 4.))
                                .h(px(22.))
                                .rounded(px(3.))
                                .bg(colour_of(&block.task_id).opacity(0.35))
                                .border_1()
                                .border_color(colour_of(&block.task_id))
                                .px_1()
                                .text_size(px(10.))
                                .truncate()
                                .child(block.title.clone()),
                        )
                    }))
                    .h(px(lanes as f32 * 26. + 8.)),
            );
        let total = shaped.total_seconds / 60;
        let side =
            div()
                .id("timeline-side")
                .w(px(260.))
                .flex_none()
                .h_full()
                .border_l_1()
                .border_color(t.border_variant)
                .overflow_y_scroll()
                .p_2()
                .child(div().pb_2().child(format!(
                    "{day} · {}h {:02}m · {:.0} pts",
                    total / 60,
                    total % 60,
                    shaped.points
                )))
                .children(summaries.iter().enumerate().map(
                    |(ix, (title, task, seconds, blocks))| {
                        let here = ix == timeline.cursor;
                        let minutes = seconds / 60;
                        div()
                            .id(("timeline-task", ix))
                            .px_1()
                            .py(px(2.))
                            .flex()
                            .gap_2()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if here && active {
                                t.accent
                            } else {
                                gpui::transparent_black()
                            })
                            .when(here, |d| d.bg(t.selected))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.app.timeline.cursor = ix;
                                this.app.region = Region::Timeline;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .w(px(8.))
                                    .h(px(8.))
                                    .mt(px(5.))
                                    .rounded(px(2.))
                                    .bg(colour_of(task)),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(title.clone()))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.placeholder)
                                    .child(format!("{minutes}m · {blocks}")),
                            )
                    },
                ));
        div().flex_1().min_h_0().flex().child(chart).child(side)
    }

    pub(super) fn progress_pane(&self, t: Tokens) -> impl IntoElement {
        let Some(progress) = self.app.progress.clone() else {
            return div()
                .p_3()
                .text_color(t.placeholder)
                .child("No progress yet.");
        };
        let most = progress
            .days
            .iter()
            .map(|d| d.completed.max(d.added))
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        let most_focus = progress
            .days
            .iter()
            .map(|d| d.focus_minutes)
            .max()
            .unwrap_or(1)
            .max(1) as f32;
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .p_2()
            .gap_2()
            .child(div().text_size(px(12.)).text_color(t.muted).child(format!(
                "Last {} days · {} closed · {} added · {}h {:02}m focused",
                progress.days.len(),
                progress.total_completed,
                progress.total_added,
                progress.focus_minutes / 60,
                progress.focus_minutes % 60
            )))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_end()
                    .gap_1()
                    .children(progress.days.iter().enumerate().map(|(ix, day)| {
                        let best = progress.best_day == Some(ix as u32);
                        let label = {
                            use chrono::{Local, TimeZone};
                            Local
                                .timestamp_millis_opt(day.day_start_ms)
                                .single()
                                .map(|d| d.format("%a").to_string())
                                .unwrap_or_default()
                        };
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_end()
                                    .justify_center()
                                    .gap(px(2.))
                                    .h(px(130.))
                                    .child(
                                        div()
                                            .w(px(8.))
                                            .h(gpui::relative(day.completed as f32 / most))
                                            .bg(if best {
                                                t.success
                                            } else {
                                                t.success.opacity(0.7)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .w(px(8.))
                                            .h(gpui::relative(day.added as f32 / most))
                                            .bg(t.muted.opacity(0.5)),
                                    )
                                    .child(
                                        div()
                                            .w(px(8.))
                                            .h(gpui::relative(
                                                day.focus_minutes as f32 / most_focus,
                                            ))
                                            .bg(t.accent.opacity(0.7)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(t.placeholder)
                                    .child(format!("{label} {}", day.completed)),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .child(div().text_color(t.success).child("■ closed"))
                    .child(div().text_color(t.muted).child("■ added"))
                    .child(div().text_color(t.accent).child("■ focus")),
            )
    }
}
