use gpui_kit::{
    App, Entity, IntoElement, SharedString,
    assets::IconName,
    base::{StyledExt, h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        input::{Input, InputState},
        progress::Progress,
        scroll::ScrollableElement as _,
        table::TableState,
    },
    div,
    prelude::*,
    px,
};
use qbittorrent::Torrent;

use crate::{
    format::{format_ago, format_bytes, format_eta, format_rate},
    speed_limits::{Direction, SpeedLimits},
    torrent_table::TorrentTable,
};

/// Width of the details panel.
const PANEL_WIDTH: f32 = 320.0;

/// A side panel with everything about `torrent`. The close button clears the table's
/// selection, which hides the panel.
pub fn details_panel(
    torrent: &Torrent,
    table: &Entity<TableState<TorrentTable>>,
    limits: &SpeedLimits,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let percent = torrent.progress * 100.0;
    let delete = table.read(cx).delegate().delete_request(torrent);
    let can_send = table.read(cx).delegate().can_send();
    let table = table.clone();

    let completed = if torrent.completion_on > 0 {
        format_ago(torrent.completion_on)
    } else {
        "—".to_owned()
    };

    // The border sits on a wrapper that clips, and the content scrolls inside it,
    // so scrolled content can't paint over the top and bottom border.
    let content = v_flex()
        .id("details-panel")
        .size_full()
        .gap_4()
        .p_3()
        .overflow_y_scrollbar()
        // Keyed by torrent, so selecting another one starts it scrolled to the top.
        .id(SharedString::from(format!("details-scroll-{}", torrent.hash)))
        .child(
            h_flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(div().flex_1().min_w_0().font_semibold().child(torrent.name.clone()))
                .child(
                    Button::new("delete-torrent")
                        .icon(IconName::Trash)
                        .danger()
                        .xsmall()
                        .tooltip("Delete")
                        .disabled(delete.is_none())
                        .on_click(move |_, window, cx| {
                            if let Some(delete) = delete.clone() {
                                delete.confirm(window, cx);
                            }
                        }),
                )
                .child(
                    Button::new("close-details")
                        .icon(IconName::Close)
                        .ghost()
                        .xsmall()
                        .tooltip("Close")
                        .on_click(move |_, _, cx| {
                            table.update(cx, TableState::clear_selection);
                        }),
                ),
        )
        .child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .justify_between()
                        .text_sm()
                        .child(torrent.state.to_string())
                        .child(format!("{percent:.1}%")),
                )
                .child(Progress::new("details-progress").value(percent_f32(percent)).small()),
        )
        .child(section(
            "Transfer",
            [
                ("Downloaded", format_bytes(torrent.downloaded)),
                ("Uploaded", format_bytes(torrent.uploaded)),
                ("Ratio", format!("{:.2}", torrent.ratio)),
                ("Download speed", format_rate(torrent.dlspeed)),
                ("Upload speed", format_rate(torrent.upspeed)),
                ("Time left", format_eta(torrent.eta)),
                ("Seeds", torrent.num_seeds.to_string()),
                ("Peers", torrent.num_leechs.to_string()),
            ],
            cx,
        ))
        .child(limits_section(limits, can_send, cx))
        .child(section(
            "Info",
            [
                ("Size", format_bytes(torrent.size)),
                ("Added", format_ago(torrent.added_on)),
                ("Completed", completed),
                ("Category", or_dash(&torrent.category)),
                ("Save path", or_dash(&torrent.save_path)),
                ("Content path", or_dash(&torrent.content_path)),
                ("Tracker", or_dash(&torrent.tracker)),
                ("Hash", torrent.hash.clone()),
            ],
            cx,
        ));

    div()
        .w(px(PANEL_WIDTH))
        .flex_shrink_0()
        .h_full()
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
        .overflow_hidden()
        .child(content)
}

/// A titled group of label/value pairs. Values wrap, so long paths stay readable.
fn section<const N: usize>(title: &'static str, fields: [(&'static str, String); N], cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(title.to_uppercase()),
        )
        .children(fields.map(|(label, value)| {
            v_flex()
                .gap_0p5()
                .child(div().text_xs().text_color(theme.muted_foreground).child(label))
                .child(div().text_sm().child(SharedString::from(value)))
        }))
}

