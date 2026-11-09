use gpui_kit::{application, component::init};

use crate::{assets::AppAssets, runtime::Tokio};

mod assets;
mod consts;
mod credentials;
mod details_panel;
mod format;
mod hotkeys;
mod runtime;
mod session;
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

        cx.on_window_closed(|cx, _window| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        startup::start(cx);
    });
}
