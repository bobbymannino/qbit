use std::{cell::Cell, cmp::Ordering, rc::Rc, sync::LazyLock};

use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, Div, IntoElement, MouseButton, SharedString, Stateful, Window,
    assets::IconName,
    base::h_flex,
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants as _},
        checkbox::Checkbox,
        menu::{PopupMenu, PopupMenuItem},
        progress::Progress,
        table::{Column, ColumnSort, TableDelegate, TableState},
        tooltip::Tooltip,
    },
    div,
    prelude::*,
    px,
};
use qbittorrent::{Client, Torrent};
use regex::Regex;
use tokio::runtime::Handle;

use crate::{
    format::{format_bytes, format_eta, format_speed},
    prefs::Prefs,
};

/// Which torrents the table shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Downloading,
    Seeding,
    Stopped,
}

impl Filter {
    pub const ALL: [Self; 4] = [Self::All, Self::Downloading, Self::Seeding, Self::Stopped];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Downloading => "Downloading",
            Self::Seeding => "Seeding",
            Self::Stopped => "Stopped",
        }
    }

    const fn matches(self, torrent: &Torrent) -> bool {
        match self {
            Self::All => true,
            Self::Downloading => torrent.state.is_downloading(),
            Self::Seeding => torrent.state.is_seeding(),
            Self::Stopped => torrent.state.is_stopped(),
        }
    }
}

/// Width of the percentage shown next to a progress bar.
const PERCENT_WIDTH: f32 = 44.0;

/// The columns shown in the torrent table, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TorrentColumn {
    /// Play/pause button.
    Toggle,
    Name,
    Size,
    Progress,
    Status,
    Download,
    /// Time left, for torrents that are downloading.
    Eta,
    Upload,
    Ratio,
}

impl TorrentColumn {
    const ALL: [Self; 9] = [
        Self::Toggle,
        Self::Name,
        Self::Size,
        Self::Progress,
        Self::Status,
        Self::Download,
        Self::Eta,
        Self::Upload,
        Self::Ratio,
    ];

    fn column(self) -> Column {
        match self {
            Self::Toggle => Column::new("toggle", "")
                .width(px(40.0))
                .text_center()
                .resizable(false)
                .movable(false),
            Self::Name => Column::new("name", "Name").width(px(280.0)).sortable(),
            Self::Size => Column::new("size", "Size").width(px(90.0)).text_right().sortable(),
            Self::Progress => Column::new("progress", "Progress").width(px(160.0)).sortable(),
            Self::Status => Column::new("status", "Status").width(px(140.0)).sortable(),
            Self::Download => Column::new("dlspeed", "Down").width(px(100.0)).text_right().sortable(),
            Self::Eta => Column::new("eta", "ETA").width(px(80.0)).text_right().sortable(),
            Self::Upload => Column::new("upspeed", "Up").width(px(100.0)).text_right().sortable(),
            Self::Ratio => Column::new("ratio", "Ratio").width(px(70.0)).text_right().sortable(),
        }
    }

    /// Identifies the column in element ids.
    const fn key(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Name => "name",
            Self::Size => "size",
            Self::Progress => "progress",
            Self::Status => "status",
            Self::Download => "dlspeed",
            Self::Eta => "eta",
            Self::Upload => "upspeed",
            Self::Ratio => "ratio",
        }
    }

    /// The column with this [`key`](Self::key), if there is one.
    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.key() == key)
    }

    /// Put the columns in the order of `keys`, as saved by [`TorrentTable::column_order`].
    ///
    /// Unknown and repeated keys are skipped, and columns missing from `keys` (such as
    /// ones added since it was saved) go at the end in their default order.
    fn ordered<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<Self> {
        let mut columns = Vec::with_capacity(Self::ALL.len());
        for column in keys.into_iter().filter_map(Self::from_key).chain(Self::ALL) {
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        columns
    }

    /// Whether the column's values line up on the right (numbers).
    const fn is_right_aligned(self) -> bool {
        matches!(
            self,
            Self::Size | Self::Download | Self::Eta | Self::Upload | Self::Ratio
        )
    }

    fn cell(self, torrent: &Torrent) -> AnyElement {
        let text: SharedString = match self {
            Self::Progress => return progress_cell(torrent),
            Self::Eta if torrent.state.is_downloading() => format_eta(torrent.eta).into(),
            // The toggle is rendered by `TorrentTable::toggle_button`, which has the
            // client; the ETA is blank unless the torrent is downloading.
            Self::Toggle | Self::Eta => SharedString::default(),
            Self::Name => torrent.name.clone().into(),
            Self::Size => format_bytes(torrent.size).into(),
            Self::Status => torrent.state.to_string().into(),
            Self::Download => format_speed(torrent.dlspeed).into(),
            Self::Upload => format_speed(torrent.upspeed).into(),
            Self::Ratio => format!("{:.2}", torrent.ratio).into(),
        };

        let id = SharedString::from(format!("cell-{}-{}", torrent.hash, self.key()));
        text_cell(id, text, self.is_right_aligned())
    }

    fn compare(self, a: &Torrent, b: &Torrent) -> Ordering {
        match self {
            Self::Toggle => a.state.is_stopped().cmp(&b.state.is_stopped()),
            Self::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Self::Size => a.size.cmp(&b.size),
            Self::Progress => a.progress.total_cmp(&b.progress),
            Self::Status => a.state.to_string().cmp(&b.state.to_string()),
            Self::Download => a.dlspeed.cmp(&b.dlspeed),
            Self::Eta => eta_sort_key(a).cmp(&eta_sort_key(b)),
            Self::Upload => a.upspeed.cmp(&b.upspeed),
            Self::Ratio => a.ratio.total_cmp(&b.ratio),
        }
    }
}

