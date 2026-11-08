use gpui_kit::{
    App, Context, Render, SharedString, Size, TitlebarOptions, Window, WindowBounds, WindowOptions,
    base::{StyledExt, v_flex},
    component::ActiveTheme as _,
    div,
    prelude::*,
    px,
};
use qbittorrent::Client;

use crate::{
    consts::{APP_ID, APP_TITLE},
    runtime::Tokio,
    session::login_from_env,
    windows::OpenableWindow,
};

/// Where the qBittorrent login is at.
enum Session {
    LoggingIn,
    LoggedIn(Client),
    Failed(SharedString),
}

pub struct MainWindow {
    session: Session,
}

impl MainWindow {
    /// Log in on the Tokio runtime and update the window with the result.
    fn start_login(cx: &Context<Self>) -> Session {
        let Some(runtime) = Tokio::handle(cx) else {
            return Session::Failed("The async runtime failed to start".into());
        };

        cx.spawn(async move |this, cx| {
            let session = match runtime.spawn(login_from_env()).await {
                Ok(Ok(client)) => Session::LoggedIn(client),
                Ok(Err(message)) => Session::Failed(message.into()),
                Err(err) => Session::Failed(format!("Login task failed: {err}").into()),
            };

            // The window may have been closed while logging in.
            this.update(cx, |this, cx| {
                this.session = session;
                cx.notify();
            })
            .ok();
        })
        .detach();

        Session::LoggingIn
    }
}

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

    fn new(_window: &Window, cx: &mut Context<Self>) -> Self {
        Self {
            session: Self::start_login(cx),
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let status: SharedString = match &self.session {
            Session::LoggingIn => "Logging in…".into(),
            Session::LoggedIn(_) => "Logged in".into(),
            Session::Failed(message) => format!("Login failed: {message}").into(),
        };

        v_flex()
            .p_4()
            .gap_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(div().child("qBit").font_bold().text_3xl())
            .child(div().child(status))
    }
}
