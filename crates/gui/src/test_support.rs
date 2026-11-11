//! Helpers for UI tests, which render real views in headless windows.

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Point, Render, TestAppContext, Window, WindowBounds,
    WindowOptions, px, size,
};
use qbittorrent::Torrent;
use serde_json::json;

/// Set up the component library, then open a window showing the view `build` makes.
pub fn open_window<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> (AnyWindowHandle, Entity<V>) {
    let (window, view) = cx.update(|cx| {
        gpui_kit::component::init(cx);
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(1024.0), px(640.0)),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        let (window, view) = gpui_kit::open_window(options, cx, build).expect("open a test window");
        (window, view)
    });
    // Focus only moves (and inputs only notice leaving) in the active window.
    cx.update_window(window, |_, window, _| window.activate_window())
        .expect("activate the test window");
    cx.run_until_parked();
    (window, view)
}

/// A Tokio runtime for the requests a view sends.
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("start a Tokio runtime")
}

/// A torrent named `name` in `state`, with `hash` and everything else zeroed.
pub fn torrent(hash: &str, name: &str, state: &str) -> Torrent {
    serde_json::from_value(json!({
        "hash": hash,
        "name": name,
        "size": 0,
        "progress": 0.0,
        "state": state,
        "dlspeed": 0,
        "upspeed": 0,
        "eta": 8_640_000,
        "ratio": 0.0,
        "num_seeds": 0,
        "num_leechs": 0,
        "added_on": 0,
    }))
    .expect("a valid torrent")
}
