use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, Subscription, Window,
    component::input::{InputEvent, InputState},
};
use qbittorrent::{Torrent, TransferInfo};
use tokio::task::JoinHandle;

/// Limits are typed in KiB/s and sent to qBittorrent in bytes per second.
const BYTES_PER_KIB: u64 = 1024;

/// Which speed a limit applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Download,
    Upload,
}

/// Download and upload limit inputs, in KiB/s, for a torrent or for every torrent.
/// Empty means unlimited.
pub struct SpeedLimits {
    pub download: Entity<InputState>,
    pub upload: Entity<InputState>,
    /// What the inputs were last filled in for, e.g. a torrent's hash.
    key: Option<String>,
    /// Limits sent but not yet reported back by qBittorrent, as shown in the inputs.
    /// Until they are, refreshes leave the typed value alone instead of flashing the old one.
    pending_download: Option<String>,
    pending_upload: Option<String>,
    /// The request being sent for each limit, if any: the key it was sent for and the
    /// request's id. The input is disabled while its key's request is in flight.
    sending_download: Option<(String, u64)>,
    sending_upload: Option<(String, u64)>,
    /// Tells requests apart, so a finished one only clears its own entry.
    next_request_id: u64,
}

/// Download and upload speed limits in bytes per second; `0` or negative means unlimited.
#[derive(Clone, Copy, Default)]
pub struct Limits {
    pub download: i64,
    pub upload: i64,
}

impl From<&Torrent> for Limits {
    fn from(torrent: &Torrent) -> Self {
        Self {
            download: torrent.dl_limit,
            upload: torrent.up_limit,
        }
    }
}

impl From<&TransferInfo> for Limits {
    fn from(transfer: &TransferInfo) -> Self {
        Self {
            download: transfer.dl_rate_limit,
            upload: transfer.up_rate_limit,
        }
    }
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
            key: None,
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

    /// Show `key`'s current limits, in bytes per second, unless the user is editing one.
    ///
    /// A new key (e.g. a newly selected torrent) always replaces what's typed.
    pub fn sync(&mut self, key: &str, limits: Limits, window: &mut Window, cx: &mut App) {
        let new_key = self.key.as_deref() != Some(key);
        if new_key {
            self.key = Some(key.to_owned());
        }

        let fields = [
            (&self.download, &mut self.pending_download, limits.download),
            (&self.upload, &mut self.pending_upload, limits.upload),
        ];
        for (input, pending, limit) in fields {
            let shown = format_limit(limit);
            if new_key || pending.as_deref() == Some(shown.as_str()) {
                *pending = None;
            }

            let state = input.read(cx);
            let editing = state.focus_handle(cx).is_focused(window);
            if new_key || (!editing && pending.is_none() && state.value() != shown.as_str()) {
                input.update(cx, |state, cx| state.set_value(shown, window, cx));
            }
        }
    }

    /// Whether the `direction` limit of the shown key is being sent.
    pub fn is_sending(&self, direction: Direction) -> bool {
        let sending = match direction {
            Direction::Download => &self.sending_download,
            Direction::Upload => &self.sending_upload,
        };
        matches!((sending, &self.key), (Some((sending, _)), Some(key)) if sending == key)
    }

