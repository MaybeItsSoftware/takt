//! The Mac's menu bar at the top of the screen: Takt, File and Window, so
//! everything the keys reach first is also there to click. Each item queues
//! the same [`Event`] the status item's menu does, and the window acts on it
//! as it drains the queue.

use crate::platform::{self, Event};
use gpui::{App, KeyBinding, Menu, MenuItem, SystemMenuType, actions};

actions!(
    takt,
    [
        About,
        OpenSettings,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        QuickAdd,
        ShowMainWindow,
        FocusPanel,
    ]
);

fn menu(action: &str) {
    platform::push(Event::Menu(action.into()));
}

pub fn install(cx: &mut App) {
    cx.on_action(|_: &About, cx| {
        cx.open_url("https://www.maybeitssoftware.co.uk/takt");
    });
    cx.on_action(|_: &OpenSettings, _| menu("settings"));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &QuickAdd, _| menu("quick-capture"));
    cx.on_action(|_: &ShowMainWindow, _| menu("show-window"));
    cx.on_action(|_: &FocusPanel, _| menu("focus-panel"));

    // Only the keys every Mac app has; the rest stay the window's own.
    cx.bind_keys([
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-q", Quit, None),
    ]);

    cx.set_menus(vec![
        Menu {
            name: "Takt".into(),
            items: vec![
                MenuItem::action("About Takt", About),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Takt", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Takt", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![MenuItem::action("Quick Add…", QuickAdd)],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Main Window", ShowMainWindow),
                MenuItem::action("Focus Panel", FocusPanel),
            ],
        },
    ]);
}
