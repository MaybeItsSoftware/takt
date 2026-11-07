//! Takt's desktop app, in Rust on GPUI, styled after Zed. See
//! `docs/rust-desktop-plan.md`. The Swift app under `Takt/` stays beside it.
//!
//! `TAKT_DESKTOP_DB` names the database; without it the app's own is used.
//! `TAKT_DESKTOP_SMOKE=1` drives the model through a set of keys, waits for
//! the first frame, prints what it saw and quits: a check that needs no one
//! at the screen and never types into whatever is in front.

#[macro_use]
extern crate objc;

mod keys;
mod overlay;
mod platform;
mod state;
mod text_field;
mod theme;
mod ui;

use gpui::{App, AppContext, Application};
use std::borrow::Cow;
use std::time::Instant;

fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/Lilex-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/Lilex-Medium.ttf")),
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
    let application = Application::new();
    // The Dock icon clicked with no window: open one.
    application.on_reopen(|cx| {
        if let Some(takt) = cx.try_global::<ui::Shell>().map(|shell| shell.0.clone()) {
            takt.update(cx, |this, cx| this.show_main_window(cx));
        }
    });
    application.run(move |cx: &mut App| {
        load_fonts(cx);
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

/// `TAKT_DESKTOP_SMOKE_PLATFORM=1`: the status item, the hotkeys and the
/// event queue, driven as AppKit would drive them (events queued, never a
/// key sent), then a report and quit.
fn smoke_platform(cx: &mut gpui::Context<ui::Takt>) {
    use platform::Event;
    platform::push(Event::QuickCapture);
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
    }
    app.run("goOutline", None);
    report.push(time("outline j", &vec!["j"; 30], app));
    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some() && app.selected().is_some() {
        let started = Instant::now();
        app.toggle_complete();
        app.toggle_complete();
        report.push(format!(
            "tick and untick {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0 / 2.0
        ));
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
