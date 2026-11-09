use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Render, Result, SharedString, Size, Subscription, Task,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
    base::{StyledExt, v_flex},
    component::{
        ActiveTheme as _, Disableable as _,
        button::{Button, ButtonVariants as _},
        input::{Input, InputEvent, InputState},
    },
    div,
    prelude::*,
    px,
};

use crate::{
    consts::{APP_ID, APP_TITLE},
    credentials::Credentials,
    runtime::Tokio,
    session::{login, normalize_host},
    windows::{MainWindow, OpenableWindow},
};

/// Where the login attempt is at.
enum Status {
    Idle,
    LoggingIn,
    Failed(SharedString),
}

/// Asks for the qBittorrent Web UI address and login, then opens the [`MainWindow`].
pub struct LoginWindow {
    host: Entity<InputState>,
    username: Entity<InputState>,
    password: Entity<InputState>,
    status: Status,
    /// What's already in the keychain, so logging in again with it doesn't re-save it
    /// (each keychain write can cost the user a password prompt).
    saved: Option<Credentials>,
    /// The login attempt in progress; replaced (and so cancelled) by a new one.
    _login: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl LoginWindow {
    /// Open the login window.
    ///
    /// - `prefill`: fills in the form (e.g. saved credentials, or the host and username after logging out).
    /// - `prefill_is_saved`: whether `prefill` is exactly what's in the keychain.
    /// - `message`: an error to show above the form.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform couldn't open the window.
    pub fn open(
        cx: &mut App,
        prefill: Option<Credentials>,
        prefill_is_saved: bool,
        message: Option<SharedString>,
    ) -> Result<AnyWindowHandle> {
        Self::open_window(cx, move |window, cx| {
            Self::new(window, cx, prefill, prefill_is_saved, message)
        })
    }

    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        prefill: Option<Credentials>,
        prefill_is_saved: bool,
        message: Option<SharedString>,
    ) -> Self {
        let saved = if prefill_is_saved { prefill.clone() } else { None };
        let (host, username, password) = prefill.map_or_else(
            || (String::new(), String::new(), String::new()),
            |saved| (saved.host, saved.username, saved.password),
        );
        let has_host = !host.is_empty();

        let host = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://localhost:8080")
                .default_value(host)
        });
        let username = cx.new(|cx| InputState::new(window, cx).placeholder("admin").default_value(username));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true).default_value(password));

        // Pressing Enter in any field submits the form.
        let subscriptions = [&host, &username, &password]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
                    }
                })
            })
            .collect();

        // Start typing where the form still needs input.
        let first_empty = if has_host { &password } else { &host };
        first_empty.update(cx, |input, cx| input.focus(window, cx));

        Self {
            host,
            username,
            password,
            status: message.map_or(Status::Idle, Status::Failed),
            saved,
            _login: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Log in with what's in the form.
    fn submit(&mut self, window: &Window, cx: &mut Context<Self>) {
        if matches!(self.status, Status::LoggingIn) {
            return;
        }

        let credentials = Credentials {
            host: normalize_host(&self.host.read(cx).value()),
            username: self.username.read(cx).value().to_string(),
            password: self.password.read(cx).value().to_string(),
        };
        if credentials.host == "http://" {
            self.status = Status::Failed("Enter the address of your qBittorrent Web UI".into());
            cx.notify();
            return;
        }

        self.status = Status::LoggingIn;
        #[expect(
            clippy::used_underscore_binding,
            reason = "the task is only held so it isn't cancelled"
        )]
        {
            let save = self.saved.as_ref() != Some(&credentials);
            self._login = Self::start_login(window, cx, credentials, save);
        }
        cx.notify();
    }

    /// Log in on the Tokio runtime; on success save the credentials (if `save`) and swap
    /// this window for the [`MainWindow`].
    fn start_login(window: &Window, cx: &Context<Self>, credentials: Credentials, save: bool) -> Task<()> {
        let Some(runtime) = Tokio::handle(cx) else {
            return cx.spawn_in(window, async move |this, cx| {
                this.update(cx, |this, cx| {
                    this.status = Status::Failed("The async runtime failed to start".into());
                    cx.notify();
                })
                .ok();
            });
        };

        cx.spawn_in(window, async move |this, cx| {
            let attempt = credentials.clone();
            let result = match runtime.spawn(login(attempt)).await {
                Ok(result) => result,
                Err(err) => Err(format!("Login task failed: {err}")),
            };

            let client = match result {
                Ok(client) => client,
                Err(message) => {
                    this.update(cx, |this, cx| {
                        this.status = Status::Failed(message.into());
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };

            if save {
                // The keychain can block (e.g. on an unlock prompt), so keep it off the UI thread.
                let to_save = credentials.clone();
                let saved = cx.background_executor().spawn(async move { to_save.save() }).await;
                if let Err(err) = saved {
                    eprintln!("{err}");
                }
            }

            cx.update(|window, cx| match MainWindow::open(cx, credentials, client) {
                Ok(_) => window.remove_window(),
                Err(err) => {
                    this.update(cx, |this, cx| {
                        this.status = Status::Failed(format!("Couldn't open the main window: {err}").into());
                        cx.notify();
                    })
                    .ok();
                }
            })
            .ok();
        })
    }
}

impl OpenableWindow for LoginWindow {
    fn window_options(cx: &App) -> WindowOptions {
        WindowOptions {
            app_id: Some(APP_ID.into()),
            titlebar: Some(TitlebarOptions {
                title: Some(format!("{APP_TITLE} — Log in").into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(Size::new(px(400.0), px(460.0)), cx)),
            is_resizable: false,
            ..Default::default()
        }
    }
}

impl Render for LoginWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let logging_in = matches!(self.status, Status::LoggingIn);

        let field = |label: &'static str, input: Input| {
            v_flex()
                .gap_1()
                .child(div().text_sm().font_medium().child(label))
                .child(input)
        };

        v_flex()
            .size_full()
            .p_8()
            .gap_4()
            .justify_center()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                v_flex()
                    .gap_1()
                    .child(div().child(APP_TITLE).font_bold().text_3xl())
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Log in to your qBittorrent Web UI"),
                    ),
            )
            .child(field("Host", Input::new(&self.host).disabled(logging_in)))
            .child(field("Username", Input::new(&self.username).disabled(logging_in)))
            .child(field(
                "Password",
                Input::new(&self.password).mask_toggle().disabled(logging_in),
            ))
            .children(match &self.status {
                Status::Failed(message) => Some(div().text_sm().text_color(theme.danger).child(message.clone())),
                Status::Idle | Status::LoggingIn => None,
            })
            .child(
                Button::new("log-in")
                    .primary()
                    .label(if logging_in { "Logging in…" } else { "Log in" })
                    .loading(logging_in)
                    .disabled(logging_in)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }
}