    /// Send what's typed in the `direction` input with `send`, which takes the new
    /// limit in bytes per second, if it differs from the `current` limits of the shown
    /// key and nothing is being sent for it yet.
    pub fn commit(
        &mut self,
        direction: Direction,
        current: Limits,
        send: impl FnOnce(u64) -> Option<JoinHandle<bool>>,
        cx: &App,
    ) -> Option<Sent> {
        if self.is_sending(direction) {
            return None;
        }
        let key = self.key.clone()?;
        let (input, pending, sending, current) = match direction {
            Direction::Download => (
                &self.download,
                &mut self.pending_download,
                &mut self.sending_download,
                current.download,
            ),
            Direction::Upload => (
                &self.upload,
                &mut self.pending_upload,
                &mut self.sending_upload,
                current.upload,
            ),
        };
        let text = input.read(cx).value();
        let text = text.trim();
        if text == format_limit(current) || pending.as_deref() == Some(text) {
            return None;
        }

        // Anything unparseable is put back on the next sync.
        let bytes = parse_limit(text)?;
        let request = send(bytes)?;

        *pending = Some(format_limit(i64::try_from(bytes).unwrap_or(i64::MAX)));
        let id = self.next_request_id;
        self.next_request_id = id.wrapping_add(1);
        *sending = Some((key, id));
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

/// The KiB/s typed in an input as bytes per second; empty (or `0`) is `0`, unlimited.
fn parse_limit(text: &str) -> Option<u64> {
    let kib = if text.is_empty() { 0 } else { text.parse::<u64>().ok()? };
    Some(kib.saturating_mul(BYTES_PER_KIB))
}

/// A limit in bytes per second as the KiB/s typed in the inputs; empty for unlimited.
fn format_limit(limit: i64) -> String {
    match u64::try_from(limit) {
        // Round up, so a limit under 1 KiB/s doesn't show as 0 (unlimited).
        Ok(bytes) if bytes > 0 => bytes.div_ceil(BYTES_PER_KIB).to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_limit, parse_limit};

    #[test]
    fn parses_kib_into_bytes() {
        assert_eq!(parse_limit("500"), Some(512_000));
        assert_eq!(parse_limit("1"), Some(1024));
    }

    #[test]
    fn parses_empty_and_zero_as_unlimited() {
        assert_eq!(parse_limit(""), Some(0));
        assert_eq!(parse_limit("0"), Some(0));
    }

    #[test]
    fn rejects_what_isnt_a_number() {
        assert_eq!(parse_limit("fast"), None);
        assert_eq!(parse_limit("-5"), None);
        assert_eq!(parse_limit("1.5"), None);
    }

    #[test]
    fn saturates_huge_limits() {
        assert_eq!(parse_limit(&u64::MAX.to_string()), Some(u64::MAX));
    }

    #[test]
    fn formats_bytes_as_kib() {
        assert_eq!(format_limit(512_000), "500");
        assert_eq!(format_limit(1024), "1");
    }

    #[test]
    fn rounds_limits_under_a_kib_up() {
        assert_eq!(format_limit(1), "1");
        assert_eq!(format_limit(1025), "2");
    }

    #[test]
    fn formats_unlimited_as_empty() {
        assert_eq!(format_limit(0), "");
        assert_eq!(format_limit(-1), "");
    }

    #[test]
    fn round_trips_what_was_typed() {
        for typed in ["", "1", "500", "102400"] {
            let bytes = parse_limit(typed).map(i64::try_from);
            assert_eq!(bytes.map(|bytes| format_limit(bytes.unwrap())), Some(typed.to_owned()));
        }
    }
}

#[cfg(test)]
mod ui_tests {
    use std::cell::Cell;

    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Subscription,
        TestAppContext, Window, component::input::Input, div, test::TestWindowExt as _,
    };

    use super::{Direction, Limits, SpeedLimits};
    use crate::test_support::{open_window, runtime};

    /// Shows the two inputs, and records every commit they ask for.
    struct Host {
        limits: SpeedLimits,
        committed: Vec<Direction>,
        _subscriptions: [Subscription; 2],
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(Input::new(&self.limits.download).id("download"))
                .child(Input::new(&self.limits.upload).id("upload"))
        }
    }

    fn open(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Host>) {
        open_window(cx, |window, cx| {
            cx.new(|cx| {
                let limits = SpeedLimits::new(window, cx);
                let subscriptions = limits.subscribe(window, cx, |host: &mut Host, direction, _| {
                    host.committed.push(direction);
                });
                Host {
                    limits,
                    committed: Vec::new(),
                    _subscriptions: subscriptions,
                }
            })
        })
    }

