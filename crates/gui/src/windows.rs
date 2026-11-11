mod login_window;
mod main_window;
mod settings_window;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Decorations, Render, Result, SharedString, Window, WindowOptions,
    component::{Theme, TitleBar},
    div,
    prelude::*,
};
pub use login_window::LoginWindow;
pub use main_window::{CurrentSession, MainWindow, Session};
pub use settings_window::SettingsWindow;

/// Defines a window that can be opened.
pub trait OpenableWindow: Render + Sized {
    /// Returns the window options for this window.
    fn window_options(cx: &App) -> WindowOptions;

    /// Creates a new window with the view `build` returns, opens it, and brings it to the
    /// foreground.
    ///
    /// # Returns
    ///
    /// A handle to the open window.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform couldn't open the window.
    fn open_window(
        cx: &mut App,
        build: impl FnOnce(&mut Window, &mut Context<Self>) -> Self + 'static,
    ) -> Result<AnyWindowHandle> {
        let wo = Self::window_options(cx);

        let (handle, _) = gpui_kit::open_window(wo, cx, |window, cx| {
            cx.activate(true);

            // Switch between Catppuccin Latte and Mocha when the system appearance changes.
            window
                .observe_window_appearance(|window, cx| {
                    Theme::sync_system_appearance(Some(window), cx);
                })
                .detach();

            cx.new(|cx| build(window, cx))
        })?;
        Ok(handle)
    }
}

/// Returns a title bar to draw at the top of the window when the window manager won't draw one.
///
/// gpui asks for server-side decorations, which macOS, Windows, X11 and most Wayland
/// compositors (KDE, Sway, …) provide. Some, notably GNOME, refuse and leave the window
/// client-decorated, in which case the app has to draw its own title bar and controls.
pub fn client_title_bar(window: &Window, title: impl Into<SharedString>) -> Option<TitleBar> {
    let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
    (cfg!(target_os = "linux") && client_decorated).then(|| TitleBar::new().child(div().text_sm().child(title.into())))
}
