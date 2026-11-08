use gpui_kit::{
    App, Entity, IntoElement, SharedString,
    assets::IconName,
    base::{StyledExt, h_flex, v_flex},
    component::{
        ActiveTheme as _, Sizable as _,
        button::{Button, ButtonVariants as _},
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
    torrent_table::TorrentTable,
};

/// Width of the details panel.
const PANEL_WIDTH: f32 = 320.0;

/// A side panel with everything about `torrent`. The close button clears the table's
/// selection, which hides the panel.
pub fn details_panel(
    torrent: &Torrent,
    table: &Entity<TableState<TorrentTable>>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let percent = torrent.progress * 100.0;
    let table = table.clone();

    let completed = if torrent.completion_on > 0 {
        format_ago(torrent.completion_on)
    } else {
        "—".to_owned()
    };

    v_flex()
        .id("details-panel")
        .w(px(PANEL_WIDTH))
        .flex_shrink_0()
        .h_full()
        .gap_4()
        .p_3()
        .border_1()
        .border_color(theme.border)
        .rounded(theme.radius)
        .overflow_y_scrollbar()
        .child(
            h_flex()
                .items_start()
                .justify_between()
                .gap_2()
                .child(div().flex_1().min_w_0().font_semibold().child(torrent.name.clone()))
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
        ))
}

/// A titled group of label/value pairs. Values wrap, so long paths stay readable.
fn section<const N: usize>(
    title: &'static str,
    fields: [(&'static str, String); N],
    cx: &App,
) -> impl IntoElement {
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