    /// Run `f` on the host inside its window, then render a frame.
    fn with_host<R>(
        cx: &mut TestAppContext,
        (handle, host): &(AnyWindowHandle, Entity<Host>),
        f: impl FnOnce(&mut Host, &mut Window, &mut Context<Host>) -> R,
    ) -> R {
        cx.update_window(*handle, |_, window, cx| {
            let result = host.update(cx, |host, cx| f(host, window, cx));
            window.render_frame(cx);
            result
        })
        .unwrap()
    }

    /// Drive the window directly: click, type and read what's shown.
    fn with_window<R>(
        cx: &mut TestAppContext,
        (handle, _): &(AnyWindowHandle, Entity<Host>),
        f: impl FnOnce(&mut Window, &mut gpui_kit::App) -> R,
    ) -> R {
        cx.update_window(*handle, |_, window, cx| {
            window.render_frame(cx);
            f(window, cx)
        })
        .unwrap()
    }

    /// What the `id` input shows.
    fn shown(cx: &mut TestAppContext, view: &(AnyWindowHandle, Entity<Host>), id: &'static str) -> String {
        with_window(cx, view, |window, _| {
            window.find(id).value().unwrap_or_default().to_owned()
        })
    }

    fn sync(cx: &mut TestAppContext, view: &(AnyWindowHandle, Entity<Host>), key: &str, download: i64, upload: i64) {
        with_host(cx, view, |host, window, cx| {
            host.limits.sync(key, Limits { download, upload }, window, cx);
        });
    }

    /// Click the `id` input and type `text` into it.
    fn type_into(cx: &mut TestAppContext, view: &(AnyWindowHandle, Entity<Host>), id: &'static str, text: &str) {
        with_window(cx, view, |window, cx| {
            window.click(id, cx);
            window.input(text, cx);
        });
    }

    /// Commit the download limit against `current`, answering with `succeeded`.
    /// Returns the bytes sent, if any, and the request's id.
    fn commit_download(
        cx: &mut TestAppContext,
        view: &(AnyWindowHandle, Entity<Host>),
        current: i64,
        runtime: &tokio::runtime::Runtime,
    ) -> Option<(u64, u64)> {
        let sent_bytes = Cell::new(None);
        let sent = with_host(cx, view, |host, _, cx| {
            let current = Limits {
                download: current,
                upload: 0,
            };
            let send = |bytes| {
                sent_bytes.set(Some(bytes));
                Some(runtime.spawn(async { true }))
            };
            host.limits
                .commit(Direction::Download, current, send, cx)
                .map(|sent| sent.id)
        });
        match (sent, sent_bytes.get()) {
            (Some(id), Some(bytes)) => Some((bytes, id)),
            (None, None) => None,
            mismatch => panic!("commit and send disagree: {mismatch:?}"),
        }
    }

    fn is_sending(cx: &mut TestAppContext, view: &(AnyWindowHandle, Entity<Host>), direction: Direction) -> bool {
        with_host(cx, view, |host, _, _| host.limits.is_sending(direction))
    }

    #[gpui_kit::test]
    fn shows_a_keys_limits_in_kib(cx: &mut TestAppContext) {
        let view = open(cx);
        sync(cx, &view, "a", 512_000, 0);

        assert_eq!(shown(cx, &view, "download"), "500");
        assert_eq!(shown(cx, &view, "upload"), "");
    }

    #[gpui_kit::test]
    fn follows_limits_changed_elsewhere(cx: &mut TestAppContext) {
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        sync(cx, &view, "a", 1024, 2048);

        assert_eq!(shown(cx, &view, "download"), "1");
        assert_eq!(shown(cx, &view, "upload"), "2");
    }

    #[gpui_kit::test]
    fn keeps_what_is_being_typed_until_the_key_changes(cx: &mut TestAppContext) {
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "123");

        sync(cx, &view, "a", 512_000, 0);
        assert_eq!(shown(cx, &view, "download"), "123");

