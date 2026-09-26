//! Windows system-tray popover. On other OSes this binary exists so
//! `cargo build --all-targets` stays uniform, and exits with a short message.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    ai_monitor::migrate::migrate_legacy_dirs();
    std::process::exit(ai_monitor::tray::run());
}
