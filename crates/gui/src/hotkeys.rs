use gpui_kit::{App, KeyBinding};

pub use actions::{CloseWindow, Quit};

// `actions!` derives `PartialEq` without `Eq`, which the workspace lints deny.
#[allow(clippy::derive_partial_eq_without_eq)]
mod actions {
    gpui_kit::actions!(
        qbit,
        [
            /// Quit the app, closing every window.
            Quit,
            /// Close the focused window; closing the last one quits the app.
            CloseWindow,
        ]
    );
}

/// Register the app-wide hotkeys and what they do.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(close_window);

    cx.bind_keys([
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-q" } else { "ctrl-q" }, Quit, None),
        KeyBinding::new(
            if cfg!(target_os = "macos") { "cmd-w" } else { "ctrl-w" },
            CloseWindow,
            None,
        ),
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
