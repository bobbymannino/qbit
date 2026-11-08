use gpui_kit::{App, Render, Size, TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px};

use crate::{
    consts::{APP_ID, APP_TITLE},
    windows::OpenableWindow,
};

pub struct MainWindow;

impl OpenableWindow for MainWindow {
    fn window_options(cx: &App) -> WindowOptions {
        WindowOptions {
            app_id: Some(APP_ID.into()),
            titlebar: Some(TitlebarOptions {
                title: Some(APP_TITLE.into()),
                ..Default::default()
            }),
            window_min_size: Some(Size::new(px(400.0), px(400.0))),
            window_bounds: Some(WindowBounds::centered(Size::new(px(600.0), px(600.0)), cx)),
            ..Default::default()
        }
    }

    fn new(_window: &Window, _cx: &mut App) -> Self {
        Self
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child("Hello")
    }
}
