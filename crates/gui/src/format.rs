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
