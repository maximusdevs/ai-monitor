//! ai-monitor library — shared core for the Waybar widget and TUI binaries.
//!
//! The crate is organized by concern, not by binary, and the tree says which:
//! - `src/core/` — config, cache, the fetch outcome, the report and its
//!   `panel` model, accounts and the renewal monitor, and the low-level
//!   primitives (`countdown`, `pacing`, `pango`, `theme`)
//! - `src/providers/` — one directory per vendor behind the `vendor` trait
//! - `src/ui/` — the composition each binary draws: `widget` (Waybar),
//!   `tui`, `tray`
//!
//! Module paths are flat (`crate::cache`, `crate::anthropic`, `crate::tui`):
//! each `#[path]` below only says where the file lives, so moving a file
//! between folders never changes a path anyone imports.
//!
//! The binaries (`ai-monitor`, `ai-monitor-tui`, and on Windows
//! `ai-monitor-tray`) are thin: they parse CLI args, instantiate vendors,
//! and hand off to a renderer in this crate.

#[path = "core/account.rs"]
pub mod account;
#[path = "core/account_monitor.rs"]
pub mod account_monitor;
#[path = "core/account_store.rs"]
pub mod account_store;
#[path = "core/active.rs"]
pub mod active;
#[path = "providers/anthropic/mod.rs"]
pub mod anthropic;
#[path = "providers/anthropic_api/mod.rs"]
pub mod anthropic_api;
#[path = "providers/antigravity/mod.rs"]
pub mod antigravity;
#[path = "core/cache.rs"]
pub mod cache;
#[path = "core/catalog.rs"]
pub mod catalog;
#[path = "providers/claude_desktop/mod.rs"]
pub mod claude_desktop;
#[path = "providers/commandcode/mod.rs"]
pub mod commandcode;
#[path = "core/config.rs"]
pub mod config;
#[path = "core/context/mod.rs"]
pub mod context;
#[path = "providers/copilot/mod.rs"]
pub mod copilot;
#[path = "core/countdown.rs"]
pub mod countdown;
#[path = "providers/cursor/mod.rs"]
pub mod cursor;
#[path = "providers/custom/mod.rs"]
pub mod custom;
#[path = "providers/deepseek/mod.rs"]
pub mod deepseek;
#[path = "core/detect.rs"]
pub mod detect;
#[path = "core/display.rs"]
pub mod display;
#[path = "core/error.rs"]
pub mod error;
#[path = "core/format.rs"]
pub mod format;
#[path = "providers/grok/mod.rs"]
pub mod grok;
/// Source-scanning helpers for structural guard tests. Test-only.
#[cfg(test)]
#[path = "core/guard.rs"]
pub(crate) mod guard;
#[path = "core/jwt.rs"]
pub mod jwt;
#[path = "providers/kilo/mod.rs"]
pub mod kilo;
#[path = "providers/kimi/mod.rs"]
pub mod kimi;
#[path = "providers/kiro/mod.rs"]
pub mod kiro;
#[path = "core/migrate.rs"]
pub mod migrate;
#[path = "providers/minimax/mod.rs"]
pub mod minimax;
#[path = "core/monitor.rs"]
pub mod monitor;
#[path = "core/monitor_service.rs"]
pub mod monitor_service;
#[path = "providers/moonshot/mod.rs"]
pub mod moonshot;
#[path = "providers/nous/mod.rs"]
pub mod nous;
#[path = "providers/novita/mod.rs"]
pub mod novita;
#[path = "providers/ollama/mod.rs"]
pub mod ollama;
#[path = "providers/openai/mod.rs"]
pub mod openai;
#[path = "providers/opencode_go/mod.rs"]
pub mod opencode_go;
#[path = "providers/openrouter/mod.rs"]
pub mod openrouter;
#[path = "core/outcome.rs"]
pub mod outcome;
#[path = "core/pacing.rs"]
pub mod pacing;
#[path = "core/panel.rs"]
pub mod panel;
#[path = "core/pango.rs"]
pub mod pango;
#[path = "core/process.rs"]
pub mod process;
#[path = "core/provider_cli.rs"]
pub mod provider_cli;
#[path = "core/report.rs"]
pub mod report;
#[path = "core/safe_storage.rs"]
pub mod safe_storage;
#[path = "core/serde_helpers.rs"]
pub mod serde_helpers;
#[path = "providers/supergrok/mod.rs"]
pub mod supergrok;
#[path = "core/theme.rs"]
pub mod theme;
#[path = "core/tooltip.rs"]
pub mod tooltip;
#[path = "ui/tray/mod.rs"]
pub mod tray;
#[path = "ui/tui/mod.rs"]
pub mod tui;
#[path = "core/update.rs"]
pub mod update;
#[path = "core/usage.rs"]
pub mod usage;
#[path = "core/vendor.rs"]
pub mod vendor;
#[path = "core/waybar.rs"]
pub mod waybar;
#[path = "ui/widget/mod.rs"]
pub mod widget;
#[path = "providers/zai/mod.rs"]
pub mod zai;

pub use error::{AppError, Result};
