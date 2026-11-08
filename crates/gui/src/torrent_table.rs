use std::cmp::Ordering;

use gpui_kit::{
    App, Context, IntoElement, SharedString, Window,
    component::table::{Column, ColumnSort, TableDelegate, TableState},
    px,
};
use qbittorrent::Torrent;

/// The columns shown in the torrent table, in display order.
#[derive(Clone, Copy)]
enum TorrentColumn {
    Name,
    Size,
    Progress,
    Status,
    Download,
    Upload,
    Ratio,
}

impl TorrentColumn {
    const ALL: [Self; 7] = [
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
            Self::Name => Column::new("name", "Name").width(px(280.0)).sortable(),
            Self::Size => Column::new("size", "Size").width(px(90.0)).text_right().sortable(),
            Self::Progress => Column::new("progress", "Progress")
                .width(px(90.0))
                .text_right()
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

    fn cell(self, torrent: &Torrent) -> SharedString {
        match self {
            Self::Name => torrent.name.clone().into(),
            Self::Size => format_bytes(torrent.size).into(),
            Self::Progress => format!("{:.1}%", torrent.progress * 100.0).into(),
            Self::Status => torrent.state.to_string().into(),
            Self::Download => format_speed(torrent.dlspeed).into(),
            Self::Upload => format_speed(torrent.upspeed).into(),
            Self::Ratio => format!("{:.2}", torrent.ratio).into(),
        }
    }

    fn compare(self, a: &Torrent, b: &Torrent) -> Ordering {
        match self {
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
    /// Torrents in display order.
    torrents: Vec<Torrent>,
    /// Torrents in the order qBittorrent sent them, restored when sorting is cleared.
    original: Vec<Torrent>,
    /// The column and direction the user sorted by, re-applied after every refresh.
    sort: Option<(TorrentColumn, ColumnSort)>,
    /// Whether the torrents are still being fetched.
    loading: bool,
}

impl TorrentTable {
    pub const fn new() -> Self {
        Self {
            torrents: Vec::new(),
            original: Vec::new(),
            sort: None,
            loading: true,
        }
    }

    /// Replace the rows, keeping the current sort, and stop showing the loading state.
    pub fn set_torrents(&mut self, torrents: Vec<Torrent>) {
        self.original.clone_from(&torrents);
        self.torrents = torrents;
        self.apply_sort();
        self.loading = false;
    }

    /// The hash of the torrent shown in row `row_ix`.
    pub fn hash_at(&self, row_ix: usize) -> Option<&str> {
        self.torrents.get(row_ix).map(|torrent| torrent.hash.as_str())
    }

    /// The row the torrent with `hash` is shown in.
    pub fn row_of(&self, hash: &str) -> Option<usize> {
        self.torrents.iter().position(|torrent| torrent.hash == hash)
    }

    /// Order the rows by the remembered sort, or qBittorrent's order if there is none.
    fn apply_sort(&mut self) {
        match self.sort {
            None | Some((_, ColumnSort::Default)) => self.torrents.clone_from(&self.original),
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
        self.apply_sort();
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
            (Some(torrent), Some(column)) => column.cell(torrent),
            _ => SharedString::default(),
        }
    }

    fn loading(&self, _cx: &App) -> bool {
        self.loading
    }
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
