//! Takt's desktop app, in Rust on GPUI, styled after Zed. See
//! `docs/rust-desktop-plan.md`.
//!
//! `TAKT_DESKTOP_DB` names the database; without it the app's own is used.
//! `TAKT_DESKTOP_SMOKE=1` drives the model through a set of keys, waits for
//! the first frame, prints what it saw and quits: a check that needs no one
//! at the screen and never types into whatever is in front.

#[macro_use]
extern crate objc;

mod google;
mod keys;
mod overlay;
mod platform;
mod state;
mod sync;
mod text_field;
mod theme;
mod ui;

use gpui::{App, AppContext, Application};
use std::borrow::Cow;
use std::time::{Duration, Instant};

fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../shared/fonts/IBMPlexSans-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../shared/fonts/IBMPlexSans-Medium.ttf")),
        Cow::Borrowed(include_bytes!(
            "../../shared/fonts/IBMPlexSans-SemiBold.ttf"
        )),
        Cow::Borrowed(include_bytes!("../../shared/fonts/IBMPlexSans-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../../shared/fonts/IBMPlexSans-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../../shared/fonts/Lilex-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../shared/fonts/Lilex-Medium.ttf")),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("takt-desktop: could not load the bundled fonts: {error}");
    }
}

fn main() {
    // Before anything else: an MCP client's process speaks JSON-RPC on
    // stdio and never needs a window. `MCPServerShim.swift`.
    let args: Vec<String> = std::env::args().collect();
    if let Some(ix) = args.iter().position(|arg| arg == "--mcp-server") {
        platform::exec_mcp_server(&args[ix..]);
    }
    let launched = Instant::now();
    let app = match state::App::open() {
        Ok(app) => app,
        Err(error) => {
            eprintln!("takt-desktop: {error}");
            std::process::exit(1);
        }
    };
    let smoke = std::env::var_os("TAKT_DESKTOP_SMOKE").is_some();
    let application = Application::new().with_assets(ui::icons::Assets);
    // The Dock icon clicked with no window: open one.
    application.on_reopen(|cx| {
        if let Some(takt) = cx.try_global::<ui::Shell>().map(|shell| shell.0.clone()) {
            takt.update(cx, |this, cx| this.show_main_window(cx));
        }
    });
    // Supabase sending the browser back after a sign-in.
    application.on_open_urls(|urls| {
        for url in urls {
            platform::push(platform::Event::Url(url));
        }
    });
    application.run(move |cx: &mut App| {
        load_fonts(cx);
        ui::menus::install(cx);
        let takt = cx.new(|cx| ui::Takt::new(app, cx));
        cx.set_global(ui::Shell(takt.clone()));
        let Some(window) = ui::open_main(&takt, cx) else {
            eprintln!("takt-desktop: couldn't open the window");
            cx.quit();
            return;
        };
        cx.on_window_closed(ui::window_closed).detach();
        window
            .update(cx, |view, window, cx| {
                if smoke {
                    smoke_test(&mut view.app, window, launched);
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_ENTER").is_some() {
                        let keys = view.smoke_new_tasks(window, cx);
                        println!("smoke enter: {}", keys.join(", "));
                        smoke_frames(window);
                    }
                    if let Ok(page) = std::env::var("TAKT_DESKTOP_SMOKE_SETTINGS_PAGE") {
                        let row = std::env::var("TAKT_DESKTOP_SMOKE_SETTINGS_ROW")
                            .ok()
                            .and_then(|r| r.parse().ok())
                            .unwrap_or(0);
                        let found = view.smoke_settings_page(&page, row, window, cx);
                        println!("smoke settings page {page}: {found}");
                    }
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_SETTINGS").is_some() {
                        let said = view.smoke_settings(window, cx);
                        println!("smoke settings: {}", said.join(" | "));
                        // After the deferred open: the window, then Esc in it.
                        let takt = cx.entity();
                        cx.defer(move |cx| {
                            let handle = takt.read(cx).settings_window_handle();
                            let windows = cx.windows().len();
                            let closed = handle.and_then(|handle| {
                                handle
                                    .update(cx, |view, window, cx| {
                                        view.smoke_settings_close(window, cx)
                                    })
                                    .ok()
                            });
                            println!(
                                "smoke settings window: {} windows, {}",
                                windows,
                                closed.unwrap_or_else(|| "no window".into())
                            );
                            // Again with one real Esc in the window, as a
                            // person presses it: still one main window.
                            // TAKT_DESKTOP_SMOKE_SETTINGS_ALONE: with the main window
                            // closed first, as when Takt lives in the menu bar.
                            if std::env::var_os("TAKT_DESKTOP_SMOKE_SETTINGS_ALONE").is_some()
                                && let Some(main) = takt.read(cx).main_window_handle()
                            {
                                let _ = cx.update_window(main.into(), |_, window, _| {
                                    window.remove_window()
                                });
                            }
                            // Twice, as ⌘, arrives by the menu's key and the
                            // window's both: still one settings window.
                            let before = cx.windows().len();
                            for _ in 0..2 {
                                takt.update(cx, |this, cx| {
                                    this.open_settings_page();
                                    this.open_requested_windows(cx);
                                });
                            }
                            let takt = takt.clone();
                            cx.spawn(async move |cx| {
                                cx.background_executor()
                                    .timer(Duration::from_millis(400))
                                    .await;
                                let _ = cx.update(|cx| {
                                    println!(
                                        "smoke settings opened twice: {} new window(s)",
                                        cx.windows().len() as isize - before as isize
                                    );
                                });
                                let _ = cx.update(|cx| {
                                    let handle = takt.read(cx).settings_window_handle();
                                    if let Some(handle) = handle {
                                        let _ = cx.update_window(handle.into(), |_, window, cx| {
                                            // TAKT_DESKTOP_SMOKE_SETTINGS_KEY: the key
                                            // that closes it, Esc unless named (cmd-w).
                                            let key = std::env::var("TAKT_DESKTOP_SMOKE_SETTINGS_KEY")
                                                .unwrap_or_else(|_| "escape".into());
                                            window.dispatch_keystroke(
                                                gpui::Keystroke::parse(&key).expect("a key"),
                                                cx,
                                            );
                                        });
                                    }
                                });
                                cx.background_executor()
                                    .timer(Duration::from_millis(600))
                                    .await;
                                let _ = cx.update(|cx| {
                                    let main = takt.read(cx).main_window_id();
                                    let mains = cx
                                        .windows()
                                        .iter()
                                        .filter(|w| Some(w.window_id()) == main)
                                        .count();
                                    for w in cx.windows() {
                                        let size = w
                                            .update(cx, |_, window, _| window.bounds().size)
                                            .ok();
                                        println!("smoke window left: {:?} {size:?}", w.window_id());
                                    }
                                    println!(
                                        "smoke settings close key: {} windows after ({} main, settings gone {}), settings page {}",
                                        cx.windows().len(),
                                        mains,
                                        takt.read(cx).settings_gone(cx),
                                        takt.read(cx).app.settings_page.is_some()
                                    );
                                });
                            })
                            .detach();
                        });
                    }
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some() {
                        view.smoke_pill(cx);
                    }
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_PANEL").is_some() {
                        view.app.wants_panel = Some(true);
                        view.open_requested_windows(cx);
                    }
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_CAPTURE").is_some() {
                        view.app.wants_capture = true;
                        view.open_requested_windows(cx);
                    }
                    if std::env::var_os("TAKT_DESKTOP_SMOKE_PLATFORM").is_some() {
                        view.start_platform(cx);
                        smoke_platform(cx);
                    }
                } else {
                    view.start_platform(cx);
                }
            })
            .ok();
    });
}

