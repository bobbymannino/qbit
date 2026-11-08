use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Render, Result,
    SharedString, Size, Task, TitlebarOptions, WeakEntity, Window, WindowBounds, WindowOptions,
    assets::IconName,
    base::{StyledExt, h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _, Toggle, ToggleGroup},
        table::{DataTable, TableState},
    },
    div,
    prelude::*,
    px,
};
use qbittorrent::{Client, Error, Torrent};

use crate::{
    consts::{APP_ID, APP_TITLE},
    credentials::Credentials,
    runtime::Tokio,
    session::login,
    torrent_table::{Filter, TorrentTable},
    windows::{LoginWindow, OpenableWindow},
};

/// How often the torrent list is refreshed.
const REFRESH_INTERVAL: Duration = Duration::from_millis(1_100);

/// Where loading the torrent list is at.
enum Status {
    LoadingTorrents,
    Ready,
    /// Something went wrong; the message is shown above the table.
    Failed(SharedString),
}

pub struct MainWindow {
    /// Used to log in again when the session expires, and to prefill the login form
    /// after logging out.
    credentials: Credentials,
    status: Status,
    table: Entity<TableState<TorrentTable>>,
    /// Logs in and refreshes the torrents; dropped (and so stopped) with the window.
    _refresh: Task<()>,
}

