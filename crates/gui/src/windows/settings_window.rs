use gpui_kit::{
    AnyWindowHandle, App, Context, Global, Render, Size, TitlebarOptions, Window, WindowBounds, WindowOptions,
    base::{StyledExt as _, v_flex},
    component::ActiveTheme as _,
    div,
    prelude::*,
    px,
};

use crate::{
    consts::{APP_ID, APP_TITLE},
    windows::OpenableWindow,
};

/// The app's settings.
pub struct SettingsWindow;

/// The open settings window, if any, so there's only ever one.
#[derive(Default)]
struct OpenSettings(Option<AnyWindowHandle>);

impl Global for OpenSettings {}

impl SettingsWindow {
    /// Close the settings window if it's open, otherwise open it.
    pub fn toggle(cx: &mut App) {
        // A handle to a window the user already closed fails to update, so it counts as closed.
        if let Some(handle) = cx.default_global::<OpenSettings>().0.take()
            && handle.update(cx, |_, _, _| ()).is_ok()
        {
            cx.defer(move |cx| {
                if let Err(err) = handle.update(cx, |_, window, _| window.remove_window()) {
                    eprintln!("Failed to close the settings window: {err}");
                }
            });
            return;
        }

        match Self::open_window(cx, |_, _| Self) {
            Ok(handle) => cx.set_global(OpenSettings(Some(handle))),
            Err(err) => eprintln!("Failed to open the settings window: {err}"),
        }
    }
}

impl OpenableWindow for SettingsWindow {
    fn window_options(cx: &App) -> WindowOptions {
        WindowOptions {
            app_id: Some(APP_ID.into()),
            titlebar: Some(TitlebarOptions {
                title: Some(format!("{APP_TITLE} — Settings").into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(Size::new(px(480.0), px(360.0)), cx)),
            is_resizable: false,
            ..Default::default()
        }
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .size_full()
            .p_8()
            .gap_1()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(div().child("Settings").font_bold().text_2xl())
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("Nothing to configure yet."),
            )
    }
}