/// The gaps between the next frames, for a freeze that only shows once
/// the window draws.
fn smoke_frames(window: &mut gpui::Window) {
    fn next(window: &mut gpui::Window, last: Instant, gaps: Vec<f64>) {
        window.on_next_frame(move |window, _| {
            let mut gaps = gaps;
            gaps.push(last.elapsed().as_secs_f64() * 1000.0);
            if gaps.len() < 12 {
                window.refresh();
                next(window, Instant::now(), gaps);
            } else {
                let worst = gaps.iter().copied().fold(0., f64::max);
                println!("smoke frames: worst {worst:.1} ms, all {gaps:.1?}");
            }
        });
    }
    next(window, Instant::now(), Vec::new());
}

/// `TAKT_DESKTOP_SMOKE_PLATFORM=1`: the status item, the hotkeys and the
/// event queue, driven as AppKit would drive them (events queued, never a
/// key sent), then a report and quit.
fn smoke_platform(cx: &mut gpui::Context<ui::Takt>) {
    use platform::Event;
    platform::push(Event::FocusPanel);
    platform::push(Event::Menu("focus-pause".into()));
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(1200))
            .await;
        this.update(cx, |takt, cx| {
            println!("platform: {}", takt.platform_report(cx));
        })
        .ok();
        cx.update(|cx| cx.quit()).ok();
    })
    .detach();
}

