use gpui_kit::{application, assets::Assets, component::init};

use crate::windows::{MainWindow, OpenableWindow};

mod consts;
mod windows;

/// Run the desktop application.
pub fn run() {
    application().with_assets(Assets).run(|cx| {
        init(cx);

        cx.on_window_closed(|cx, _window| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        MainWindow::open_new_window(cx).expect("Failed to open the main window");
    });
}