        sync(cx, &view, "b", 512_000, 0);
        assert_eq!(shown(cx, &view, "download"), "500");
    }

    #[gpui_kit::test]
    fn only_accepts_digits(cx: &mut TestAppContext) {
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "1a2.3");

        assert_eq!(shown(cx, &view, "download"), "123");
    }

    #[gpui_kit::test]
    fn asks_to_commit_on_enter_and_on_leaving(cx: &mut TestAppContext) {
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);

        type_into(cx, &view, "download", "1");
        with_window(cx, &view, |window, cx| window.press("enter", cx));
        assert_eq!(
            with_host(cx, &view, |host, _, _| host.committed.clone()),
            [Direction::Download]
        );

        // Moving to the upload input leaves the download one.
        with_window(cx, &view, |window, cx| window.click("upload", cx));
        let committed = with_host(cx, &view, |host, _, _| host.committed.clone());
        assert_eq!(committed, [Direction::Download, Direction::Download]);
    }

    #[gpui_kit::test]
    fn sends_the_typed_limit_in_bytes_once(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "100");

        let (bytes, _) = commit_download(cx, &view, 0, &runtime).unwrap();
        assert_eq!(bytes, 102_400);
        assert!(is_sending(cx, &view, Direction::Download));
        assert!(!is_sending(cx, &view, Direction::Upload));

        // Nothing more is sent while the first request is in flight.
        assert_eq!(commit_download(cx, &view, 0, &runtime), None);
    }

    #[gpui_kit::test]
    fn sends_nothing_when_the_limit_is_unchanged(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 102_400, 0);

        assert_eq!(commit_download(cx, &view, 102_400, &runtime), None);
    }

    #[gpui_kit::test]
    fn sends_nothing_before_the_first_sync(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        type_into(cx, &view, "download", "100");

        assert_eq!(commit_download(cx, &view, 0, &runtime), None);
    }

    #[gpui_kit::test]
    fn keeps_a_sent_limit_until_qbittorrent_reports_it(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "100");
        let (_, id) = commit_download(cx, &view, 0, &runtime).unwrap();
        with_host(cx, &view, |host, _, _| {
            host.limits.finish(Direction::Download, id, true);
        });
        assert!(!is_sending(cx, &view, Direction::Download));

        // Stop editing, so refreshes may change the input again.
        with_window(cx, &view, |window, cx| window.click("upload", cx));

        // A refresh from before qBittorrent applied it doesn't flash the old limit.
        sync(cx, &view, "a", 0, 0);
        assert_eq!(shown(cx, &view, "download"), "100");

        // Once reported, later changes from elsewhere show again.
        sync(cx, &view, "a", 102_400, 0);
        sync(cx, &view, "a", 1024, 0);
        assert_eq!(shown(cx, &view, "download"), "1");
    }

    #[gpui_kit::test]
    fn puts_the_actual_limit_back_when_sending_fails(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "100");
        let (_, id) = commit_download(cx, &view, 0, &runtime).unwrap();
        with_host(cx, &view, |host, _, _| {
            host.limits.finish(Direction::Download, id, false);
        });
        with_window(cx, &view, |window, cx| window.click("upload", cx));

        sync(cx, &view, "a", 0, 0);
        assert_eq!(shown(cx, &view, "download"), "");
    }

    #[gpui_kit::test]
    fn ignores_an_old_request_finishing(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "100");
        let (_, id) = commit_download(cx, &view, 0, &runtime).unwrap();

        with_host(cx, &view, |host, _, _| {
            host.limits.finish(Direction::Download, id.wrapping_add(1), true);
        });
        assert!(is_sending(cx, &view, Direction::Download));
    }

    #[gpui_kit::test]
    fn only_disables_the_key_being_sent(cx: &mut TestAppContext) {
        let runtime = runtime();
        let view = open(cx);
        sync(cx, &view, "a", 0, 0);
        type_into(cx, &view, "download", "100");
        commit_download(cx, &view, 0, &runtime).unwrap();

        sync(cx, &view, "b", 0, 0);
        assert!(!is_sending(cx, &view, Direction::Download));
        sync(cx, &view, "a", 0, 0);
        assert!(is_sending(cx, &view, Direction::Download));
    }
}
