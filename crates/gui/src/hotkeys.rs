use gpui_kit::{App, KeyBinding, Menu, MenuItem, SystemMenuType};

pub use actions::{
    CheckForUpdates, CloseWindow, DeleteTorrent, FocusSearch, Quit, ShowAll, ShowDownloading, ShowSeeding, ShowStopped,
    StartAll, StopAll, ToggleSettings, ToggleTorrent,
};

use crate::{update::Updater, windows::SettingsWindow};

/// The key context of the main window, for hotkeys that only work there.
pub const MAIN_WINDOW: &str = "MainWindow";

/// The torrent table's key context, inside [`MAIN_WINDOW`], so hotkeys like space don't
/// fire while typing in the search box.
const TORRENT_TABLE: &str = "MainWindow > DataTable";

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
            /// Show the settings window, where the result appears, and check for a newer release.
            CheckForUpdates,
            /// Focus the main window's search box, selecting what's in it.
            FocusSearch,
            /// Show every torrent.
            ShowAll,
            /// Show only downloading torrents.
            ShowDownloading,
            /// Show only seeding torrents.
            ShowSeeding,
            /// Show only stopped torrents.
            ShowStopped,
            /// Start the selected torrent if it's stopped, otherwise stop it.
            ToggleTorrent,
            /// Ask whether to delete the selected torrent.
            DeleteTorrent,
            /// Start every torrent.
            StartAll,
            /// Stop every torrent.
            StopAll,
        ]
    );
}

/// `key` with the platform's main modifier: `cmd` on macOS, `ctrl` elsewhere.
macro_rules! primary {
    ($key:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $key)
        } else {
            concat!("ctrl-", $key)
        }
    };
}

/// Register the app-wide hotkeys, what they do, and the menu bar items that show them.
///
/// The main window's hotkeys are handled by the main window itself.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(close_window);
    cx.on_action(|_: &ToggleSettings, cx| SettingsWindow::toggle(cx));
    cx.on_action(|_: &CheckForUpdates, cx| {
        SettingsWindow::show(cx);
        Updater::check(cx);
    });

    let main = Some(MAIN_WINDOW);
    let table = Some(TORRENT_TABLE);
    cx.bind_keys([
        KeyBinding::new(primary!("q"), Quit, None),
        KeyBinding::new(primary!("w"), CloseWindow, None),
        KeyBinding::new(primary!(","), ToggleSettings, None),
        KeyBinding::new(primary!("f"), FocusSearch, main),
        KeyBinding::new(primary!("1"), ShowAll, main),
        KeyBinding::new(primary!("2"), ShowDownloading, main),
        KeyBinding::new(primary!("3"), ShowSeeding, main),
        KeyBinding::new(primary!("4"), ShowStopped, main),
        KeyBinding::new(primary!("alt-r"), StartAll, main),
        KeyBinding::new(primary!("alt-."), StopAll, main),
        KeyBinding::new("space", ToggleTorrent, table),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-backspace"
            } else {
                "delete"
            },
            DeleteTorrent,
            table,
        ),
    ]);

    // The first menu is the app menu on macOS. Each item shows its key binding.
    cx.set_menus([
        Menu::new("qBit").items([
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::action("Settings…", ToggleSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Quit qBit", Quit),
        ]),
        Menu::new("File").items([MenuItem::action("Close Window", CloseWindow)]),
        Menu::new("View").items([
            MenuItem::action("Search", FocusSearch),
            MenuItem::separator(),
            MenuItem::action("All", ShowAll),
            MenuItem::action("Downloading", ShowDownloading),
            MenuItem::action("Seeding", ShowSeeding),
            MenuItem::action("Stopped", ShowStopped),
        ]),
        // Starting or stopping one torrent isn't here: space can't be a menu shortcut,
        // as the menu would take it from the search box.
        Menu::new("Torrents").items([
            MenuItem::action("Delete…", DeleteTorrent),
            MenuItem::separator(),
            MenuItem::action("Start All", StartAll),
            MenuItem::action("Stop All", StopAll),
        ]),
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
