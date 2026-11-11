use gpui_kit::{
    AnyWindowHandle, App, Context, Global, Render, Size, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
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
pub struct SettingsWindow {
    /// Forgets this window in [`OpenSettings`] once it's gone.
    _on_release: Subscription,
}

/// The open settings window, if any, so there's only ever one.
#[derive(Default)]
struct OpenSettings(Option<AnyWindowHandle>);

impl Global for OpenSettings {}

impl SettingsWindow {
    /// The settings window, if it's open.
    pub fn handle(cx: &App) -> Option<AnyWindowHandle> {
        cx.try_global::<OpenSettings>().and_then(|open| open.0)
    }

    /// Open the settings window; if it's already open, focus it, or close it if it's
    /// already focused. There's only ever one.
    pub fn toggle(cx: &mut App) {
        let Some(handle) = Self::handle(cx) else {
            match Self::open_window(cx, |_, cx| Self::new(cx)) {
                Ok(handle) => cx.set_global(OpenSettings(Some(handle))),
                Err(err) => eprintln!("Failed to open the settings window: {err}"),
            }
            return;
        };

        // Deferred, because a hotkey pressed in the settings window runs while that window
        // is being updated, and it can't be updated again from inside that.
        let close = cx.active_window() == Some(handle);
        cx.defer(move |cx| {
            let result = handle.update(cx, |_, window, _| {
                if close {
                    window.remove_window();
                } else {
                    window.activate_window();
                }
            });
            if let Err(err) = result {
                eprintln!("Failed to update the settings window: {err}");
            }
        });
    }

    fn new(cx: &Context<Self>) -> Self {
        Self {
            _on_release: cx.on_release(|_, cx| cx.set_global(OpenSettings(None))),
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
