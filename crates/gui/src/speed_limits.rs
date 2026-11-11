use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, Subscription, Window,
    component::input::{InputEvent, InputState},
};
use qbittorrent::Torrent;

use crate::torrent_table::TorrentTable;

/// Limits are typed in KiB/s and sent to qBittorrent in bytes per second.
const BYTES_PER_KIB: u64 = 1024;

/// How far the up/down arrows move a limit, in KiB/s.
const STEP_KIB: f64 = 100.0;

/// Which speed a limit applies to.
#[derive(Clone, Copy)]
pub enum Direction {
    Download,
    Upload,
}

/// The details panel's download and upload limit inputs, in KiB/s. Empty means unlimited.
pub struct SpeedLimits {
    pub download: Entity<InputState>,
    pub upload: Entity<InputState>,
    /// The torrent the inputs were last filled in for.
    hash: Option<String>,
    /// Limits sent but not yet reported back by qBittorrent, as shown in the inputs.
    /// Until they are, refreshes leave the typed value alone instead of flashing the old one.
    pending_download: Option<String>,
    pending_upload: Option<String>,
}

impl SpeedLimits {
    pub fn new<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Self {
        let input = |window: &mut Window, cx: &mut Context<V>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Unlimited")
                    .validate(|text, _| text.chars().all(|c| c.is_ascii_digit()))
                    .step(STEP_KIB)
                    .min(0.0)
            })
        };

        Self {
            download: input(window, cx),
            upload: input(window, cx),
            hash: None,
            pending_download: None,
            pending_upload: None,
        }
    }

    /// Calls `on_commit` when the user presses Enter in, or leaves, either input.
    pub fn subscribe<V: 'static>(
        &self,
        window: &Window,
        cx: &mut Context<V>,
        on_commit: impl Fn(&mut V, Direction, &mut Context<V>) + Copy + 'static,
    ) -> [Subscription; 2] {
        [(Direction::Download, &self.download), (Direction::Upload, &self.upload)].map(|(direction, input)| {
            cx.subscribe_in(input, window, move |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    on_commit(this, direction, cx);
                }
            })
        })
    }

    /// Show `torrent`'s current limits, unless the user is editing one.
    ///
    /// A newly selected torrent always replaces what's typed.
    pub fn sync(&mut self, torrent: &Torrent, window: &mut Window, cx: &mut App) {
        let new_torrent = self.hash.as_deref() != Some(torrent.hash.as_str());
        self.hash = Some(torrent.hash.clone());

        let fields = [
            (&self.download, &mut self.pending_download, torrent.dl_limit),
            (&self.upload, &mut self.pending_upload, torrent.up_limit),
        ];
        for (input, pending, limit) in fields {
            let shown = format_limit(limit);
            if new_torrent || pending.as_deref() == Some(shown.as_str()) {
                *pending = None;
            }

            let state = input.read(cx);
            let editing = state.focus_handle(cx).is_focused(window);
            if new_torrent || (!editing && pending.is_none() && state.value() != shown.as_str()) {
                input.update(cx, |state, cx| state.set_value(shown, window, cx));
            }
        }
    }

    /// Send what's typed in the `direction` input for the selected torrent, if it
    /// differs from the torrent's current limit.
    pub fn commit(&mut self, direction: Direction, torrent: &Torrent, table: &TorrentTable, cx: &App) {
        let (input, pending, current) = match direction {
            Direction::Download => (&self.download, &mut self.pending_download, torrent.dl_limit),
            Direction::Upload => (&self.upload, &mut self.pending_upload, torrent.up_limit),
        };
        let text = input.read(cx).value();
        let text = text.trim();
        if text == format_limit(current) {
            return;
        }

        // Empty (or 0) is unlimited; anything unparseable is put back on the next sync.
        let Ok(kib) = (if text.is_empty() { Ok(0) } else { text.parse::<u64>() }) else {
            return;
        };
        let bytes = kib.saturating_mul(BYTES_PER_KIB);
        *pending = Some(format_limit(i64::try_from(bytes).unwrap_or(i64::MAX)));
        let download = matches!(direction, Direction::Download);
        table.send_limit(&torrent.hash, download, bytes);
    }
}

/// A limit in bytes per second as the KiB/s typed in the inputs; empty for unlimited.
fn format_limit(limit: i64) -> String {
    match u64::try_from(limit) {
        // Round up, so a limit under 1 KiB/s doesn't show as 0 (unlimited).
        Ok(bytes) if bytes > 0 => bytes.div_ceil(BYTES_PER_KIB).to_string(),
        _ => String::new(),
    }
}
