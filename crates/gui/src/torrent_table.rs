use std::cmp::Ordering;

use gpui_kit::{
    AnyElement, App, Context, IntoElement, SharedString, Window,
    assets::IconName,
    base::h_flex,
    component::{
        Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        progress::Progress,
        table::{Column, ColumnSort, TableDelegate, TableState},
    },
    prelude::*,
    px,
};
use qbittorrent::{Client, Torrent};
use tokio::runtime::Handle;

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
#[derive(Clone, Copy)]
enum TorrentColumn {
    /// Play/pause button.
    Toggle,
    Name,
    Size,
    Progress,
    Status,
    Download,
    Upload,
    Ratio,
}

impl TorrentColumn {
    const ALL: [Self; 8] = [
        Self::Toggle,
        Self::Name,
        Self::Size,
        Self::Progress,
        Self::Status,
        Self::Download,
        Self::Upload,
        Self::Ratio,
    ];

    fn from_index(col_ix: usize) -> Option<Self> {
        Self::ALL.get(col_ix).copied()
    }

    fn column(self) -> Column {
        match self {
            Self::Toggle => Column::new("toggle", "")
                .width(px(40.0))
                .text_center()
                .resizable(false)
                .movable(false),
            Self::Name => Column::new("name", "Name").width(px(280.0)).sortable(),
            Self::Size => Column::new("size", "Size").width(px(90.0)).text_right().sortable(),
            Self::Progress => Column::new("progress", "Progress")
                .width(px(160.0))
                .sortable(),
            Self::Status => Column::new("status", "Status").width(px(140.0)).sortable(),
            Self::Download => Column::new("dlspeed", "Down")
                .width(px(100.0))
                .text_right()
                .sortable(),
            Self::Upload => Column::new("upspeed", "Up")
                .width(px(100.0))
                .text_right()
                .sortable(),
            Self::Ratio => Column::new("ratio", "Ratio")
                .width(px(70.0))
                .text_right()
                .sortable(),
        }
    }

    fn cell(self, torrent: &Torrent) -> AnyElement {
        let text: SharedString = match self {
            Self::Progress => return progress_cell(torrent),
            // Rendered by `TorrentTable::toggle_button`, which has the client.
            Self::Toggle => SharedString::default(),
            Self::Name => torrent.name.clone().into(),
            Self::Size => format_bytes(torrent.size).into(),
            Self::Status => torrent.state.to_string().into(),
            Self::Download => format_speed(torrent.dlspeed).into(),
            Self::Upload => format_speed(torrent.upspeed).into(),
            Self::Ratio => format!("{:.2}", torrent.ratio).into(),
        };
        text.into_any_element()
    }

    fn compare(self, a: &Torrent, b: &Torrent) -> Ordering {
        match self {
            Self::Toggle => a.state.is_stopped().cmp(&b.state.is_stopped()),
            Self::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Self::Size => a.size.cmp(&b.size),
            Self::Progress => a.progress.total_cmp(&b.progress),
            Self::Status => a.state.to_string().cmp(&b.state.to_string()),
            Self::Download => a.dlspeed.cmp(&b.dlspeed),
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
    /// The column and direction the user sorted by, re-applied after every refresh.
    sort: Option<(TorrentColumn, ColumnSort)>,
    /// Whether the torrents are still being fetched.
    loading: bool,
    /// Used to start/stop torrents; `None` until logged in.
    actions: Option<Actions>,
}

/// What a play/pause button needs to send its request.
#[derive(Clone)]
struct Actions {
    client: Client,
    runtime: Handle,
}

impl TorrentTable {
    pub const fn new() -> Self {
        Self {
            torrents: Vec::new(),
            original: Vec::new(),
            filter: Filter::All,
            sort: None,
            loading: true,
            actions: None,
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

                let Some(Actions { client, runtime }) = actions.clone() else {
                    return;
                };
                let hash = hash.clone();
                runtime.spawn(async move {
                    let result = if stopped {
                        client.start(&[&hash]).await
                    } else {
                        client.stop(&[&hash]).await
                    };
                    if let Err(err) = result {
                        eprintln!("Failed to {} torrent {hash}: {err}", tooltip.to_lowercase());
                    }
                });
            })
            .into_any_element()
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

    /// How many torrents match `filter`, regardless of the current one.
    pub fn count(&self, filter: Filter) -> usize {
        self.original.iter().filter(|torrent| filter.matches(torrent)).count()
    }

    /// The hash of the torrent shown in row `row_ix`.
    pub fn hash_at(&self, row_ix: usize) -> Option<&str> {
        self.torrents.get(row_ix).map(|torrent| torrent.hash.as_str())
    }

    /// The row the torrent with `hash` is shown in.
    pub fn row_of(&self, hash: &str) -> Option<usize> {
        self.torrents.iter().position(|torrent| torrent.hash == hash)
    }

    /// Rebuild the rows: filter, then order by the remembered sort (or qBittorrent's order).
    fn apply_view(&mut self) {
        let filter = self.filter;
        self.torrents = self
            .original
            .iter()
            .filter(|torrent| filter.matches(torrent))
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
        TorrentColumn::from_index(col_ix).map_or_else(Column::default, TorrentColumn::column)
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) {
        let Some(column) = TorrentColumn::from_index(col_ix) else {
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
        let column = TorrentColumn::from_index(col_ix);

        match (torrent, column) {
            (Some(torrent), Some(TorrentColumn::Toggle)) => self.toggle_button(torrent),
            (Some(torrent), Some(column)) => column.cell(torrent),
            _ => SharedString::default().into_any_element(),
        }
    }

    fn loading(&self, _cx: &App) -> bool {
        self.loading
    }
}

/// The progress percentage, with a progress bar in front while the torrent is downloading.
fn progress_cell(torrent: &Torrent) -> AnyElement {
    let percent = torrent.progress * 100.0;

    h_flex()
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
        .child(
            h_flex()
                .flex_shrink_0()
                .w(px(PERCENT_WIDTH))
                .justify_end()
                .child(format!("{percent:.1}%")),
        )
        .into_any_element()
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

/// Format a byte count with binary units, e.g. `1.4 GiB`.
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

    // Work in tenths so one decimal place survives without floating point.
    let mut tenths = u128::from(bytes).saturating_mul(10);
    let mut unit = 0;
    while tenths >= 10 * 1024 && unit < UNITS.len().saturating_sub(1) {
        tenths /= 1024;
        unit = unit.saturating_add(1);
    }

    let suffix = UNITS.get(unit).copied().unwrap_or_default();
    if unit == 0 {
        format!("{bytes} {suffix}")
    } else {
        format!("{}.{} {suffix}", tenths / 10, tenths % 10)
    }
}

/// Format a transfer rate in bytes per second, e.g. `2.1 MiB/s`, blank when idle.
fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        String::new()
    } else {
        format!("{}/s", format_bytes(bytes_per_sec))
    }
}
