// Release builds on Windows are GUI apps, so they shouldn't open a console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    gui::run();
}
