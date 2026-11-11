use gpui_kit::{application, component::init};

use crate::{
    assets::AppAssets,
    runtime::Tokio,
    windows::{LoginWindow, MainWindow},
};

mod assets;
mod consts;
mod credentials;
mod details_panel;
mod format;
mod hotkeys;
mod runtime;
mod session;
mod speed_limits;
mod startup;
mod theme;
mod torrent_table;
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

        // Quit once neither the main nor the login window is open, so a leftover window
        // like Settings doesn't keep the app running. Logging in or out opens the next
        // window before closing the old one, so that doesn't count.
        cx.on_window_closed(|cx, _window| {
            let primary_open = cx
                .windows()
                .iter()
                .any(|handle| handle.downcast::<MainWindow>().is_some() || handle.downcast::<LoginWindow>().is_some());
            if !primary_open {
                cx.quit();
            }
        })
        .detach();

        startup::start(cx);
    });
}
