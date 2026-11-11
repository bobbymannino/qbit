use std::time::Duration;

use gpui_kit::{
    Anchor, AnyElement, AnyWindowHandle, App, AppContext as _, AsyncApp, Context, Entity, Focusable as _, Global,
    Render, Result, SharedString, Size, Subscription, Task, TitlebarOptions, WeakEntity, Window, WindowBounds,
    WindowOptions,
    assets::IconName,
    base::{StyledExt, h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
        button::{Button, ButtonVariants as _, Toggle, ToggleGroup},
        input::{Input, InputEvent, InputState},
        popover::Popover,
        table::{DataTable, TableEvent, TableState},
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
    hotkeys::ToggleSettings,
    prefs::Prefs,
    runtime::Tokio,
    session::login,
    speed_limits::{Direction, Limits, Sent, SpeedLimits},
    torrent_table::{Filter, TorrentTable},
    update::Updater,
    windows::{LoginWindow, OpenableWindow, SettingsWindow, client_title_bar},
};

/// How often the torrent list is refreshed.
const REFRESH_INTERVAL: Duration = Duration::from_millis(1_100);

/// What the global speed limit inputs are filled in for, as opposed to a torrent's hash.
const GLOBAL_LIMITS_KEY: &str = "global";

/// Width of the global speed limit popovers.
const LIMIT_POPOVER_WIDTH: f32 = 220.0;

/// Where loading the torrent list is at.
enum Status {
    LoadingTorrents,
    Ready,
    /// Something went wrong; the message is shown above the table.
    Failed(SharedString),
}

/// Who's logged in, and the main window showing their torrents, while there is one.
pub struct Session {
    pub credentials: Credentials,
    window: AnyWindowHandle,
}

/// The logged-in session, if any.
#[derive(Default)]
pub struct CurrentSession(Option<Session>);

impl Global for CurrentSession {}

impl Session {
    /// The logged-in session, if there is one.
    pub fn current(cx: &App) -> Option<&Self> {
        cx.try_global::<CurrentSession>().and_then(|current| current.0.as_ref())
    }

    /// Forget the saved login, close the main window and go back to the login window,
    /// keeping the host and username filled in.
    pub fn log_out(cx: &mut App) {
        let Some(session) = Self::current(cx) else { return };
        let window = session.window;
        let prefill = Credentials {
            password: String::new(),
            ..session.credentials.clone()
        };

        if let Err(err) = Credentials::clear() {
            eprintln!("{err}");
        }
        // Open the login window before closing the main one, so the app doesn't quit
        // for having no windows left.
        if let Err(err) = LoginWindow::open(cx, Some(prefill), false, None) {
            eprintln!("Couldn't open the login window: {err}");
            return;
        }
        // Deferred, because this can run while another window (e.g. settings) is being
        // updated, and the main window can't be updated from inside that.
        cx.defer(move |cx| {
            if let Err(err) = window.update(cx, |_, window, _| window.remove_window()) {
                eprintln!("Failed to close the main window: {err}");
            }
        });
    }
}

pub struct MainWindow {
    status: Status,
    /// Global speeds for the status bar; `None` until first fetched.
    transfer: Option<TransferInfo>,
    table: Entity<TableState<TorrentTable>>,
    /// Filters the table by torrent name.
    search: Entity<InputState>,
    /// The details panel's speed limit inputs for the selected torrent.
    limits: SpeedLimits,
    /// The status bar's speed limit inputs for every torrent.
    global_limits: SpeedLimits,
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
        let prefs = Prefs::load();
        rows.set_column_order(prefs.column_order.iter().map(String::as_str));
        rows.set_column_widths(prefs.column_widths.iter().map(|(key, width)| (key.as_str(), *width)));
        if let Some(runtime) = Tokio::handle(cx) {
            rows.set_client(client.clone(), runtime);
        }
        let table = cx.new(|cx| TableState::new(rows, window, cx));

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search torrents"));
        // Redraw when the selection changes, so the details panel follows it.
        let observe_table = cx.observe(&table, |_, _, cx| cx.notify());
        let limits = SpeedLimits::new(window, cx);
        let global_limits = SpeedLimits::new(window, cx);
        let mut subscriptions = vec![
            observe_table,
            cx.subscribe(&table, |_, table, event: &TableEvent, cx| match event {
                // Clicking the selected torrent again closes its details panel.
                TableEvent::SelectRow(row_ix) => table.update(cx, |table, cx| {
                    if table.delegate_mut().take_reselected(*row_ix) {
                        table.clear_selection(cx);
                    }
                }),
                // Sent once a resize finishes, so this doesn't write on every mouse move.
                TableEvent::ColumnWidthsChanged(widths) => {
                    let widths = table.update(cx, |table, _| table.delegate_mut().record_column_widths(widths));
                    Prefs::update(|prefs| prefs.column_widths = widths);
                }
                _ => {}
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
            let Some(torrent) = table
                .selected_row()
                .and_then(|row_ix| table.delegate().torrent_at(row_ix))
            else {
                return;
            };
            let rows = table.delegate();
            let download = matches!(direction, Direction::Download);
            let send_limit = |limit| rows.send_limit(&torrent.hash, download, limit);
            if let Some(sent) = this.limits.commit(direction, Limits::from(torrent), send_limit, cx) {
                Self::finish_when_answered(sent, |this| &mut this.limits, cx);
            }
        }));
        subscriptions.extend(global_limits.subscribe(window, cx, |this: &mut Self, direction, cx| {
            let Some(transfer) = &this.transfer else { return };
            let rows = this.table.read(cx).delegate();
            let download = matches!(direction, Direction::Download);
            let send_limit = |limit| rows.send_global_limit(download, limit);
            if let Some(sent) = this
                .global_limits
                .commit(direction, Limits::from(transfer), send_limit, cx)
            {
                Self::finish_when_answered(sent, |this| &mut this.global_limits, cx);
            }
        }));

        // Offer to install an update the startup check finds, whether it finished before
        // this window opened or finishes later.
        subscriptions.push(cx.observe_global_in::<Updater>(window, |_, window, cx| {
            Self::prompt_for_update(window, cx);
        }));
        window.defer(cx, Self::prompt_for_update);

        // Track the session while this window is open, for the settings window's log out button.
        let handle = window.window_handle();
        subscriptions.push(cx.on_release(move |_, cx| {
            if Session::current(cx).is_some_and(|session| session.window == handle) {
                cx.set_global(CurrentSession(None));
            }
        }));
        cx.set_global(CurrentSession(Some(Session {
            credentials: credentials.clone(),
            window: handle,
        })));

        Self {
            _refresh: Self::start_refreshing(cx, credentials, client),
            status: Status::LoadingTorrents,
            transfer: None,
            table,
            search,
            limits,
            global_limits,
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

    /// Keep `limits`' input disabled until qBittorrent has answered `sent`, then enable it again.
    fn finish_when_answered(sent: Sent, limits: fn(&mut Self) -> &mut SpeedLimits, cx: &mut Context<Self>) {
        cx.notify();
        cx.spawn(async move |this, cx| {
            // A failed request is logged by the task.
            let succeeded = sent.request.await.unwrap_or(false);
            this.update(cx, |this, cx| {
                limits(this).finish(sent.direction, sent.id, succeeded);
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        self.limits.sync(&torrent.hash, Limits::from(&torrent), window, cx);
        Some(details_panel(&torrent, &self.table, &self.limits, cx).into_any_element())
    }

    /// Torrent count on the left; total download and upload speed on the right, each
    /// opening a popover to limit it.
    fn render_status_bar(&mut self, window: &mut Window, cx: &mut App) -> AnyElement {
        if let Some(transfer) = &self.transfer {
            self.global_limits
                .sync(GLOBAL_LIMITS_KEY, Limits::from(transfer), window, cx);
        }

        let theme = cx.theme();
        let count = self.table.read(cx).delegate().count(Filter::All);
        let torrents = if count == 1 {
            "1 torrent".to_owned()
        } else {
            format!("{count} torrents")
        };

        let transfer = self.transfer.as_ref();
        let (down, up) = transfer.map_or((0, 0), |transfer| (transfer.dl_info_speed, transfer.up_info_speed));
        let limits = transfer.map(Limits::from).unwrap_or_default();
        let can_send = transfer.is_some() && self.table.read(cx).delegate().can_send();

        let speed = |id: &'static str, icon: IconName, rate: u64, limit: i64, direction: Direction| {
            let (input, tooltip, title) = match direction {
                Direction::Download => (
                    &self.global_limits.download,
                    "Total download speed",
                    "Max download speed",
                ),
                Direction::Upload => (&self.global_limits.upload, "Total upload speed", "Max upload speed"),
            };
            let label = match u64::try_from(limit) {
                Ok(limit) if limit > 0 => format!("{} / {}", format_rate(rate), format_rate(limit)),
                _ => format_rate(rate),
            };
            let enabled = can_send && !self.global_limits.is_sending(direction);
            let input = input.clone();

            Popover::new(id)
                .anchor(Anchor::BottomRight)
                // Focus the input on opening, so a limit can be typed straight away.
                .track_focus(&input.read(cx).focus_handle(cx))
                .trigger(
                    Button::new(id)
                        .icon(Icon::new(icon).xsmall())
                        .label(label)
                        .ghost()
                        .xsmall()
                        .tooltip(tooltip),
                )
                .content(move |_, _, cx| {
                    let theme = cx.theme();
                    v_flex()
                        .gap_0p5()
                        .w(px(LIMIT_POPOVER_WIDTH))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(title))
                        .child(
                            Input::new(&input)
                                .suffix(div().text_xs().text_color(theme.muted_foreground).child("KiB/s"))
                                .small()
                                .disabled(!enabled),
                        )
                })
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
                    .gap_1()
                    .child(speed(
                        "global-download",
                        IconName::ArrowDown,
                        down,
                        limits.download,
                        Direction::Download,
                    ))
                    .child(speed(
                        "global-upload",
                        IconName::ArrowUp,
                        up,
                        limits.upload,
                        Direction::Upload,
                    )),
            )
            .into_any_element()
    }

    /// Ask whether to install the update the startup check found, if the user hasn't
    /// been asked yet.
    fn prompt_for_update(window: &mut Window, cx: &mut App) {
        let Some(release) = Updater::take_startup_prompt(cx) else { return };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let url = release.url.clone();
            alert
                .confirm()
                .title("Update available")
                .description(format!(
                    "qBit v{} is available. Download and install it now? qBit will restart.",
                    release.version
                ))
                .child(
                    Button::new("whats-new")
                        .link()
                        .label("What's new")
                        .small()
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
                .ok_text("Install and restart")
                .cancel_text("Not now")
                .on_ok(|_, _, cx| {
                    Updater::install(cx);
                    true
                })
        });
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
            window_bounds: Some(WindowBounds::centered(Size::new(px(1024.0), px(640.0)), cx)),
            ..Default::default()
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let details = self.render_details(window, cx);
        let status_bar = self.render_status_bar(window, cx);
        let theme = cx.theme();

        let status: Option<SharedString> = match &self.status {
            Status::LoadingTorrents => Some("Loading torrents…".into()),
            Status::Ready => None,
            Status::Failed(message) => Some(message.clone()),
        };

        let content = v_flex()
            .flex_1()
            .min_h_0()
            .p_4()
            .gap_4()
            .child(
                h_flex()
                    .justify_between()
                    .child(div().child(APP_TITLE).font_bold().text_3xl())
                    .child(
                        Button::new("settings")
                            .icon(IconName::Settings)
                            .ghost()
                            .tooltip_with_action("Settings", &ToggleSettings, None)
                            .on_click(|_, _, cx| SettingsWindow::toggle(cx)),
                    ),
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
            .child(status_bar);

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .children(client_title_bar(window, APP_TITLE))
            .child(content)
    }
}
