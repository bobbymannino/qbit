use gpui_kit::{application, assets::Assets, component::init};

use crate::{credentials::Credentials, runtime::Tokio, windows::LoginWindow};

mod consts;
mod credentials;
mod runtime;
mod session;
mod theme;
mod torrent_table;
mod windows;

/// Run the desktop application.
pub fn run() {
    application().with_assets(Assets).run(|cx| {
        init(cx);

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

        // Log straight in with saved credentials; otherwise ask for them.
        let (saved, message) = match Credentials::load() {
            Ok(saved) => (saved, None),
            Err(message) => (None, Some(message.into())),
        };
        let auto_login = saved.is_some();

        if let Err(err) = LoginWindow::open(cx, saved, message, auto_login) {
            eprintln!("Failed to open the login window: {err}");
            cx.quit();
        }
    });
}