/// Supplies torrents to a gpui-kit `DataTable`.
pub struct TorrentTable {
    /// The filtered, sorted torrents being shown.
    torrents: Vec<Torrent>,
    /// Every torrent, in the order qBittorrent sent them.
    original: Vec<Torrent>,
    /// Which torrents are shown.
    filter: Filter,
    /// Only torrents whose name contains this (case-insensitively) are shown.
    query: String,
    /// The column and direction the user sorted by, re-applied after every refresh.
    sort: Option<(TorrentColumn, ColumnSort)>,
    /// Whether the torrents are still being fetched.
    loading: bool,
    /// Used to start/stop torrents; `None` until logged in.
    actions: Option<Actions>,
    /// Columns in display order. The table passes display indices to the delegate, so
    /// this follows the user dragging columns around.
    columns: Vec<TorrentColumn>,
    /// The row last pressed while it was already selected. Clicking the selected
    /// torrent again deselects it, which closes the details panel.
    pressed_selected_row: Option<usize>,
}

/// What a start/stop button needs to send its request.
#[derive(Clone)]
struct Actions {
    client: Client,
    runtime: Handle,
}

/// Which torrents a start/stop request applies to.
enum Target {
    One(String),
    All,
}

impl Actions {
    /// Start (or, if `start` is `false`, stop) `target` in the background.
    ///
    /// The new state shows up on the next refresh.
    fn send(&self, start: bool, target: Target) {
        let client = self.client.clone();
        self.runtime.spawn(async move {
            let result = match (&target, start) {
                (Target::One(hash), true) => client.start(&[hash]).await,
                (Target::One(hash), false) => client.stop(&[hash]).await,
                (Target::All, true) => client.start_all().await,
                (Target::All, false) => client.stop_all().await,
            };
            if let Err(err) = result {
                let verb = if start { "start" } else { "stop" };
                match target {
                    Target::One(hash) => eprintln!("Failed to {verb} torrent {hash}: {err}"),
                    Target::All => eprintln!("Failed to {verb} all torrents: {err}"),
                }
            }
        });
    }
}

/// A torrent that can be deleted, once the user confirms.
#[derive(Clone)]
pub struct DeleteRequest {
    actions: Actions,
    hash: String,
    name: String,
}

impl DeleteRequest {
    /// Ask whether to delete the torrent, offering to also remove its content files,
    /// and delete it if the user agrees.
    ///
    /// The torrent disappears on the next refresh.
    pub fn confirm(self, window: &Window, cx: &mut App) {
        let delete_files = Rc::new(Cell::new(false));

        // Deferred so a context menu that triggered this has closed (and given back
        // focus) before the dialog takes it.
        window.defer(cx, move |window, cx| {
            window.open_alert_dialog(cx, move |alert, _, _| {
                let toggle = Rc::clone(&delete_files);
                let chosen = Rc::clone(&delete_files);
                let request = self.clone();

                alert
                    .confirm()
                    .title("Delete torrent?")
                    .description(format!(
                        "\u{201c}{}\u{201d} will be removed from qBittorrent.",
                        self.name
                    ))
                    .child(
                        Checkbox::new("delete-files")
                            .label("Also remove content files")
                            .checked(delete_files.get())
                            .on_click(move |checked, window, _| {
                                toggle.set(*checked);
                                window.refresh();
                            }),
                    )
                    .ok_text("Delete")
                    .ok_variant(ButtonVariant::Danger)
                    .on_ok(move |_, _, _| {
                        request.send(chosen.get());
                        true
                    })
            });
        });
    }

