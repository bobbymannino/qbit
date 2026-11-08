mod main_window;

use gpui_kit::{AnyWindowHandle, App, AppContext as _, Render, Result, Window, WindowOptions};

pub use main_window::MainWindow;

/// Defines a window that can be opened.
pub trait OpenableWindow: Render {
    /// Returns the window options for this window.
    fn window_options(cx: &App) -> WindowOptions;

    /// Create a new instance of this window.
    fn new(window: &Window, cx: &mut App) -> Self;

    /// Creates a new window, opens it, and brings it to the foreground.
    ///
    /// # Parameters
    ///
    /// - `window`: The window to open.
    ///
    /// # Returns
    ///
    /// A handle to the open window.
    fn open_new_window(cx: &mut App) -> Result<AnyWindowHandle> {
        let wo = Self::window_options(cx);

        let (handle, _) = gpui_kit::open_window(wo, cx, |window, cx| {
            cx.activate(true);
            cx.new(|cx| Self::new(window, cx))
        })?;
        Ok(handle)
    }
}
