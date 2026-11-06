//! Takt's desktop app, in Rust on GPUI, styled after Zed. See
//! `docs/rust-desktop-plan.md`. The Swift app under `Takt/` stays beside it.
//!
//! `TAKT_DESKTOP_DB` names the database; without it the app's own is used.
//! `TAKT_DESKTOP_SMOKE=1` drives the model through a set of keys, waits for
//! the first frame, prints what it saw and quits: a check that needs no one
//! at the screen and never types into whatever is in front.

mod keys;
mod overlay;
mod state;
mod text_field;
mod theme;
mod ui;

use gpui::{
    App, AppContext, Application, Bounds, Focusable, TitlebarOptions, WindowBounds, WindowOptions,
    point, px, size,
};
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
    let launched = Instant::now();
    let app = match state::App::open() {
        Ok(app) => app,
        Err(error) => {
            eprintln!("takt-desktop: {error}");
            std::process::exit(1);
        }
    };
    let smoke = std::env::var_os("TAKT_DESKTOP_SMOKE").is_some();
    Application::new().run(move |cx: &mut App| {
        load_fonts(cx);
        let bounds = Bounds::centered(None, size(px(1200.), px(780.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Takt".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(11.))),
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| ui::Takt::new(app, cx)),
            )
            .expect("open the window");
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
                if smoke {
                    smoke_test(&mut view.app, window, launched);
                }
            })
            .ok();
    });
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
    if let Some(message) = &app.message {
        report.push(format!("message: {message}"));
    }
    window.on_next_frame(move |_, cx| {
        println!(
            "smoke: {}; first frame {:.0} ms after launch",
            report.join("; "),
            launched.elapsed().as_secs_f64() * 1000.0
        );
        cx.quit();
    });
}