    /// Delete the torrent in the background, with its files if `delete_files`.
    fn send(&self, delete_files: bool) {
        let client = self.actions.client.clone();
        let hash = self.hash.clone();
        self.actions.runtime.spawn(async move {
            if let Err(err) = client.delete(&[&hash], delete_files).await {
                eprintln!("Failed to delete torrent {hash}: {err}");
            }
        });
    }
}

impl TorrentTable {
    const SEARCHIFY_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\W+").unwrap());

    pub const fn new() -> Self {
        Self {
            torrents: Vec::new(),
            original: Vec::new(),
            filter: Filter::All,
            query: String::new(),
            sort: None,
            loading: true,
            actions: None,
            columns: Vec::new(),
            pressed_selected_row: None,
        }
    }

    /// Show the columns in the order of `keys`, as saved by [`Self::column_order`].
    ///
    /// Call before the table is built from this delegate, which reads the columns once.
    pub fn set_column_order<'a>(&mut self, keys: impl IntoIterator<Item = &'a str>) {
        self.columns = TorrentColumn::ordered(keys);
    }

    /// The keys of the columns in display order, for [`Self::set_column_order`].
    pub fn column_order(&self) -> Vec<String> {
        let columns = if self.columns.is_empty() {
            TorrentColumn::ALL.as_slice()
        } else {
            self.columns.as_slice()
        };
        columns.iter().map(|column| column.key().to_owned()).collect()
    }

    /// The column shown at display index `col_ix`.
    fn column_at(&self, col_ix: usize) -> Option<TorrentColumn> {
        if self.columns.is_empty() {
            TorrentColumn::ALL.get(col_ix).copied()
        } else {
            self.columns.get(col_ix).copied()
        }
    }

    /// Let the play/pause buttons use `client`, sending requests on `runtime`.
    pub fn set_client(&mut self, client: Client, runtime: Handle) {
        self.actions = Some(Actions { client, runtime });
    }

    /// A button that starts a stopped torrent, or stops a running one.
    ///
    /// The new state shows up on the next refresh.
    fn toggle_button(&self, torrent: &Torrent) -> AnyElement {
        let stopped = torrent.state.is_stopped();
        let (icon, tooltip) = if stopped {
            (IconName::Play, "Start")
        } else {
            (IconName::Pause, "Stop")
        };
        let actions = self.actions.clone();
        let hash = torrent.hash.clone();

        Button::new(SharedString::from(format!("toggle-{}", torrent.hash)))
            .icon(icon)
            .ghost()
            .xsmall()
            .tooltip(tooltip)
            .disabled(actions.is_none())
            .on_click(move |_, _, cx| {
                // Don't let the click also select the row.
                cx.stop_propagation();

                if let Some(actions) = &actions {
                    actions.send(stopped, Target::One(hash.clone()));
                }
            })
            .into_any_element()
    }

    /// A request to delete `torrent`; `None` until logged in.
    pub fn delete_request(&self, torrent: &Torrent) -> Option<DeleteRequest> {
        self.actions.clone().map(|actions| DeleteRequest {
            actions,
            hash: torrent.hash.clone(),
            name: torrent.name.clone(),
        })
    }

    /// Limit `hash`'s download (or, if `download` is `false`, upload) speed to
    /// `limit` bytes per second, `0` meaning unlimited, in the background.
    ///
    /// The new limit shows up on the next refresh. Returns the request, which
    /// finishes with whether it worked once qBittorrent has answered; `None` if
    /// not logged in.
    pub fn send_limit(&self, hash: &str, download: bool, limit: u64) -> Option<tokio::task::JoinHandle<bool>> {
        let actions = self.actions.as_ref()?;
        let client = actions.client.clone();
        let hash = hash.to_owned();
        let request = actions.runtime.spawn(async move {
            let result = if download {
                client.set_download_limit(&[&hash], limit).await
            } else {
                client.set_upload_limit(&[&hash], limit).await
            };
            if let Err(err) = result {
                let which = if download { "download" } else { "upload" };
                eprintln!("Failed to set the {which} limit of torrent {hash}: {err}");
                return false;
            }
            true
        });
        Some(request)
    }

    /// Whether start/stop requests can be sent yet (i.e. logged in).
    pub const fn can_send(&self) -> bool {
        self.actions.is_some()
    }

    /// Start (or, if `start` is `false`, stop) every torrent.
    pub fn send_all(&self, start: bool) {
        if let Some(actions) = &self.actions {
            actions.send(start, Target::All);
        }
    }

    /// Replace the torrents, keeping the filter and sort, and stop showing the loading state.
    pub fn set_torrents(&mut self, torrents: Vec<Torrent>) {
        self.original = torrents;
        self.apply_view();
        self.loading = false;
    }

    pub const fn filter(&self) -> Filter {
        self.filter
    }

    /// Show only the torrents matching `filter`.
    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
        self.apply_view();
    }

    /// Show only torrents whose name contains `query`, ignoring case and surrounding spaces.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.trim().to_lowercase();
        self.apply_view();
    }

    /// How many torrents match `filter` and the search, regardless of the current filter.
    pub fn count(&self, filter: Filter) -> usize {
        self.original
            .iter()
            .filter(|torrent| filter.matches(torrent) && self.matches_query(torrent))
            .count()
    }

    /// Turn a string into a searchable value by removing all non a-z/0-9 characters.
    fn searchify(value: &String) -> String {
        Self::SEARCHIFY_PATTERN.replace_all(value, "").to_lowercase()
    }

    fn matches_query(&self, torrent: &Torrent) -> bool {
        self.query.is_empty() || Self::searchify(&torrent.name).contains(&Self::searchify(&self.query))
    }

    /// Whether a row selection is a click on row `row_ix` while it was already
    /// selected. Forgets the press either way.
    pub const fn take_reselected(&mut self, row_ix: usize) -> bool {
        matches!(self.pressed_selected_row.take(), Some(pressed) if pressed == row_ix)
    }

    /// The torrent shown in row `row_ix`.
    pub fn torrent_at(&self, row_ix: usize) -> Option<&Torrent> {
        self.torrents.get(row_ix)
    }

    /// The hash of the torrent shown in row `row_ix`.
    pub fn hash_at(&self, row_ix: usize) -> Option<&str> {
        self.torrents.get(row_ix).map(|torrent| torrent.hash.as_str())
    }

    /// The row the torrent with `hash` is shown in.
    pub fn row_of(&self, hash: &str) -> Option<usize> {
        self.torrents.iter().position(|torrent| torrent.hash == hash)
    }

    /// Rebuild the rows: filter and search, then order by the remembered sort (or
    /// qBittorrent's order).
    fn apply_view(&mut self) {
        let filter = self.filter;
        self.torrents = self
            .original
            .iter()
            .filter(|torrent| filter.matches(torrent) && self.matches_query(torrent))
            .cloned()
            .collect();

        match self.sort {
            None | Some((_, ColumnSort::Default)) => {}
            Some((column, ColumnSort::Ascending)) => {
                self.torrents.sort_by(|a, b| column.compare(a, b));
            }
            Some((column, ColumnSort::Descending)) => {
                self.torrents.sort_by(|a, b| column.compare(b, a));
            }
        }
    }
}

