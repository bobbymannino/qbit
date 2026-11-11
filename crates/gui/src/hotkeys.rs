use gpui_kit::{App, KeyBinding, Menu, MenuItem, SystemMenuType};

pub use actions::{CloseWindow, Quit, ToggleSettings};

use crate::windows::SettingsWindow;

// `actions!` derives `PartialEq` without `Eq`, which the workspace lints deny.
#[allow(clippy::derive_partial_eq_without_eq)]
mod actions {
    gpui_kit::actions!(
        qbit,
        [
            /// Quit the app, closing every window.
            Quit,
            /// Close the focused window; closing the main (or login) window quits the app.
            CloseWindow,
            /// Open the settings window, or close it if it's already open.
            ToggleSettings,
        ]
    );
}

/// Register the app-wide hotkeys, what they do, and the menu bar items that show them.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(close_window);
    cx.on_action(|_: &ToggleSettings, cx| SettingsWindow::toggle(cx));

    cx.bind_keys([
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-q" } else { "ctrl-q" }, Quit, None),
        KeyBinding::new(
            if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" },
            CloseWindow,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") { "cmd-," } else { "ctrl-," },
            ToggleSettings,
            None,
        ),
    ]);

    // The first menu is the app menu on macOS. Each item shows its key binding.
    cx.set_menus([
        Menu::new("qBit").items([
            MenuItem::action("Settings…", ToggleSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Quit qBit", Quit),
        ]),
        Menu::new("File").items([MenuItem::action("Close Window", CloseWindow)]),
    ]);
}

fn close_window(_: &CloseWindow, cx: &mut App) {
    let Some(handle) = cx.active_window() else { return };

    cx.defer(move |cx| {
        if let Err(err) = handle.update(cx, |_, window, _| window.remove_window()) {
            eprintln!("Failed to close the window: {err}");
        }
    });
}