/// Inputs for the torrent's download and upload speed limits, applied on Enter or
/// when focus leaves them. Empty means unlimited. Each is disabled while its new
/// limit is being sent.
fn limits_section(limits: &SpeedLimits, enabled: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();

    let field = |id: &'static str, label: &'static str, input: &Entity<InputState>, direction: Direction| {
        v_flex()
            .gap_0p5()
            .child(div().text_xs().text_color(theme.muted_foreground).child(label))
            .child(
                Input::new(input)
                    .id(id)
                    .suffix(div().text_xs().text_color(theme.muted_foreground).child("KiB/s"))
                    .small()
                    .disabled(!enabled || limits.is_sending(direction)),
            )
    };

    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child("SPEED LIMITS"),
        )
        .child(field(
            "max-download",
            "Max download speed",
            &limits.download,
            Direction::Download,
        ))
        .child(field(
            "max-upload",
            "Max upload speed",
            &limits.upload,
            Direction::Upload,
        ))
}

/// `value`, or `—` when it's empty.
fn or_dash(value: &str) -> String {
    if value.is_empty() {
        "—".to_owned()
    } else {
        value.to_owned()
    }
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
mod ui_tests {
    use gpui_kit::{
        AnyWindowHandle, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        Subscription, TestAppContext, Window,
        component::table::TableState,
        div,
        test::{TestQueryExt as _, TestWindowExt as _},
    };
    use qbittorrent::Torrent;

    use super::details_panel;
    use crate::{
        speed_limits::{Limits, SpeedLimits},
        test_support::{MockQbittorrent, open_window, torrent},
        torrent_table::TorrentTable,
    };

    /// The details panel for the table's selected torrent, as the main window shows it.
    struct Host {
        table: Entity<TableState<TorrentTable>>,
        limits: SpeedLimits,
        _observe_table: Subscription,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let table = self.table.read(cx);
            let torrent = table
                .selected_row()
                .and_then(|row_ix| table.delegate().torrent_at(row_ix))
                .cloned();
            div().size_full().children(torrent.map(|torrent| {
                self.limits.sync(&torrent.hash, Limits::from(&torrent), window, cx);
                details_panel(&torrent, &self.table, &self.limits, cx).into_any_element()
            }))
        }
    }

    type View = (AnyWindowHandle, Entity<Host>);

    /// Show `torrents`, logged in to `qbittorrent` if given, with the first selected.
    fn open(cx: &mut TestAppContext, torrents: Vec<Torrent>, qbittorrent: Option<&MockQbittorrent>) -> View {
        let view = open_window(cx, |window, cx| {
            cx.new(|cx| {
                let mut rows = TorrentTable::new();
                rows.set_torrents(torrents);
                if let Some(qbittorrent) = qbittorrent {
                    rows.set_client(qbittorrent.client.clone(), qbittorrent.runtime.handle().clone());
                }
                let table = cx.new(|cx| TableState::new(rows, window, cx));
                Host {
                    _observe_table: cx.observe(&table, |_, _, cx| cx.notify()),
                    table,
                    limits: SpeedLimits::new(window, cx),
                }
            })
        });
        select(cx, &view, 0);
        view
    }

    fn select(cx: &TestAppContext, (_, host): &View, row_ix: usize) {
        cx.update(|cx| {
            let table = host.read(cx).table.clone();
            table.update(cx, |table, cx| table.set_selected_row(row_ix, cx));
        });
    }

    fn selected(cx: &TestAppContext, (_, host): &View) -> Option<usize> {
        cx.update(|cx| host.read(cx).table.read(cx).selected_row())
    }

    /// Render a frame, then run `f` on the window.
    fn with_window<R>(cx: &mut TestAppContext, (handle, _): &View, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        cx.run_until_parked();
        cx.update_window(*handle, |_, window, cx| {
            window.render_frame(cx);
            f(window, cx)
        })
        .unwrap()
    }

    fn disabled(cx: &mut TestAppContext, view: &View, id: &'static str) -> bool {
        with_window(cx, view, |window, _| window.find(id).disabled().unwrap_or(false))
    }

    /// Whether clicking the `id` input lets the user type in it.
    fn editable(cx: &mut TestAppContext, view: &View, id: &'static str) -> bool {
        with_window(cx, view, |window, cx| window.click(id, cx));
        with_window(cx, view, |window, _| window.find(id).focused().unwrap_or(false))
    }

    /// Click the element labelled `label`, like a dialog's buttons.
    fn click_label(window: &mut Window, label: &str, cx: &mut App) {
        let id = window
            .find_by_label(label)
            .path()
            .last()
            .cloned()
            .expect("an identified element");
        window.click(id, cx);
    }

    fn mixed() -> Vec<Torrent> {
        let mut limited = torrent("a", "Ubuntu", "downloading");
        limited.dl_limit = 512_000;
        limited.up_limit = 1024;
        vec![limited, torrent("b", "Debian", "uploading")]
    }

    #[gpui_kit::test]
    fn shows_the_selected_torrent(cx: &mut TestAppContext) {
        let view = open(cx, mixed(), None);
        assert!(with_window(cx, &view, |window, _| window
            .try_find("close-details")
            .is_some()));
    }

    #[gpui_kit::test]
    fn is_hidden_without_a_selection(cx: &mut TestAppContext) {
        let view = open(cx, mixed(), None);
        cx.update(|cx| {
            let table = view.1.read(cx).table.clone();
            table.update(cx, TableState::clear_selection);
        });
        assert!(with_window(cx, &view, |window, _| window
            .try_find("close-details")
            .is_none()));
    }

    #[gpui_kit::test]
    fn closes_by_clearing_the_selection(cx: &mut TestAppContext) {
        let view = open(cx, mixed(), None);
        with_window(cx, &view, |window, cx| window.click("close-details", cx));

        assert_eq!(selected(cx, &view), None);
        assert!(with_window(cx, &view, |window, _| window
            .try_find("close-details")
            .is_none()));
    }

    #[gpui_kit::test]
    fn shows_the_torrents_speed_limits(cx: &mut TestAppContext) {
        let view = open(cx, mixed(), None);
        let shown = |cx: &mut TestAppContext| {
            with_window(cx, &view, |window, _| {
                let value = |id: &'static str| window.find(id).value().unwrap_or_default().to_owned();
                (value("max-download"), value("max-upload"))
            })
        };
        assert_eq!(shown(cx), ("500".to_owned(), "1".to_owned()));

        select(cx, &view, 1);
        assert_eq!(shown(cx), (String::new(), String::new()));
    }

    #[gpui_kit::test]
    fn disables_changes_until_logged_in(cx: &mut TestAppContext) {
        let view = open(cx, mixed(), None);
        assert!(disabled(cx, &view, "delete-torrent"));
        assert!(!editable(cx, &view, "max-download"));
        assert!(!editable(cx, &view, "max-upload"));
    }

    #[gpui_kit::test]
    fn enables_changes_once_logged_in(cx: &mut TestAppContext) {
        let qbittorrent = MockQbittorrent::start();
        let view = open(cx, mixed(), Some(&qbittorrent));
        assert!(!disabled(cx, &view, "delete-torrent"));
        assert!(editable(cx, &view, "max-download"));
        assert!(editable(cx, &view, "max-upload"));
    }

    #[gpui_kit::test]
    fn deletes_the_torrent_once_confirmed(cx: &mut TestAppContext) {
        let qbittorrent = MockQbittorrent::start();
        let view = open(cx, mixed(), Some(&qbittorrent));

        with_window(cx, &view, |window, cx| window.click("delete-torrent", cx));
        with_window(cx, &view, |window, cx| click_label(window, "Delete", cx));

        let request = qbittorrent.wait_for("/api/v2/torrents/delete");
        assert_eq!(String::from_utf8_lossy(&request.body), "hashes=a&deleteFiles=false");
    }

    #[gpui_kit::test]
    fn deletes_the_files_too_if_asked(cx: &mut TestAppContext) {
        let qbittorrent = MockQbittorrent::start();
        let view = open(cx, mixed(), Some(&qbittorrent));

        with_window(cx, &view, |window, cx| window.click("delete-torrent", cx));
        with_window(cx, &view, |window, cx| window.click("delete-files", cx));
        with_window(cx, &view, |window, cx| click_label(window, "Delete", cx));

        let request = qbittorrent.wait_for("/api/v2/torrents/delete");
        assert_eq!(String::from_utf8_lossy(&request.body), "hashes=a&deleteFiles=true");
    }

    #[gpui_kit::test]
    fn deletes_nothing_when_cancelled(cx: &mut TestAppContext) {
        let qbittorrent = MockQbittorrent::start();
        let view = open(cx, mixed(), Some(&qbittorrent));

        with_window(cx, &view, |window, cx| window.click("delete-torrent", cx));
        with_window(cx, &view, |window, cx| click_label(window, "Cancel", cx));

        cx.run_until_parked();
        assert_eq!(qbittorrent.requests().len(), 0);
    }
}
