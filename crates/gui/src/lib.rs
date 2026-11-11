use gpui_kit::{application, component::init};

use crate::{assets::AppAssets, runtime::Tokio, update::Updater, windows::SettingsWindow};

mod assets;
mod consts;
mod credentials;
mod details_panel;
mod format;
mod hotkeys;
mod prefs;
mod runtime;
mod session;
mod speed_limits;
mod startup;
mod theme;
mod torrent_table;
mod update;
mod windows;

/// Run the desktop application.
pub fn run() {
    application().with_assets(AppAssets).run(|cx| {
        init(cx);
        hotkeys::init(cx);

        if let Err(err) = Tokio::init(cx) {
            eprintln!("Failed to start the async runtime: {err}");
        }

        if let Err(err) = theme::init(cx) {
            eprintln!("Failed to load the Catppuccin theme: {err}");
        }

        // Quit once only the settings window (or nothing) is left, so it doesn't keep the
        // app running after the main window closes. Logging in or out opens the next
        // window before closing the old one, so that doesn't count.
        cx.on_window_closed(|cx, _window| {
            let settings = SettingsWindow::handle(cx);
            if cx.windows().iter().all(|handle| Some(*handle) == settings) {
                cx.quit();
            }
        })
        .detach();

        startup::start(cx);
        Updater::check_on_startup(cx);
    });
}
