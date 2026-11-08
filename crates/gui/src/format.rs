use std::time::{SystemTime, UNIX_EPOCH};

/// Format a byte count with binary units, e.g. `1.4 GiB`.
pub fn format_bytes(bytes: u64) -> String {
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
pub fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        String::new()
    } else {
        format!("{}/s", format_bytes(bytes_per_sec))
    }
}

/// Format a transfer rate in bytes per second, e.g. `2.1 MiB/s`, showing `0 B/s` when idle.
pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

/// qBittorrent's ETA for "never" (100 days).
const ETA_INFINITE: i64 = 8_640_000;

/// Format a number of seconds as its two largest units, e.g. `2h 15m` or `45s`.
pub fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    let secs = seconds % 60;

    match (days, hours, minutes) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, _) => format!("{minutes}m {secs}s"),
        (0, _, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

/// Format a torrent's ETA, using `∞` for qBittorrent's "never".
pub fn format_eta(eta: i64) -> String {
    match u64::try_from(eta) {
        Ok(seconds) if eta < ETA_INFINITE => format_duration(seconds),
        _ => "∞".to_owned(),
    }
}

/// Format a Unix timestamp as how long ago it was, e.g. `3d 4h ago`, or `—` if unset.
pub fn format_ago(timestamp: i64) -> String {
    let Ok(then) = u64::try_from(timestamp) else {
        return "—".to_owned();
    };
    if then == 0 {
        return "—".to_owned();
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    format!("{} ago", format_duration(now.saturating_sub(then)))
}
