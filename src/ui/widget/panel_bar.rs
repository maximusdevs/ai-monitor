//! Waybar rendering of the shared panel model (`--panel`).
//!
//! The same `panel` object every desktop frontend draws, as one Waybar
//! module. Waybar re-runs the exec on its own `interval`, so the carousel is
//! stateless: the visible item is `now / carousel_interval`, shifted by a
//! persisted offset that `--panel-next` / `--panel-prev` (the scroll wheel)
//! move. A Waybar tooltip takes no clicks, so the pager's ◀ ▶ are the wheel
//! too, the documented fallback for `hover_mode = "pager"`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::Result;
use crate::pango::escape;
use crate::waybar::{Class, WaybarOutput};

const OFFSET_FILE: &str = "panel_offset";

fn str_of<'v>(v: &'v Value, key: &str) -> &'v str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn items(panel: &Value) -> &[Value] {
    panel
        .get("items")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn wrap(position: i64, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    position.rem_euclid(n as i64) as usize
}

/// The carousel/pager position: time-driven steps plus the scroll offset.
pub fn position(panel: &Value, now_secs: u64, offset: i64) -> i64 {
    let interval = panel
        .get("interval")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .max(1);
    let ticks = if str_of(panel, "mode") == "carousel" {
        now_secs / interval
    } else {
        0
    };
    ticks as i64 + offset
}

/// Account items print their account name first, unless `show_account_name`
/// is off, which leaves only provider names and usage.
fn names_accounts(panel: &Value) -> bool {
    str_of(panel, "unit") == "account"
        && panel.get("show_account_name").and_then(Value::as_bool) != Some(false)
}

fn class_of(severity: &str) -> Class {
    match severity {
        "critical" => Class::Critical,
        "high" => Class::High,
        "mid" | "warning" | "medium" => Class::Mid,
        _ => Class::Low,
    }
}

fn metrics(provider: &Value) -> &[Value] {
    provider
        .get("metrics")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Antigravity's Claude/GPT pool, shown only with `ui.show_extra_models`.
fn is_extra(provider: &Value, metric: &Value) -> bool {
    let label = str_of(metric, "label").to_ascii_lowercase();
    str_of(provider, "id") == "antigravity" && (label.contains("claude") || label.contains("gpt"))
}

/// The metrics the bar shows for a provider.
fn shown_metrics(provider: &Value, show_extra: bool) -> impl Iterator<Item = &Value> {
    metrics(provider)
        .iter()
        .filter(move |m| show_extra || !is_extra(provider, m))
}

fn is_weekly(metric: &Value) -> bool {
    metric
        .get("window_secs")
        .and_then(Value::as_u64)
        .is_some_and(|s| (500_000..=700_000).contains(&s))
}

/// `Gemini 10% · 7d 2%` for one provider; `⚠` when it failed.
fn provider_text(provider: &Value, show_extra: bool) -> String {
    let name = escape(str_of(provider, "name"));
    if provider.get("error").is_some_and(|e| !e.is_null()) {
        return format!("{name} ⚠");
    }
    let values: Vec<String> = shown_metrics(provider, show_extra)
        .map(|m| {
            let v = escape(str_of(m, "value"));
            if is_weekly(m) { format!("7d {v}") } else { v }
        })
        .filter(|v| !v.is_empty())
        .collect();
    if values.is_empty() {
        name
    } else {
        format!("{name} {}", values.join(" · "))
    }
}

fn item_text(item: &Value, account_unit: bool, show_extra: bool) -> String {
    let providers: Vec<String> = item
        .get("providers")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|p| provider_text(p, show_extra))
        .collect();
    let body = providers.join("  ");
    if account_unit {
        format!("<b>{}</b> {body}", escape(str_of(item, "title")))
    } else {
        body
    }
}

fn item_block(item: &Value, show_extra: bool) -> Vec<String> {
    let mut lines = vec![format!("<b>{}</b>", escape(str_of(item, "title")))];
    for provider in item
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        lines.push(format!("  {}", escape(str_of(provider, "name"))));
        if let Some(err) = provider.get("error").and_then(Value::as_str) {
            lines.push(format!("    ⚠ {}", escape(err)));
        }
        for m in shown_metrics(provider, show_extra) {
            lines.push(format!(
                "    {:<18} {}",
                escape(str_of(m, "label")),
                escape(str_of(m, "value"))
            ));
        }
    }
    lines
}

