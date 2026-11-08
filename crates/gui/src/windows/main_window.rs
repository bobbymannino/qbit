use gpui_kit::{
    App, AppContext as _, Context, Entity, Render, SharedString, Size, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
    base::{StyledExt, v_flex},
    component::{
        ActiveTheme as _,
        table::{DataTable, TableState},
    },
    div,
    prelude::*,
    px,
};

use crate::{
    consts::{APP_ID, APP_TITLE},
    runtime::Tokio,
    session::login_from_env,
    torrent_table::TorrentTable,
    windows::OpenableWindow,
};

/// Where loading the torrent list is at.
enum Status {
    LoggingIn,
    LoadingTorrents,
    Ready,
    Failed(SharedString),
}

pub struct MainWindow {
    status: Status,
    table: Entity<TableState<TorrentTable>>,
}

impl MainWindow {
    /// Log in, then fetch every torrent into the table, all on the Tokio runtime.
    fn start_loading(cx: &Context<Self>) -> Status {
        let Some(runtime) = Tokio::handle(cx) else {
            return Status::Failed("The async runtime failed to start".into());
        };

        cx.spawn(async move |this, cx| {
            let client = match runtime.spawn(login_from_env()).await {
                Ok(Ok(client)) => client,
                Ok(Err(message)) => {
                    Self::set_status(&this, cx, Status::Failed(message.into()));
                    return;
                }
                Err(err) => {
                    let message = format!("Login task failed: {err}");
                    Self::set_status(&this, cx, Status::Failed(message.into()));
                    return;
                }
            };

            Self::set_status(&this, cx, Status::LoadingTorrents);

            let torrents = match runtime.spawn(async move { client.torrents().await }).await {
                Ok(Ok(torrents)) => torrents,
                Ok(Err(err)) => {
                    let message = format!("Couldn't load torrents: {err}");
                    Self::set_status(&this, cx, Status::Failed(message.into()));
                    return;
                }
                Err(err) => {
                    let message = format!("Torrent task failed: {err}");
                    Self::set_status(&this, cx, Status::Failed(message.into()));
                    return;
                }
            };

            // The window may have been closed while loading.
            this.update(cx, |this, cx| {
                this.table.update(cx, |table, cx| {
                    table.delegate_mut().set_torrents(torrents);
                    table.refresh(cx);
                    cx.notify();
                });
                this.status = Status::Ready;
                cx.notify();
            })
            .ok();
        })
        .detach();

        Status::LoggingIn
    }

    /// Update the status line, ignoring a window that has already closed.
    fn set_status(this: &gpui_kit::WeakEntity<Self>, cx: &mut gpui_kit::AsyncApp, status: Status) {
        this.update(cx, |this, cx| {
            this.status = status;
            cx.notify();
        })
        .ok();
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
            window_bounds: Some(WindowBounds::centered(Size::new(px(900.0), px(600.0)), cx)),
            ..Default::default()
        }
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(TorrentTable::new(), window, cx));

        Self {
            status: Self::start_loading(cx),
            table,
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let status: Option<SharedString> = match &self.status {
            Status::LoggingIn => Some("Logging in…".into()),
            Status::LoadingTorrents => Some("Loading torrents…".into()),
            Status::Ready => None,
            Status::Failed(message) => Some(message.clone()),
        };

        v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(div().child("qBit").font_bold().text_3xl())
            .children(status.map(|status| div().text_color(theme.muted_foreground).child(status)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table).stripe(true).bordered(true)),
            )
    }
}