impl TableDelegate for TorrentTable {
    fn columns_count(&self, _cx: &App) -> usize {
        TorrentColumn::ALL.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.torrents.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.column_at(col_ix)
            .map_or_else(Column::default, TorrentColumn::column)
    }

    fn move_column(&mut self, col_ix: usize, to_ix: usize, _window: &mut Window, _cx: &mut Context<TableState<Self>>) {
        if self.columns.is_empty() {
            self.columns = TorrentColumn::ALL.to_vec();
        }
        if col_ix < self.columns.len() && to_ix < self.columns.len() {
            let column = self.columns.remove(col_ix);
            self.columns.insert(to_ix, column);
            let order = self.column_order();
            Prefs::update(|prefs| prefs.column_order = order);
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        let Some(column) = self.column_at(col_ix) else {
            return;
        };

        self.sort = Some((column, sort));
        self.apply_view();
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let torrent = self.torrents.get(row_ix);
        let column = self.column_at(col_ix);

        match (torrent, column) {
            (Some(torrent), Some(TorrentColumn::Toggle)) => self.toggle_button(torrent),
            (Some(torrent), Some(column)) => column.cell(torrent),
            _ => SharedString::default().into_any_element(),
        }
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(torrent) = self.torrents.get(row_ix) else {
            return menu;
        };

        let name = torrent.name.clone();
        let delete = self.delete_request(torrent);
        let danger = cx.theme().danger;
        // Older qBittorrent versions don't report `content_path`.
        let path = if torrent.content_path.is_empty() {
            torrent.save_path.clone()
        } else {
            torrent.content_path.clone()
        };

        menu.item(
            PopupMenuItem::new("Copy name")
                .icon(IconName::Copy)
                .on_click(move |_, _, cx| copy(cx, &name)),
        )
        .item(
            PopupMenuItem::new("Copy file path")
                .icon(IconName::FolderOpen)
                .on_click(move |_, _, cx| copy(cx, &path)),
        )
        .separator()
        .item(
            // Red, as deleting can't be undone.
            PopupMenuItem::element(move |_, _| div().text_color(danger).child("Delete…"))
                .icon(Icon::new(IconName::Trash).text_color(danger))
                .disabled(delete.is_none())
                .on_click(move |_, window, cx| {
                    if let Some(delete) = delete.clone() {
                        delete.confirm(window, cx);
                    }
                }),
        )
    }

    fn render_tr(&mut self, row_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        // Runs before the table's click handler selects the row, so it can tell
        // whether the row was already selected.
        div().id(("row", row_ix)).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |table, _, _, _| {
                let was_selected = table.selected_row() == Some(row_ix);
                table.delegate_mut().pressed_selected_row = was_selected.then_some(row_ix);
            }),
        )
    }

    fn loading(&self, _cx: &App) -> bool {
        self.loading
    }
}