impl MainWindow {
    /// Open the main window with an already logged-in `client`.
    ///
    /// # Errors
    ///
    /// Returns an error if the platform couldn't open the window.
    pub fn open(cx: &mut App, credentials: Credentials, client: Client) -> Result<AnyWindowHandle> {
        Self::open_window(cx, move |window, cx| Self::new(window, cx, credentials, client))
    }

    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        credentials: Credentials,
        client: Client,
    ) -> Self {
        let mut rows = TorrentTable::new();
        if let Some(runtime) = Tokio::handle(cx) {
            rows.set_client(client.clone(), runtime);
        }
        let table = cx.new(|cx| TableState::new(rows, window, cx));

        Self {
            _refresh: Self::start_refreshing(cx, credentials.clone(), client),
            credentials,
            status: Status::LoadingTorrents,
            table,
        }
    }

    /// Refresh the torrents every [`REFRESH_INTERVAL`] until the window closes, logging in
    /// again with `credentials` if the session expires.
    ///
    /// Network calls run on the Tokio runtime; the loop itself runs on gpui's executor.
    fn start_refreshing(cx: &Context<Self>, credentials: Credentials, client: Client) -> Task<()> {
        let Some(runtime) = Tokio::handle(cx) else {
            return cx.spawn(async move |this, cx| {
                let status = Status::Failed("The async runtime failed to start".into());
                Self::set_status(&this, cx, status);
            });
        };

        cx.spawn(async move |this, cx| {
            let mut client = Some(client);
            // Re-logging in is only tried once per expired session, so a bad
            // password can't hammer the server into banning this IP.
            let mut relogged_in = false;

            loop {
                let current = if let Some(current) = &client {
                    current.clone()
                } else {
                    let logged_in = match runtime.spawn(login(credentials.clone())).await {
                        Ok(result) => result,
                        Err(err) => Err(format!("Login task failed: {err}")),
                    };
                    match logged_in {
                        Ok(logged_in) => {
                            client = Some(logged_in.clone());
                            Self::give_table_client(&this, cx, logged_in.clone(), runtime.clone());
                            logged_in
                        }
                        Err(message) => {
                            Self::set_status(&this, cx, Status::Failed(message.into()));
                            return;
                        }
                    }
                };

                if !Self::has_torrents(&this, cx) {
                    Self::set_status(&this, cx, Status::LoadingTorrents);
                }

                let fetched = runtime.spawn(async move { current.torrents().await }).await;
                let still_open = match fetched {
                    Ok(Ok(torrents)) => {
                        relogged_in = false;
                        Self::show_torrents(&this, cx, torrents)
                    }
                    Ok(Err(Error::NotLoggedIn)) if !relogged_in => {
                        // The session expired; log in again straight away.
                        client = None;
                        relogged_in = true;
                        continue;
                    }
                    // Keep the last list on screen and try again next tick.
                    Ok(Err(err)) => {
                        let message = format!("Couldn't refresh torrents: {err}");
                        Self::set_status(&this, cx, Status::Failed(message.into()))
                    }
                    Err(err) => {
                        let message = format!("Torrent task failed: {err}");
                        Self::set_status(&this, cx, Status::Failed(message.into()))
                    }
                };
                if !still_open {
                    return;
                }

                cx.background_executor().timer(REFRESH_INTERVAL).await;
            }
        })
    }

    /// Put fresh torrents in the table, keeping the selected torrent selected.
    ///
    /// Returns `false` if the window has closed.
    fn show_torrents(this: &WeakEntity<Self>, cx: &mut AsyncApp, torrents: Vec<Torrent>) -> bool {
        this.update(cx, |this, cx| {
            this.table.update(cx, |table, cx| {
                Self::update_rows(table, cx, |rows| rows.set_torrents(torrents));
            });
            this.status = Status::Ready;
            cx.notify();
        })
        .is_ok()
    }

    /// Change the table's rows with `change`, keeping the selected torrent selected.
    ///
    /// Rows move when sorted by a changing value like speed, or when the filter
    /// changes; if the selected torrent is no longer shown, the selection is cleared.
    fn update_rows(
        table: &mut TableState<TorrentTable>,
        cx: &mut Context<TableState<TorrentTable>>,
        change: impl FnOnce(&mut TorrentTable),
    ) {
        let selected = table
            .selected_row()
            .and_then(|row_ix| table.delegate().hash_at(row_ix))
            .map(ToOwned::to_owned);

        change(table.delegate_mut());

        if let Some(hash) = selected {
            match table.delegate().row_of(&hash) {
                Some(row_ix) if table.selected_row() == Some(row_ix) => {}
                Some(row_ix) => table.set_selected_row(row_ix, cx),
                None => table.clear_selection(cx),
            }
        }

        table.refresh(cx);
        cx.notify();
    }

    /// "Start all" and "Stop all" buttons, applying to every torrent.
    fn render_bulk_actions(&self, cx: &App) -> impl IntoElement {
        let enabled = self.table.read(cx).delegate().can_send();

        let button = |id: &'static str, label: &'static str, icon: IconName, start: bool| {
            let table = self.table.clone();
            Button::new(id)
                .label(label)
                .icon(icon)
                .small()
                .outline()
                .disabled(!enabled)
                .on_click(move |_, _, cx| table.read(cx).delegate().send_all(start))
        };

        h_flex()
            .gap_2()
            .child(button("start-all", "Start all", IconName::Play, true))
            .child(button("stop-all", "Stop all", IconName::Pause, false))
    }

    /// Toggles for showing all, downloading, seeding or stopped torrents, with counts.
    fn render_filters(&self, cx: &App) -> impl IntoElement {
        let rows = self.table.read(cx).delegate();
        let current = rows.filter();

        let toggles = Filter::ALL.map(|filter| {
            Toggle::new(filter.label())
                .label(format!("{} ({})", filter.label(), rows.count(filter)))
                .checked(filter == current)
        });

        let table = self.table.clone();
        ToggleGroup::new("filters")
            .segmented()
            .small()
            .children(toggles)
            .on_click(move |checked, _, cx| {
                // The group allows several toggles on; treat it as pick-one by taking
                // the newly checked toggle. Unchecking the current one keeps it.
                let picked = Filter::ALL
                    .iter()
                    .zip(checked)
                    .find(|(filter, on)| **on && **filter != current)
                    .map(|(filter, _)| *filter);

                if let Some(filter) = picked {
                    table.update(cx, |table, cx| {
                        Self::update_rows(table, cx, |rows| rows.set_filter(filter));
                    });
                }
            })
    }

    /// Forget the saved login and go back to the login window, keeping the host and
    /// username filled in.
    fn render_log_out(&self) -> impl IntoElement {
        let prefill = Credentials {
            password: String::new(),
            ..self.credentials.clone()
        };

        Button::new("log-out")
            .label("Log out")
            .icon(IconName::LogOut)
            .small()
            .ghost()
            .on_click(move |_, window, cx| {
                if let Err(err) = Credentials::clear() {
                    eprintln!("{err}");
                }
                match LoginWindow::open(cx, Some(prefill.clone()), None, false) {
                    Ok(_) => window.remove_window(),
                    Err(err) => eprintln!("Couldn't open the login window: {err}"),
                }
            })
    }

    /// Hand the logged-in client to the table so its play/pause buttons work.
    fn give_table_client(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        client: Client,
        runtime: tokio::runtime::Handle,
    ) {
        this.update(cx, |this, cx| {
            this.table.update(cx, |table, cx| {
                table.delegate_mut().set_client(client, runtime);
                cx.notify();
            });
        })
        .ok();
    }

    /// Whether the table already has data (so later refreshes don't flash "Loading").
    fn has_torrents(this: &WeakEntity<Self>, cx: &AsyncApp) -> bool {
        this.read_with(cx, |this, _| matches!(this.status, Status::Ready | Status::Failed(_)))
            .unwrap_or(false)
    }

    /// Update the status line.
    ///
    /// Returns `false` if the window has closed.
    fn set_status(this: &WeakEntity<Self>, cx: &mut AsyncApp, status: Status) -> bool {
        this.update(cx, |this, cx| {
            this.status = status;
            cx.notify();
        })
        .is_ok()
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

}

impl Render for MainWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let status: Option<SharedString> = match &self.status {
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
            .child(
                h_flex()
                    .justify_between()
                    .child(div().child(APP_TITLE).font_bold().text_3xl())
                    .child(self.render_log_out()),
            )
            .children(status.map(|status| div().text_color(theme.muted_foreground).child(status)))
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(self.render_filters(cx))
                    .child(self.render_bulk_actions(cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table).stripe(true).bordered(true)),
            )
    }
}