/// Walks the sidebar, the outline, the board and the matrix the way the keys
/// would, timing each, then reports on the first frame. With
/// `TAKT_DESKTOP_SMOKE_WRITE=1` it also ticks a task off and back.
fn smoke_test(app: &mut state::App, window: &mut gpui::Window, launched: Instant) {
    let time = |label: &'static str, keys: &[&str], app: &mut state::App| {
        let mut samples = Vec::new();
        for key in keys {
            let started = Instant::now();
            app.handle_key(key, Instant::now());
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let median = samples.get(samples.len() / 2).copied().unwrap_or_default();
        let worst = samples.last().copied().unwrap_or_default();
        format!("{label} median {median:.2} worst {worst:.2} ms")
    };
    let rows = app.sidebar_rows.len();
    let mut report = vec![format!("sidebar rows {rows}")];
    app.region = state::Region::Sidebar;
    report.push(time("sidebar j", &vec!["j"; rows.min(30)], app));
    report.push(time("sidebar k", &vec!["k"; rows.min(30)], app));
    app.region = state::Region::Tasks;
    report.push(time("everything", &["shift+a"], app));
    for (mode, key) in [("board", "2"), ("outline", "3"), ("matrix", "4")] {
        let started = Instant::now();
        app.run(
            match mode {
                "board" => "goBoard",
                "outline" => "goOutline",
                _ => "goMatrix",
            },
            Some(key),
        );
        report.push(format!(
            "{mode} {:.2} ms ({} rows)",
            started.elapsed().as_secs_f64() * 1000.0,
            app.visible_ids().len()
        ));
        if mode == "board" {
            let cards: Vec<_> = app.board.cards.iter().flatten().cloned().collect();
            let (mut rows, mut elsewhere, mut hints) = (0, 0, 0);
            for card in &cards {
                let (subtree, _) = app.card_subtree(card);
                rows += subtree.len();
                elsewhere += subtree.iter().filter(|r| r.elsewhere.is_some()).count();
                hints += usize::from(app.card_parent_hint(card).is_some());
            }
            report.push(format!(
                "card subtasks {rows} rows, {elsewhere} in other columns, {hints} parent hints"
            ));
            // `l` into a card's subtask rows, `j` along them, `h` back out.
            let held = app
                .board
                .cards
                .iter()
                .enumerate()
                .find_map(|(column, cards)| {
                    cards
                        .iter()
                        .find(|card| app.card_row_ids(card).len() > 1)
                        .map(|card| (column, card.clone()))
                });
            if let Some((column, card)) = held {
                let ids = app.card_row_ids(&card);
                app.board.column = column;
                app.selected_task = Some(card.id.clone());
                app.run("motionBoardIntoCard", Some("l"));
                let into = app.selected_task.as_deref() == ids.first().map(String::as_str)
                    && app.visible_ids() == ids;
                app.run("motionSelectNext", Some("j"));
                let along = app.selected_task.as_deref() == ids.get(1).map(String::as_str);
                app.run("motionBoardOutOfCard", Some("h"));
                let out = app.selected_task.as_deref() == Some(card.id.as_str());
                report.push(format!("card rows: into {into}, along {along}, out {out}"));
            }
        }
    }
    app.run("goOutline", None);
    // ⌘B and ⌘R as Zed's: hidden, it opens with the keyboard; up without
    // the keyboard, it takes it; with it, it goes away.
    {
        use state::Region;
        let mut steps = Vec::new();
        for (command, inside) in [
            ("windowToggleSidebar", Region::Sidebar),
            ("windowToggleRightDock", Region::Inspector),
        ] {
            let visible = |app: &state::App| match command {
                "windowToggleSidebar" => app.prefs.sidebar_visible,
                _ => app.prefs.right_dock_visible,
            };
            if command == "windowToggleSidebar" {
                app.prefs.left_dock_tab = "lists".into();
                app.prefs.sidebar_visible = false;
            } else {
                app.prefs.right_dock_tab = "inspector".into();
                app.prefs.right_dock_visible = false;
            }
            app.region = Region::Tasks;
            app.run(command, None);
            let opened = visible(app) && app.region == inside;
            app.region = Region::Tasks;
            app.run(command, None);
            let focused = visible(app) && app.region == inside;
            app.run(command, None);
            let away = !visible(app) && app.region == Region::Tasks;
            steps.push(format!(
                "{command} opens {opened}, focuses {focused}, closes {away}"
            ));
        }
        app.prefs.sidebar_visible = true;
        // A folder clicked opens, and again closes.
        if let Some(row) = app
            .sidebar_rows
            .iter()
            .find(|r| matches!(r.kind, state::sidebar::RowKind::Folder(_)))
            .cloned()
            && let state::sidebar::RowKind::Folder(id) = &row.kind
        {
            let before = app.prefs.expanded_folders.contains(id);
            app.click_sidebar_row(&row);
            let flipped = app.prefs.expanded_folders.contains(id) != before;
            app.click_sidebar_row(&row);
            let back = app.prefs.expanded_folders.contains(id) == before;
            steps.push(format!("folder click opens and closes {}", flipped && back));
        }
        report.push(format!("docks: {}", steps.join(", ")));
    }
    // The status bar's history: the last thing done, what ⌘Z undid, and a
    // step chosen from the list going back to it.
    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some()
        && let Some(id) = app.selected().map(|t| t.id.clone())
    {
        let title = app.selected().map(|t| t.title.clone()).unwrap_or_default();
        app.write(|store| {
            store.core.update_task(
                id.clone(),
                format!("{title} (smoke)"),
                String::new(),
                None,
                None,
                state::store::now_ms(),
                "UTC".into(),
            )
        });
        let done = app.history_status();
        app.undo(false);
        let undid = app.history_status();
        let restored = app.selected().map(|t| t.title.clone()) == Some(title.clone());
        app.undo(true);
        let redid = app.history_status();
        app.open_history_menu(0., 0.);
        let listed = app
            .context_menu
            .as_ref()
            .map(|m| m.items().count())
            .unwrap_or(0);
        app.context_menu = None;
        // Choosing the step below the newest takes the newest back.
        app.pick_from_menu("history:1");
        let back = app.selected().map(|t| t.title.clone()) == Some(title.clone());
        report.push(format!(
            "history: done {done:?}, undid {undid:?} (title back {restored}), redid {redid:?}, menu {listed} items, step back {back}"
        ));
    }
    app.run("goOutline", None);
    report.push(time("outline j", &vec!["j"; 30], app));
    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some() && app.selected().is_some() {
        let started = Instant::now();
        // From the top, so there is a row after it to land on.
        app.select_row(0);
        let ticked = app.selected().map(|t| t.id.clone()).unwrap_or_default();
        let at = app.selected_index();
        let next = at.and_then(|i| app.visible_ids().get(i + 1).cloned());
        app.toggle_complete();
        let celebrated = app.celebrating(&ticked).is_some();
        let stays = app.visible_ids().contains(&ticked);
        if let Some(since) = app.lingering_since.get_mut(&ticked) {
            *since -= state::outline::LINGER_MS;
        }
        app.expire_lingering();
        let leaves = !app.visible_ids().contains(&ticked);
        let kept_place = next.is_some() && app.selected_task == next;
        report.push(format!(
            "finished task stays at first: {stays}, gone after its seconds: {leaves}, selection moves to the next row {kept_place}"
        ));
        app.set_status(&ticked, "open");
        app.selected_task = Some(ticked);
        report.push(format!(
            "celebration {:?} played: {celebrated}",
            app.celebration_style()
        ));
        report.push(format!(
            "tick and untick {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0 / 2.0
        ));
        // A drag: the selected row dropped before its previous sibling,
        // then put back.
        if let Some(task) = app.selected().cloned() {
            let sibling = app
                .rows
                .iter()
                .map(|row| &row.task)
                .filter(|t| t.parent_task_id == task.parent_task_id && t.list_id == task.list_id)
                .take_while(|t| t.id != task.id)
                .last()
                .map(|t| t.id.clone());
            if let Some(sibling) = sibling {
                app.drop_task_before(&task.id, &sibling);
                let order: Vec<&str> = app.rows.iter().map(|r| r.task.id.as_str()).collect();
                let moved = order.iter().position(|id| *id == task.id)
                    < order.iter().position(|id| *id == sibling);
                app.undo(false);
                report.push(format!("drag before a sibling: moved {moved}"));
            }
        }
        let started = Instant::now();
        app.apply_typed("due tomorrow 4pm");
        app.apply_typed("tag smoke");
        let id = app.selected_task.clone().unwrap_or_default();
        let snapshot = app.store.core.editor_snapshot(id).ok();
        app.undo(false);
        app.undo(false);
        report.push(format!(
            "typed due and tag {:.2} ms (due set: {}, tagged: {})",
            started.elapsed().as_secs_f64() * 1000.0 / 2.0,
            snapshot.as_ref().is_some_and(|s| s.due_at_ms.is_some()),
            snapshot
                .as_ref()
                .is_some_and(|s| s.metadata.tags.iter().any(|t| t == "smoke")),
        ));
        let started = Instant::now();
        let stops = app.inspector_stops().len();
        app.edit_in_inspector("estimate");
        app.editing = None;
        app.commit_inspector_field("estimate", "45");
        app.commit_inspector_field("notes", "first line\nsecond");
        let snapshot = app.inspected();
        app.undo(false);
        app.undo(false);
        report.push(format!(
            "inspector {stops} stops, estimate and notes {:.2} ms (estimate {:?}, notes lines {})",
            started.elapsed().as_secs_f64() * 1000.0 / 2.0,
            snapshot.as_ref().and_then(|s| s.estimate_seconds),
            snapshot.as_ref().map_or(0, |s| s.notes.lines().count()),
        ));
        smoke_notes_editor(app, &mut report);
        smoke_container_inspector(app, &mut report);
        // A linked task's links reach the inspector with their kinds, for
        // the icons drawn beside them.
        let linked = app
            .store
            .core
            .linked_task_ids()
            .ok()
            .and_then(|ids| ids.into_iter().next());
        if let Some(id) = linked
            && let Ok(Some(task)) = app.store.core.task(id.clone())
        {
            let back = app.selected_task.clone();
            app.reveal_task(&id, &task.list_id);
            let kinds: Vec<String> = app
                .inspector_stops()
                .into_iter()
                .find(|s| s.id == "title")
                .map(|s| s.links.into_iter().map(|l| l.icon).collect())
                .unwrap_or_default();
            report.push(format!("inspector links: {}", kinds.join(" ")));
            if let Some(back) = back
                && let Ok(Some(task)) = app.store.core.task(back.clone())
            {
                app.reveal_task(&back, &task.list_id);
            }
        }
        // The calendar under Due: a day on, set; then cleared. The repeat
        // selector: every 3 weeks by its keys, then none.
        if app.selected_task.is_some() {
            let value = |app: &mut state::App, id: &str| {
                app.inspector_stops()
                    .into_iter()
                    .find(|s| s.id == id)
                    .map(|s| s.value)
                    .unwrap_or_default()
            };
            let due_before = value(app, "due");
            let started = Instant::now();
            app.open_date_edit("due");
            let opened = matches!(app.editing, Some(state::Editing::Date(_)));
            for key in ["t", "right", "enter"] {
                app.date_key(key);
            }
            let set = value(app, "due");
            app.open_date_edit("due");
            app.date_key("backspace");
            let cleared = value(app, "due");
            // Due at a typed time, then a slot scheduled with its length:
            // Tab from the days to the time, and on to how long.
            app.open_date_edit("due");
            for key in ["t", "tab", "1", "7", "enter"] {
                app.date_key(key);
            }
            let due_timed = value(app, "due");
            app.date_key("escape");
            app.open_date_edit("due");
            app.date_key("backspace");
            let estimate_before = app.inspected().and_then(|s| s.estimate_seconds);
            let scheduled_before = value(app, "start");
            app.open_date_edit("start");
            for key in ["t", "right", "tab", "1", "4", "tab", "4", "5", "enter"] {
                app.date_key(key);
            }
            let scheduled = value(app, "start");
            app.open_date_edit("start");
            app.date_key("backspace");
            // The estimate the slot's length wrote is put back.
            let estimate_text = estimate_before
                .map(|seconds| (seconds / 60).to_string())
                .unwrap_or_default();
            app.commit_inspector_field("estimate", &estimate_text);
            // The task's own agent context, apart from its notes.
            let notes_before = app.inspector_field_text("notes");
            app.commit_inspector_field("agent-context", "Smoke: warm up first.");
            let context = app.inspector_field_text("agent-context");
            let notes_kept = app.inspector_field_text("notes") == notes_before;
            app.commit_inspector_field("agent-context", "");
            report.push(format!(
                "agent context saved {:?}, notes kept {notes_kept}, cleared {:?}",
                context,
                app.inspector_field_text("agent-context"),
            ));
            // Time worked without the timer.
            let focus_before = value(app, "focus");
            app.commit_inspector_field("log", "25m");
            report.push(format!(
                "log time: {:?} -> {:?}",
                focus_before,
                value(app, "focus")
            ));
            app.open_repeat_edit();
            let selector = matches!(app.editing, Some(state::Editing::Repeat(_)));
            for key in ["3", "right", "down", "enter"] {
                app.repeat_key(key);
            }
            let repeating = value(app, "repeat");
            app.open_repeat_edit();
            app.repeat_key("backspace");
            report.push(format!(
                "inspector dates {:.2} ms: calendar {opened}, due {due_before:?} -> {set:?} -> {cleared:?}, timed {due_timed:?}; scheduled {scheduled_before:?} -> {scheduled:?} -> {:?}; repeat selector {selector}, set {repeating:?}, now {:?}",
                started.elapsed().as_secs_f64() * 1000.0,
                value(app, "start"),
                value(app, "repeat"),
            ));
            // The commitment form, worked by its keys and left unsaved.
            app.begin_commitment();
            let form = matches!(app.editing, Some(state::Editing::Commitment));
            for key in [
                "down", "right", "right", "tab", "right", "space", "tab", "right",
            ] {
                app.commitment_key(key);
            }
            let rule = app.commitment_form.as_ref().map(|f| f.rule());
            let until = app.commitment_form.as_ref().and_then(|f| f.until);
            app.commitment_key("escape");
            app.close_commitment_form();
            report.push(format!(
                "commitment form {form}: rule {rule:?}, until {until:?}, closed {}",
                app.editing.is_none()
            ));
        }
        let started = Instant::now();
        app.start_focus_on_selected(true);
        let running = app
            .focus_status()
            .map(|(title, clock, _)| format!("{title} {clock}"));
        app.stop_focus();
        report.push(format!(
            "focus start and stop {:.2} ms (ran: {}, after: {})",
            started.elapsed().as_secs_f64() * 1000.0,
            running.unwrap_or_else(|| "nothing".into()),
            if app.focus_session.is_some() {
                "still running"
            } else {
                "stopped"
            }
        ));
    }
    {
        let started = Instant::now();
        app.run("focusSetAvailable", None);
        let rows = app.condition_views().len();
        app.run("motionConditionsSelect", Some("j"));
        app.run("windowToggleDoneRail", None);
        app.run("goTimeline", None);
        let tasks = app.timeline.summaries().len();
        app.run("windowDockNextTab", Some("]"));
        let progress = app.progress.as_ref().map_or(0, |p| p.days.len());
        app.run("windowToggleProgressDock", None);
        app.run("windowToggleRightDock", None);
        report.push(format!(
            "conditions {rows} rows, timeline {tasks} tasks, progress {progress} days {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0
        ));
    }
    if let Some(id) = app.selected_task.clone() {
        app.open_task_menu(&id, 100., 100.);
        let task_items = app.context_menu.as_ref().map_or(0, |m| m.items().count());
        let first = app.sidebar_rows.get(2).map(|r| r.id.clone());
        if let Some(row) = first {
            app.open_sidebar_menu(&row, 10., 10.);
        }
        let sidebar_items = app.context_menu.as_ref().map_or(0, |m| m.items().count());
        app.context_menu = None;
        report.push(format!(
            "menus: task {task_items} items, sidebar {sidebar_items}"
        ));
    }
    {
        let started = Instant::now();
        app.index_obsidian();
        let indexed = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let (notes, _, _) = app.link_candidates("daily");
        report.push(format!(
            "link picker: {} vaults, {} entries in {indexed:.1} ms, \"daily\" {} rows in {:.2} ms",
            app.links.vaults.len(),
            app.links.entries.len(),
            notes.len(),
            started.elapsed().as_secs_f64() * 1000.0
        ));
    }
    report.push(format!(
        "{} themes, {} settings, {} diagnostics",
        app.library.themes.len(),
        app.settings_rows().len(),
        app.diagnostics_rows().len()
    ));
    if std::env::var_os("TAKT_DESKTOP_SMOKE_AGENT").is_some() {
        let started = Instant::now();
        app.send_to_agent("How many lists are in the sidebar? Answer with the number only.");
        while app.agent.working && started.elapsed().as_secs() < 120 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            app.drain_agent();
        }
        let said: Vec<String> = app
            .agent
            .entries
            .iter()
            .map(|entry| format!("{entry:?}").chars().take(120).collect())
            .collect();
        report.push(format!(
            "agent {:.1} s: {}",
            started.elapsed().as_secs_f64(),
            said.join(" | ")
        ));
    }
    if std::env::var_os("TAKT_DESKTOP_SMOKE_QUICK_ASK").is_some() && app.selected().is_some() {
        let started = Instant::now();
        app.open_quick_ask();
        app.send_quick_ask("What is this task's title? Answer with the title only.");
        while app.quick_agent.working && started.elapsed().as_secs() < 120 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            app.drain_agent();
        }
        report.push(format!(
            "quick ask {:.1} s (panel untouched: {}): {}",
            started.elapsed().as_secs_f64(),
            app.agent.entries.is_empty(),
            app.quick_ask_transcript().replace('\n', " | ")
        ));
        app.editing = None;
    }
    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some() {
        app.overlay = None;
        app.editing = None;
        app.region = state::Region::Tasks;
        app.run("goOutline", None);
        let mut said = Vec::new();
        for n in 0..3 {
            let started = Instant::now();
            app.run("taskNew", None);
            let opened = started.elapsed().as_secs_f64() * 1000.0;
            let drafted = app
                .draft_inspector_stops("smoke draft")
                .is_some_and(|stops| stops[0].value == "smoke draft");
            let started = Instant::now();
            app.create_task(&format!("smoke new task {n}"));
            let created = started.elapsed().as_secs_f64() * 1000.0;
            // Return closes the draft and leaves the new task selected.
            let closed = app.editing.is_none()
                && app
                    .selected()
                    .is_some_and(|task| task.title == format!("smoke new task {n}"));
            let started = Instant::now();
            let today = state::today::compute(
                app.store.core.clone(),
                app.store.workspace_id.clone(),
                app.store.zone.clone(),
            )
            .is_ok();
            said.push(format!(
                "open {opened:.1} create {created:.1} today {:.1} ms ({today}, drafted {drafted}, closed {closed})",
                started.elapsed().as_secs_f64() * 1000.0
            ));
        }
        app.editing = None;
        report.push(format!("new task: {}", said.join(", ")));
    }
    // A subtask filed into the next column, selected, and the board left
    // up: the shot then shows its link's line.
    if std::env::var_os("TAKT_DESKTOP_SMOKE_LINK").is_some() {
        app.overlay = None;
        app.editing = None;
        app.region = state::Region::Tasks;
        app.select_everything();
        app.run("goBoard", Some("2"));
        let found = app
            .board
            .cards
            .iter()
            .enumerate()
            .find_map(|(column, cards)| {
                cards.iter().find_map(|card| {
                    let (rows, _) = app.card_subtree(card);
                    rows.iter()
                        .find(|row| row.task.status == "open")
                        .map(|row| (column, row.task.id.clone()))
                })
            });
        if let Some((column, child)) = found {
            app.selected_task = Some(child.clone());
            app.board.column = column;
            let by = if column + 1 < app.board.columns.len() {
                1
            } else {
                -1
            };
            app.move_card_to_adjacent_column(by);
            let elsewhere: usize = app
                .board
                .cards
                .iter()
                .flatten()
                .map(|card| {
                    app.card_subtree(card)
                        .0
                        .iter()
                        .filter(|r| r.elsewhere.is_some())
                        .count()
                })
                .sum();
            let now_in = app
                .board
                .cards
                .iter()
                .position(|cards| cards.iter().any(|c| c.id == child));
            report.push(format!(
                "link: a subtask filed {by:+} column is a card in column {now_in:?}, {elsewhere} rows name another column"
            ));
            app.selected_task = Some(child);
        }
    }
    if let Some(message) = &app.message {
        report.push(format!("message: {message}"));
    }
    let shot = std::env::var("TAKT_DESKTOP_SMOKE_SHOT").ok();
    window.on_next_frame(move |_, cx| {
        println!(
            "smoke: {}; first frame {:.0} ms after launch",
            report.join("; "),
            launched.elapsed().as_secs_f64() * 1000.0
        );
        match shot {
            // A picture of the screen a moment after, for checking the
            // drawing by eye; the screen only, never a key sent.
            Some(path) => {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1500))
                        .await;
                    println!(
                        "smoke: board link lines drawn {}",
                        ui::board_links::painted()
                    );
                    let _ = std::process::Command::new("/usr/sbin/screencapture")
                        .args(["-x", &path])
                        .status();
                    cx.update(|cx| cx.quit()).ok();
                })
                .detach();
            }
            // The platform check quits once it has looked.
            None if std::env::var_os("TAKT_DESKTOP_SMOKE_PLATFORM").is_some() => {}
            None => cx.quit(),
        }
    });
}

