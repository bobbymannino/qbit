use gpui_kit::{application, assets::Assets, component::init};

use crate::windows::{MainWindow, OpenableWindow};

mod consts;
mod theme;
mod windows;

/// Run the desktop application.
pub fn run() {
    application().with_assets(Assets).run(|cx| {
        init(cx);

        if let Err(err) = theme::init(cx) {
            eprintln!("Failed to load the Catppuccin theme: {err}");
        }

        cx.on_window_closed(|cx, _window| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        MainWindow::open_new_window(cx).expect("Failed to open the main window");
    });
}
