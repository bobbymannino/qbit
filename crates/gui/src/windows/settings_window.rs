use gpui_kit::{
    AnyWindowHandle, App, Context, Global, Render, SharedString, Size, Subscription, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
    assets::IconName,
    base::{StyledExt as _, h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
    },
    div,
    prelude::*,
    px,
};

use crate::{
    consts::{APP_ID, APP_TITLE, APP_VERSION, RELEASES_URL, REPO_URL},
    update::{Status, Updater},
    windows::{CurrentSession, OpenableWindow, Session, client_title_bar},
};

/// The app's settings.
pub struct SettingsWindow {
    _subscriptions: [Subscription; 3],
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
            _subscriptions: [
                // Forget this window in `OpenSettings` once it's gone.
                cx.on_release(|_, cx| cx.set_global(OpenSettings(None))),
                // Redraw when logging in or out, so the account section follows.
                cx.observe_global::<CurrentSession>(|_, cx| cx.notify()),
                // Redraw as an update check or install goes on.
                cx.observe_global::<Updater>(|_, cx| cx.notify()),
            ],
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
            window_bounds: Some(WindowBounds::centered(Size::new(px(480.0), px(420.0)), cx)),
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

impl SettingsWindow {
    /// Checking for a newer release, and installing it.
    fn render_updates(cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let status = Updater::status(cx);

        let message: SharedString = match &status {
            Status::Idle => "Check GitHub for a newer version.".into(),
            Status::Checking => "Checking for updates…".into(),
            Status::UpToDate => format!("You're on the latest version, v{APP_VERSION}.").into(),
            Status::Available(release) => format!("v{} is available.", release.version).into(),
            Status::Installing(version) => format!("Downloading and installing v{version}…").into(),
            Status::Failed(message) => message.clone(),
        };

        let check = |label: &'static str| {
            Button::new("check-for-updates")
                .label(label)
                .small()
                .on_click(|_, _, cx| Updater::check(cx))
        };
        let button = match &status {
            Status::Idle => check("Check for updates"),
            Status::UpToDate | Status::Failed(_) => check("Check again"),
            Status::Checking => check("Checking…").loading(true).disabled(true),
            Status::Available(_) => Button::new("install-update")
                .primary()
                .label("Install and restart")
                .small()
                .on_click(|_, _, cx| Updater::install(cx)),
            Status::Installing(_) => Button::new("install-update")
                .primary()
                .label("Installing…")
                .small()
                .loading(true)
                .disabled(true),
        };

        let whats_new = match &status {
            Status::Available(release) => {
                let url = release.url.clone();
                Some(
                    Button::new("whats-new")
                        .link()
                        .label("What's new")
                        .small()
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            }
            _ => None,
        };

        h_flex()
            .justify_between()
            .gap_4()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().font_medium().child("Updates"))
                    .child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .text_color(if matches!(status, Status::Failed(_)) {
                                theme.danger
                            } else {
                                theme.muted_foreground
                            })
                            .child(message)
                            .children(whats_new),
                    ),
            )
            .child(button)
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
                    .child(Self::render_updates(cx))
                    .children(Self::render_account(cx)),
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