/// The Markdown editor on the selected task: a note in a scratch vault
/// linked, its file made the notes, typed into and saved, then changed on
/// disk mid-edit, which the editor must not write over. Everything is put
/// back: the link undone, the settings as they were, the vault removed.
/// The inspector on a list from the sidebar: its rows, a rename and a move
/// to a folder through them, each undone.
fn smoke_container_inspector(app: &mut state::App, report: &mut Vec<String>) {
    use state::inspector_container::Subject;
    let Some(list) = app
        .lists
        .iter()
        .find(|l| l.system_role.is_none() && !l.is_archived)
        .cloned()
    else {
        return;
    };
    let (scope, region, selected, container) = (
        app.scope.clone(),
        app.region,
        app.selected_task.clone(),
        app.inspecting_container,
    );
    app.select_list(&list.id);
    app.region = state::Region::Sidebar;
    let on_list = app.inspector_subject() == Some(Subject::List(list.id.clone()));
    let ids: Vec<&str> = app.inspector_stops().iter().map(|s| s.id).collect();
    let ids = ids.join(" ");
    app.focus_region(state::Region::Inspector);
    let stays = app.inspector_subject() == Some(Subject::List(list.id.clone()));
    app.commit_inspector_field("title", &format!("{} smoke", list.name));
    let renamed = app
        .list(&list.id)
        .is_some_and(|l| l.name == format!("{} smoke", list.name));
    app.undo(false);
    let mut moved = None;
    if !app.folders.is_empty()
        && let Some(ix) = app
            .inspector_stops()
            .iter()
            .position(|s| s.id == "container-folder")
    {
        app.inspector_cursor = ix;
        app.activate_inspector();
        moved =
            Some(app.list(&list.id).map(|l| l.folder_id.clone()) != Some(list.folder_id.clone()));
        app.undo(false);
    }
    let restored = app
        .list(&list.id)
        .is_some_and(|l| l.name == list.name && l.folder_id == list.folder_id);
    app.focus_region(state::Region::Tasks);
    let task_again = !matches!(
        app.inspector_subject(),
        Some(Subject::List(_)) if app.selected_task.is_some()
    );
    app.scope = scope;
    app.region = region;
    app.inspecting_container = container;
    app.selected_task = selected;
    app.reload_rows();
    report.push(format!(
        "list inspector (from {region:?}): on the list {on_list}, stays {stays}, rows [{ids}], renamed {renamed}, moved {moved:?}, undone {restored}, task again {task_again}"
    ));
}

