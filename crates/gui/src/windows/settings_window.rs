use gpui_kit::{
    AnyWindowHandle, App, Context, Global, Render, Size, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
    assets::IconName,
    base::{StyledExt as _, h_flex, v_flex},
    component::{
        ActiveTheme as _, Sizable as _,
        button::{Button, ButtonVariants as _},
    },
    div,
    prelude::*,
    px,
};

use crate::{
    consts::{APP_ID, APP_TITLE, APP_VERSION, RELEASES_URL, REPO_URL},
    windows::{CurrentSession, OpenableWindow, Session, client_title_bar},
};

/// The app's settings.
pub struct SettingsWindow {
    /// Forgets this window in [`OpenSettings`] once it's gone.
    _on_release: Subscription,
    /// Redraws when logging in or out, so the account section follows.
    _on_session: Subscription,
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

    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            _on_release: cx.on_release(|_, cx| cx.set_global(OpenSettings(None))),
            _on_session: cx.observe_global::<CurrentSession>(|_, cx| cx.notify()),
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

impl SettingsWindow {
    /// Who's logged in, with a button to log out; `None` while nobody is.
    fn render_account(cx: &App) -> Option<impl IntoElement> {
        let session = Session::current(cx)?;
        let theme = cx.theme();
        let who = format!("{} on {}", session.credentials.username, session.credentials.host);

        Some(
            h_flex()
                .justify_between()
                .gap_4()
                .child(
                    v_flex()
                        .min_w_0()
                        .gap_1()
                        .child(div().text_sm().font_medium().child("Account"))
                        .child(div().text_sm().text_color(theme.muted_foreground).truncate().child(who)),
                )
                .child(
                    Button::new("log-out")
                        .danger()
                        .label("Log out")
                        .icon(IconName::LogOut)
                        .small()
                        .on_click(|_, _, cx| Session::log_out(cx)),
                ),
        )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let link = |id: &'static str, label: &'static str, url: &'static str| {
            Button::new(id)
                .link()
                .label(label)
                .on_click(move |_, _, cx| cx.open_url(url))
        };

        let content = v_flex()
            .flex_1()
            .min_h_0()
            .p_8()
            .justify_between()
            .child(
                v_flex()
                    .gap_6()
                    .child(div().child("Settings").font_bold().text_2xl())
                    .child(Self::render_account(cx).map_or_else(
                        || {
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Nothing to configure yet.")
                                .into_any_element()
                        },
                        IntoElement::into_any_element,
                    )),
            )
            .child(
                h_flex()
                    .justify_between()
                    .text_sm()
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(format!("{APP_TITLE} v{APP_VERSION}")),
                    )
                    .child(h_flex().gap_4().child(link("repo", "GitHub", REPO_URL)).child(link(
                        "changelog",
                        "Changelog",
                        RELEASES_URL,
                    ))),
            );

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .children(client_title_bar(window, format!("{APP_TITLE} — Settings")))
            .child(content)
    }
}