/// Render the panel for Waybar. Pure: the clock and the scroll offset are
/// inputs, so the carousel and the pager are testable without a clock.
pub fn render(panel: &Value, now_secs: u64, offset: i64) -> WaybarOutput {
    let all = items(panel);
    if all.is_empty() {
        return WaybarOutput {
            text: "ai: –".into(),
            tooltip: "No provider reported usage.".into(),
            class: Class::Low,
        };
    }
    let account_unit = names_accounts(panel);
    let show_extra = panel.get("show_extra_models").and_then(Value::as_bool) == Some(true);
    let pos = position(panel, now_secs, offset);
    let count = panel
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(3)
        .max(1) as usize;
    let visible: Vec<&Value> = if str_of(panel, "mode") == "carousel" {
        vec![&all[wrap(pos, all.len())]]
    } else {
        all.iter().take(count).collect()
    };

    let text = visible
        .iter()
        .map(|item| item_text(item, account_unit, show_extra))
        .collect::<Vec<_>>()
        .join("  │  ");

    let class = visible
        .iter()
        .flat_map(|item| {
            item.get("providers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|p| shown_metrics(p, show_extra))
        .map(|m| class_of(str_of(m, "severity")))
        .max_by_key(|c| *c as u8)
        .unwrap_or(Class::Low);

    let tooltip = if str_of(panel, "hover") == "pager" {
        let at = wrap(pos, all.len());
        let mut lines = vec![format!(
            "◀ {} / {} ▶  (scroll to change)",
            at + 1,
            all.len()
        )];
        lines.extend(item_block(&all[at], show_extra));
        lines.join("\n")
    } else {
        all.iter()
            .map(|item| item_block(item, show_extra).join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    WaybarOutput {
        text,
        tooltip,
        class,
    }
}

/// Pango markup back to plain text: drop the tags, then undo [`escape`].
fn plain(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut in_tag = false;
    for c in markup.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The tray icon's hover text: the panel's tooltip as plain text, capped at
/// `max_chars` (Windows allows 127). Blocks keep one line per item; a pager
/// shows its current item. Empty when the report carries no panel items.
pub fn tray_tooltip(panel: &Value, now_secs: u64, max_chars: usize) -> String {
    if items(panel).is_empty() {
        return String::new();
    }
    let out = render(panel, now_secs, 0);
    // One summary line per item reads better in a tiny native tooltip than the
    // indented blocks: the item text is already that summary.
    let text = if str_of(panel, "hover") == "pager" {
        plain(&out.tooltip)
    } else {
        let account_unit = names_accounts(panel);
        let show_extra = panel.get("show_extra_models").and_then(Value::as_bool) == Some(true);
        items(panel)
            .iter()
            .map(|item| plain(&item_text(item, account_unit, show_extra)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn offset_path() -> Result<PathBuf> {
    Ok(crate::cache::xdg_cache_dir()?
        .join("ai-monitor")
        .join(OFFSET_FILE))
}

/// The persisted scroll offset; 0 when absent or unreadable.
pub fn read_offset_at(path: &Path) -> i64 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Move the scroll offset by `delta`, atomically.
pub fn step_offset_at(path: &Path, delta: i64) -> Result<i64> {
    let next = read_offset_at(path).wrapping_add(delta);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    crate::cache::atomic_write(path, next.to_string().as_bytes())?;
    Ok(next)
}

/// `--panel`: the whole panel as one module.
pub async fn output() -> Result<WaybarOutput> {
    let json = crate::report::collect_json()
        .await
        .map_err(crate::error::AppError::Other)?;
    let report: Value = serde_json::from_str(&json)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let offset = offset_path().map(|p| read_offset_at(&p)).unwrap_or(0);
    Ok(render(
        report.get("panel").unwrap_or(&Value::Null),
        now,
        offset,
    ))
}

/// `--panel-next` / `--panel-prev`: step, then have Waybar re-render.
pub fn step(delta: i64) -> i32 {
    if let Ok(path) = offset_path()
        && step_offset_at(&path, delta).is_ok()
    {
        crate::waybar::request_refresh();
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn panel(mode: &str, hover: &str) -> Value {
        let p = |name: &str, pct: u16, sev: &str| {
            json!({"id": name.to_lowercase(), "name": name, "metrics": [
                {"label": "5h", "value": format!("{pct}%"), "severity": sev},
                {"label": "Weekly", "value": "3%", "severity": "low", "window_secs": 604_800}]})
        };
        json!({
            "mode": mode, "unit": "account", "count": 2, "interval": 10, "hover": hover,
            "items": [
                {"key": "a", "title": "alice", "providers": [p("Claude", 17, "low"), p("Gemini", 10, "low")]},
                {"key": "b", "title": "bob", "providers": [p("Codex", 91, "critical")]},
                {"key": "c", "title": "<carol>", "providers": [p("Gemini", 0, "low")]},
            ]
        })
    }

    #[test]
    fn expanded_shows_count_items_side_by_side() {
        let out = render(&panel("expanded", "blocks"), 12_345, 0);
        assert_eq!(
            out.text,
            "<b>alice</b> Claude 17% · 7d 3%  Gemini 10% · 7d 3%  │  <b>bob</b> Codex 91% · 7d 3%"
        );
        assert_eq!(out.class, Class::Critical, "worst visible window");
        assert!(
            out.tooltip.contains("&lt;carol&gt;"),
            "blocks list every item, escaped"
        );
    }

    #[test]
    fn carousel_follows_the_clock_and_the_scroll_offset() {
        let p = panel("carousel", "blocks");
        assert!(render(&p, 0, 0).text.starts_with("<b>alice</b>"));
        assert!(
            render(&p, 10, 0).text.starts_with("<b>bob</b>"),
            "one step per interval"
        );
        assert!(
            render(&p, 10, 1).text.starts_with("<b>&lt;carol&gt;</b>"),
            "scroll shifts it"
        );
        assert!(
            render(&p, 0, -1).text.starts_with("<b>&lt;carol&gt;</b>"),
            "wraps backwards"
        );
    }

    #[test]
    fn pager_tooltip_shows_one_item_with_its_position() {
        let out = render(&panel("expanded", "pager"), 0, 1);
        assert!(out.tooltip.starts_with("◀ 2 / 3 ▶"));
        assert!(out.tooltip.contains("bob") && !out.tooltip.contains("alice"));
    }

    #[test]
    fn extra_pools_follow_show_extra_models() {
        let mut p = json!({"mode": "expanded", "unit": "provider", "count": 1, "items": [
            {"key": "antigravity", "title": "Antigravity", "providers": [{"id": "antigravity",
                "name": "Antigravity", "metrics": [
                    {"label": "Gemini", "value": "10%", "severity": "low"},
                    {"label": "Claude & GPT OSS", "value": "99%", "severity": "critical"}]}]}]});
        let out = render(&p, 0, 0);
        assert_eq!(out.text, "Antigravity 10%");
        assert_eq!(
            out.class,
            Class::Low,
            "a hidden pool does not colour the bar"
        );
        p["show_extra_models"] = json!(true);
        assert_eq!(render(&p, 0, 0).text, "Antigravity 10% · 99%");
    }

    #[test]
    fn tray_tooltip_is_plain_and_bounded() {
        let p = panel("expanded", "blocks");
        let tip = tray_tooltip(&p, 0, 127);
        assert!(tip.starts_with("alice Claude 17% · 7d 3%"));
        assert!(
            tip.contains("<carol> Gemini"),
            "entities undone for a plain-text tooltip"
        );
        assert!(!tip.contains("<b>"));
        let short = tray_tooltip(&p, 0, 20);
        assert_eq!(short.chars().count(), 20);
        assert!(short.ends_with('…'));
        assert!(tray_tooltip(&panel("expanded", "pager"), 0, 127).starts_with("◀ 1 / 3 ▶"));
        assert_eq!(tray_tooltip(&Value::Null, 0, 127), "");
    }

    #[test]
    fn account_names_can_be_left_off() {
        let mut p = panel("expanded", "blocks");
        p["show_account_name"] = json!(false);
        let out = render(&p, 0, 0);
        assert_eq!(
            out.text,
            "Claude 17% · 7d 3%  Gemini 10% · 7d 3%  │  Codex 91% · 7d 3%"
        );
        assert!(!tray_tooltip(&p, 0, 127).contains("alice"));
    }

    #[test]
    fn no_items_is_a_quiet_placeholder() {
        let out = render(&Value::Null, 0, 0);
        assert_eq!(out.text, "ai: –");
    }

    #[test]
    fn offset_steps_persist_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("panel_offset");
        assert_eq!(read_offset_at(&path), 0);
        assert_eq!(step_offset_at(&path, 1).unwrap(), 1);
        assert_eq!(step_offset_at(&path, -3).unwrap(), -2);
        assert_eq!(read_offset_at(&path), -2);
    }
}