fn smoke_notes_editor(app: &mut state::App, report: &mut Vec<String>) {
    let Some(task_id) = app.selected_task.clone() else {
        return;
    };
    let started = Instant::now();
    let root = std::env::temp_dir().join(format!("takt-smoke-vault-{}", std::process::id()));
    let vault = root.join("Smoke");
    let note = vault.join("Plan.md");
    if std::fs::create_dir_all(&vault).is_err() || std::fs::write(&note, "# Plan\n").is_err() {
        return;
    }
    let kept_vaults = app.prefs.links_obsidian_vaults.clone();
    let kept_plugins = app.prefs.plugins.clone();
    app.prefs
        .links_obsidian_vaults
        .push(vault.to_string_lossy().into_owned());
    app.prefs.plugins.insert("markdown-editor".into(), true);
    app.prefs.plugins.insert("obsidian".into(), true);
    let url = takt_core::links::obsidian_link_url("Smoke".into(), "Plan.md".into(), false);
    let now = state::store::now_ms();
    let linked = app
        .write(|s| s.core.add_task_link(task_id.clone(), url, None, None, now))
        .is_some();
    app.open_notes_editor();
    let additional = app
        .notes_editor
        .as_ref()
        .is_some_and(|e| !e.same && e.note.is_some() && e.note_text.is_some());
    let own = app
        .notes_editor
        .as_ref()
        .map(|e| e.saved.clone())
        .unwrap_or_default();
    let switched = app.switch_notes_source(&own).as_deref() == Some("# Plan\n");
    let typed = "# Plan\n- [ ] step\n";
    let saved = app.save_notes_editor(typed, false)
        && std::fs::read_to_string(&note).ok().as_deref() == Some(typed);
    // Obsidian writes while something else is typed here.
    let _ = std::fs::write(&note, "# Plan\nfrom Obsidian, longer\n");
    let mine = "# Plan\n- [ ] step\nmine\n";
    app.poll_notes_editor(mine);
    let refused = !app.save_notes_editor(mine, false)
        && std::fs::read_to_string(&note)
            .ok()
            .is_some_and(|t| t.contains("from Obsidian"));
    let theirs = app.take_their_notes().unwrap_or_default();
    let closed = app.close_notes_editor(&theirs).is_none();
    let notes_kept = app
        .store
        .core
        .editor_snapshot(task_id.clone())
        .is_ok_and(|s| s.notes == own);
    app.prefs.notes_same_as_obsidian.remove(&task_id);
    if linked {
        app.undo(false);
    }
    app.prefs.links_obsidian_vaults = kept_vaults;
    app.prefs.plugins = kept_plugins;
    app.prefs.save();
    let _ = std::fs::remove_dir_all(&root);
    report.push(format!(
        "notes editor {:.1} ms (additional first: {additional}, file became the notes: {switched}, saved to it: {saved}, a change on disk not written over: {refused}, closed: {closed}, Takt's notes kept: {notes_kept})",
        started.elapsed().as_secs_f64() * 1000.0
    ));
}