/// Put `text` on the clipboard.
fn copy(cx: &App, text: &str) {
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
}

/// A single-line cell that cuts long text off with an ellipsis and shows the whole
/// value in a tooltip on hover.
fn text_cell(id: SharedString, text: SharedString, right_aligned: bool) -> AnyElement {
    let cell = div()
        .id(id)
        .w_full()
        .min_w_0()
        .truncate()
        .when(right_aligned, Styled::text_right)
        .child(text.clone());

    if text.is_empty() {
        cell.into_any_element()
    } else {
        cell.tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
            .into_any_element()
    }
}

/// The progress percentage, with a progress bar in front while the torrent is downloading.
fn progress_cell(torrent: &Torrent) -> AnyElement {
    let percent = torrent.progress * 100.0;
    let label = SharedString::from(format!("{percent:.1}%"));
    let tooltip = label.clone();

    h_flex()
        .id(SharedString::from(format!("cell-{}-progress", torrent.hash)))
        .w_full()
        .gap_2()
        .justify_end()
        .when(torrent.state.is_downloading(), |row| {
            row.child(
                // Keyed by hash so the bar animates smoothly even when rows are re-sorted.
                Progress::new(SharedString::from(format!("progress-{}", torrent.hash)))
                    .value(percent_f32(percent))
                    .xsmall()
                    .flex_1(),
            )
        })
        .child(h_flex().flex_shrink_0().w(px(PERCENT_WIDTH)).justify_end().child(label))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .into_any_element()
}

/// Orders torrents by time left: downloading ones soonest first, then those that will
/// never finish (`∞`), then everything not downloading (blank).
const fn eta_sort_key(torrent: &Torrent) -> (bool, i64) {
    (!torrent.state.is_downloading(), torrent.eta)
}

/// Narrow a percentage to the `f32` the progress bar takes.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "a 0–100 percentage fits in f32 with room to spare"
)]
const fn percent_f32(percent: f64) -> f32 {
    percent as f32
}

#[cfg(test)]
mod tests {
    use super::{TorrentColumn, TorrentTable};

    #[test]
    fn keeps_a_saved_column_order() {
        let mut table = TorrentTable::new();
        let saved = [
            "toggle", "name", "ratio", "size", "progress", "status", "dlspeed", "eta", "upspeed",
        ];
        table.set_column_order(saved);
        assert_eq!(table.column_order(), saved);
    }

    #[test]
    fn defaults_to_every_column_in_order() {
        assert_eq!(TorrentColumn::ordered([]), TorrentColumn::ALL);
        let keys = TorrentTable::new().column_order();
        let expected: Vec<_> = TorrentColumn::ALL.iter().map(|column| column.key()).collect();
        assert_eq!(keys, expected);
    }

    #[test]
    fn skips_unknown_and_repeated_columns_and_adds_missing_ones() {
        let columns = TorrentColumn::ordered(["ratio", "gone", "ratio", "name"]);
        assert_eq!(
            columns,
            [
                TorrentColumn::Ratio,
                TorrentColumn::Name,
                TorrentColumn::Toggle,
                TorrentColumn::Size,
                TorrentColumn::Progress,
                TorrentColumn::Status,
                TorrentColumn::Download,
                TorrentColumn::Eta,
                TorrentColumn::Upload,
            ]
        );
    }
}
