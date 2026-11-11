use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, Subscription, Window,
    component::input::{InputEvent, InputState},
};
use qbittorrent::Torrent;
use tokio::task::JoinHandle;

use crate::torrent_table::TorrentTable;

/// Limits are typed in KiB/s and sent to qBittorrent in bytes per second.
const BYTES_PER_KIB: u64 = 1024;

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
    /// The request being sent for each limit, if any: the torrent's hash and the
    /// request's id. The input is disabled while its torrent's request is in flight.
    sending_download: Option<(String, u64)>,
    sending_upload: Option<(String, u64)>,
    /// Tells requests apart, so a finished one only clears its own entry.
    next_request_id: u64,
}

/// A limit request that [`SpeedLimits::commit`] sent. Await `request` (whether it
/// worked), then call [`SpeedLimits::finish`] to enable the input again.
pub struct Sent {
    pub direction: Direction,
    pub id: u64,
    pub request: JoinHandle<bool>,
}

impl SpeedLimits {
    pub fn new<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Self {
        let input = |window: &mut Window, cx: &mut Context<V>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Unlimited")
                    .validate(|text, _| text.chars().all(|c| c.is_ascii_digit()))
            })
        };

        Self {
            download: input(window, cx),
            upload: input(window, cx),
            hash: None,
            pending_download: None,
            pending_upload: None,
            sending_download: None,
            sending_upload: None,
            next_request_id: 0,
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

    /// Whether the `direction` limit of the shown torrent is being sent.
    pub fn is_sending(&self, direction: Direction) -> bool {
        let sending = match direction {
            Direction::Download => &self.sending_download,
            Direction::Upload => &self.sending_upload,
        };
        matches!((sending, &self.hash), (Some((sending, _)), Some(hash)) if sending == hash)
    }

    /// Send what's typed in the `direction` input for the selected torrent, if it
    /// differs from the torrent's current limit and nothing is being sent for it yet.
    pub fn commit(&mut self, direction: Direction, torrent: &Torrent, table: &TorrentTable, cx: &App) -> Option<Sent> {
        if self.is_sending(direction) {
            return None;
        }
        let (input, pending, sending, current) = match direction {
            Direction::Download => (
                &self.download,
                &mut self.pending_download,
                &mut self.sending_download,
                torrent.dl_limit,
            ),
            Direction::Upload => (
                &self.upload,
                &mut self.pending_upload,
                &mut self.sending_upload,
                torrent.up_limit,
            ),
        };
        let text = input.read(cx).value();
        let text = text.trim();
        if text == format_limit(current) || pending.as_deref() == Some(text) {
            return None;
        }

        // Empty (or 0) is unlimited; anything unparseable is put back on the next sync.
        let kib = if text.is_empty() { 0 } else { text.parse::<u64>().ok()? };
        let bytes = kib.saturating_mul(BYTES_PER_KIB);
        let download = matches!(direction, Direction::Download);
        let request = table.send_limit(&torrent.hash, download, bytes)?;

        *pending = Some(format_limit(i64::try_from(bytes).unwrap_or(i64::MAX)));
        let id = self.next_request_id;
        self.next_request_id = id.wrapping_add(1);
        *sending = Some((torrent.hash.clone(), id));
        Some(Sent { direction, id, request })
    }

    /// Mark request `id` as finished, enabling its input again. If it failed, the
    /// next sync puts the torrent's actual limit back in the input.
    pub fn finish(&mut self, direction: Direction, id: u64, succeeded: bool) {
        let (sending, pending) = match direction {
            Direction::Download => (&mut self.sending_download, &mut self.pending_download),
            Direction::Upload => (&mut self.sending_upload, &mut self.pending_upload),
        };
        if matches!(sending, Some((_, sending_id)) if *sending_id == id) {
            *sending = None;
            if !succeeded {
                *pending = None;
            }
        }
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
