use std::time::Duration;

use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Render, Result, SharedString, Size,
    Subscription, Task, TitlebarOptions, WeakEntity, Window, WindowBounds, WindowOptions,
    assets::IconName,
    base::{StyledExt, h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _,
        button::{Button, ButtonVariants as _, Toggle, ToggleGroup},
        input::{Input, InputEvent, InputState},
        table::{DataTable, TableEvent, TableState},
        tooltip::Tooltip,
    },
    div,
    prelude::*,
    px,
};
use qbittorrent::{Client, Error, MainData, Torrent, TransferInfo};

use crate::{
    consts::{APP_ID, APP_TITLE},
    credentials::Credentials,
    details_panel::details_panel,
    format::format_rate,
    runtime::Tokio,
    session::login,
    speed_limits::SpeedLimits,
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
    /// Global speeds for the status bar; `None` until first fetched.
    transfer: Option<TransferInfo>,
    table: Entity<TableState<TorrentTable>>,
    /// Filters the table by torrent name.
    search: Entity<InputState>,
    /// The details panel's speed limit inputs for the selected torrent.
    limits: SpeedLimits,
    _subscriptions: Vec<Subscription>,
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

    fn new(window: &mut Window, cx: &mut Context<Self>, credentials: Credentials, client: Client) -> Self {
        let mut rows = TorrentTable::new();
        if let Some(runtime) = Tokio::handle(cx) {
            rows.set_client(client.clone(), runtime);
        }
        let table = cx.new(|cx| TableState::new(rows, window, cx));

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search torrents"));
        // Redraw when the selection changes, so the details panel follows it.
        let observe_table = cx.observe(&table, |_, _, cx| cx.notify());
        let limits = SpeedLimits::new(window, cx);
        let mut subscriptions = vec![
            observe_table,
            // Clicking the selected torrent again closes its details panel.
            cx.subscribe(&table, |_, table, event: &TableEvent, cx| {
                if let TableEvent::SelectRow(row_ix) = *event {
                    table.update(cx, |table, cx| {
                        if table.delegate_mut().take_reselected(row_ix) {
                            table.clear_selection(cx);
                        }
                    });
                }
            }),
            cx.subscribe_in(&search, window, |this, search, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = search.read(cx).value();
                    this.table.update(cx, |table, cx| {
                        Self::update_rows(table, cx, |rows| rows.set_query(&query));
                    });
                    cx.notify();
                }
            }),
        ];
        subscriptions.extend(limits.subscribe(window, cx, |this: &mut Self, direction, cx| {
            let table = this.table.read(cx);
            if let Some(torrent) = table
                .selected_row()
                .and_then(|row_ix| table.delegate().torrent_at(row_ix))
            {
                this.limits.commit(direction, torrent, table.delegate(), cx);
            }
        }));

        Self {
            _refresh: Self::start_refreshing(cx, credentials.clone(), client),
            credentials,
            status: Status::LoadingTorrents,
            transfer: None,
            table,
            search,
            limits,
            _subscriptions: subscriptions,
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
            // Merged state from `sync/maindata`; each refresh fetches only what changed.
            let mut data = MainData::new();
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
                            // A new session starts its own sync, so begin from scratch.
                            data = MainData::new();
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

                // The state moves into the Tokio task and comes back with the update.
                let mut pending = std::mem::take(&mut data);
                let fetched = runtime
                    .spawn(async move {
                        let result = pending.update(&current).await;
                        (pending, result)
                    })
                    .await
                    .map(|(updated, result)| {
                        data = updated;
                        result
                    });
                let still_open = match fetched {
                    Ok(Ok(())) => {
                        relogged_in = false;
                        Self::show_torrents(&this, cx, data.torrents(), data.transfer())
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
    fn show_torrents(
        this: &WeakEntity<Self>,
        cx: &mut AsyncApp,
        torrents: Vec<Torrent>,
        transfer: Option<TransferInfo>,
    ) -> bool {
        this.update(cx, |this, cx| {
            this.table.update(cx, |table, cx| {
                Self::update_rows(table, cx, |rows| rows.set_torrents(torrents));
            });
            if transfer.is_some() {
                this.transfer = transfer;
            }
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

        // Only redraw. `TableState::refresh` rebuilds the columns from the delegate,
        // which would throw away the user's column widths, order and sort arrow.
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

    /// The details panel for the selected torrent, if any.
    fn render_details(&mut self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let table = self.table.read(cx);
        let torrent = table.delegate().torrent_at(table.selected_row()?)?.clone();
        self.limits.sync(&torrent, window, cx);
        Some(details_panel(&torrent, &self.table, &self.limits, cx).into_any_element())
    }

    /// Torrent count on the left; total download and upload speed on the right.
    fn render_status_bar(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let count = self.table.read(cx).delegate().count(Filter::All);
        let torrents = if count == 1 {
            "1 torrent".to_owned()
        } else {
            format!("{count} torrents")
        };

        let (down, up) = self
            .transfer
            .as_ref()
            .map_or((0, 0), |transfer| (transfer.dl_info_speed, transfer.up_info_speed));
        let speed = |icon: IconName, rate: u64, label: &'static str| {
            h_flex()
                .gap_1()
                .child(Icon::new(icon).xsmall())
                .child(format_rate(rate))
                .id(label)
                .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        };

        h_flex()
            .justify_between()
            .pt_2()
            .border_t_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(torrents)
            .child(
                h_flex()
                    .gap_4()
                    .child(speed(IconName::ArrowDown, down, "Total download speed"))
                    .child(speed(IconName::ArrowUp, up, "Total upload speed")),
            )
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
                match LoginWindow::open(cx, Some(prefill.clone()), false, None) {
                    Ok(_) => window.remove_window(),
                    Err(err) => eprintln!("Couldn't open the login window: {err}"),
                }
            })
    }

    /// Hand the logged-in client to the table so its play/pause buttons work.
    fn give_table_client(this: &WeakEntity<Self>, cx: &mut AsyncApp, client: Client, runtime: tokio::runtime::Handle) {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let details = self.render_details(window, cx);
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
                h_flex().justify_between().gap_2().child(self.render_filters(cx)).child(
                    h_flex()
                        .gap_2()
                        .child(
                            Input::new(&self.search)
                                .prefix(Icon::new(IconName::Search).small())
                                .cleanable(true)
                                .small()
                                .w(px(220.0)),
                        )
                        .child(self.render_bulk_actions(cx)),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(DataTable::new(&self.table).stripe(true).bordered(true)),
                    )
                    .children(details),
            )
            .child(self.render_status_bar(cx))
    }
}
