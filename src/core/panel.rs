//! The panel model every frontend draws.
//!
//! `usage --json` carries a `panel` object built here from `[display]`: the
//! layout (`expanded` or `carousel`), the tooltip style, and the items in
//! display order, each already holding its providers and metrics. Frontends
//! only draw it: none re-derives the order or the item set, which is how KDE,
//! GNOME, Omarchy, Waybar, the tray and the macOS menu stay identical.

use serde::Serialize;
use serde_json::Value;

use crate::config::{BarMode, BarUnit, DisplayConfig, HoverMode, format_account_label};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Panel {
    pub mode: BarMode,
    /// The unit actually used. `account` falls back to `provider` when
    /// multi-account is off or no account has a provider to show.
    pub unit: BarUnit,
    /// Items side by side in `expanded` mode; the first `count` of `items`.
    pub count: usize,
    /// Seconds between carousel steps.
    pub interval: u64,
    pub hover: HoverMode,
    /// `[display] show_account_name`: whether an account item prints its
    /// account before its providers, or only provider names and usage.
    pub show_account_name: bool,
    /// `ui.show_full_email`, for frontends that print an account label that
    /// is not an item title (the popup's account cards, renewal notices).
    pub show_full_email: bool,
    /// `ui.show_extra_models`: Antigravity's Claude/GPT pool rows.
    pub show_extra_models: bool,
    /// `ui.refresh_interval`, seconds: how often a frontend re-runs `usage`.
    pub refresh_interval: u64,
    /// Every eligible item, in display order. The carousel cycles all of them.
    pub items: Vec<PanelItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PanelItem {
    /// Stable key: the provider id, or the account label.
    pub key: String,
    /// What the frontend prints for the item.
    pub title: String,
    /// The account label, for account items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub active: bool,
    /// Provider objects in the report's `accounts[].providers` shape.
    pub providers: Vec<Value>,
}

/// Everything [`build`] reads, so it stays a pure function of its inputs.
pub struct PanelInputs<'a> {
    pub display: &'a DisplayConfig,
    /// The vendor preferred first inside an item.
    pub primary: Option<&'a str>,
    pub show_full_email: bool,
    pub show_extra_models: bool,
    pub refresh_interval: u64,
    /// Account labels the user switched off for the panel.
    pub disabled_accounts: &'a [String],
    /// The active account's providers.
    pub providers: &'a [Value],
    /// The report's `accounts` array; `None` when multi-account is off.
    pub accounts: Option<&'a [Value]>,
}

pub fn build(inputs: &PanelInputs<'_>) -> Panel {
    let display = inputs.display;
    let account_items = match (display.bar_unit(), inputs.accounts) {
        (BarUnit::Account, Some(accounts)) => account_items(inputs, accounts),
        _ => Vec::new(),
    };
    let (unit, items) = if account_items.is_empty() {
        (BarUnit::Provider, provider_items(inputs))
    } else {
        (BarUnit::Account, account_items)
    };
    Panel {
        mode: display.bar_mode(),
        unit,
        count: display.bar_count(),
        interval: display.carousel_interval(),
        hover: display.hover_mode(),
        show_account_name: display.show_account_name(),
        show_full_email: inputs.show_full_email,
        show_extra_models: inputs.show_extra_models,
        refresh_interval: inputs.refresh_interval,
        items,
    }
}

fn str_field<'v>(value: &'v Value, key: &str) -> &'v str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// `primary` first, the rest in their original order.
fn primary_first(mut providers: Vec<Value>, primary: Option<&str>) -> Vec<Value> {
    if let Some(primary) = primary {
        providers.sort_by_key(|p| str_field(p, "id") != primary);
    }
    providers
}

fn provider_items(inputs: &PanelInputs<'_>) -> Vec<PanelItem> {
    primary_first(inputs.providers.to_vec(), inputs.primary)
        .into_iter()
        .map(|provider| PanelItem {
            key: str_field(&provider, "id").to_string(),
            title: str_field(&provider, "name").to_string(),
            account: None,
            active: true,
            providers: vec![provider],
        })
        .collect()
}

/// Accounts in use, in display order: the active one, then those with a
/// recently used provider. An account that is only logged in (no current
/// session for any provider) is monitored for renewals but never drawn, and
/// neither is a switched-off one or one with no provider.
fn account_items(inputs: &PanelInputs<'_>, accounts: &[Value]) -> Vec<PanelItem> {
    let mut ranked: Vec<(u8, PanelItem)> = accounts
        .iter()
        .filter_map(|account| {
            let label = str_field(account, "label");
            if label.is_empty() || inputs.disabled_accounts.iter().any(|d| d == label) {
                return None;
            }
            let all: Vec<Value> = account
                .get("providers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let is_recent = |p: &Value| p.get("recent").and_then(Value::as_bool) == Some(true);
            let has_recent = all.iter().any(is_recent);
            let providers = if inputs.display.recent_only() && has_recent {
                all.into_iter().filter(is_recent).collect()
            } else {
                all
            };
            if providers.is_empty() {
                return None;
            }
            let active = account.get("active").and_then(Value::as_bool) == Some(true);
            if !active && !has_recent {
                return None;
            }
            let rank = if active { 0 } else { 1 };
            Some((
                rank,
                PanelItem {
                    key: label.to_string(),
                    title: format_account_label(label, inputs.show_full_email),
                    account: Some(label.to_string()),
                    active,
                    providers: primary_first(providers, inputs.primary),
                },
            ))
        })
        .collect();
    // Stable, so report order breaks ties.
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(id: &str, recent: bool) -> Value {
        json!({"id": id, "name": id.to_uppercase(), "recent": recent, "metrics": []})
    }

    fn accounts() -> Vec<Value> {
        vec![
            json!({"label": "a@x.dev", "active": false, "providers": [provider("antigravity", false)]}),
            json!({"label": "b@x.dev", "active": false,
                   "providers": [provider("openai", true), provider("anthropic", false)]}),
            json!({"label": "c@x.dev", "active": true,
                   "providers": [provider("anthropic", true), provider("antigravity", true)]}),
            json!({"label": "d@x.dev", "active": false, "providers": [provider("antigravity", false)]}),
            json!({"label": "e@x.dev", "active": false, "providers": []}),
        ]
    }

    fn display(unit: BarUnit) -> DisplayConfig {
        DisplayConfig {
            bar_unit: Some(unit),
            ..DisplayConfig::default()
        }
    }

    fn inputs<'a>(
        display: &'a DisplayConfig,
        providers: &'a [Value],
        accounts: Option<&'a [Value]>,
    ) -> PanelInputs<'a> {
        PanelInputs {
            display,
            primary: Some("antigravity"),
            show_full_email: false,
            show_extra_models: false,
            refresh_interval: 300,
            disabled_accounts: &[],
            providers,
            accounts,
        }
    }

    fn keys(panel: &Panel) -> Vec<&str> {
        panel.items.iter().map(|i| i.key.as_str()).collect()
    }

    #[test]
    fn accounts_are_ordered_active_then_recent_then_the_rest() {
        let accts = accounts();
        let d = display(BarUnit::Account);
        let panel = build(&inputs(&d, &[], Some(&accts)));
        assert_eq!(panel.unit, BarUnit::Account);
        assert_eq!(
            keys(&panel),
            ["c@x.dev", "b@x.dev"],
            "logged-in-only accounts are not drawn"
        );
        assert_eq!(
            panel.items[0].title, "c",
            "show_full_email off prints the user part"
        );
        assert_eq!(str_field(&panel.items[0].providers[0], "id"), "antigravity");
    }

    #[test]
    fn recent_only_narrows_providers_but_never_hides_an_account() {
        let accts = accounts();
        let d = DisplayConfig {
            recent_only: Some(true),
            ..display(BarUnit::Account)
        };
        let panel = build(&inputs(&d, &[], Some(&accts)));
        assert_eq!(panel.items.len(), 2);
        assert_eq!(
            panel.items[1].providers.len(),
            1,
            "only the recent provider"
        );
    }

    #[test]
    fn account_name_defaults_on_and_can_be_turned_off() {
        let d = DisplayConfig::default();
        assert!(build(&inputs(&d, &[], None)).show_account_name);
        let d = DisplayConfig {
            show_account_name: Some(false),
            ..DisplayConfig::default()
        };
        let v = serde_json::to_value(build(&inputs(&d, &[], None))).unwrap();
        assert_eq!(v["show_account_name"], false);
    }

    #[test]
    fn switched_off_accounts_are_left_out() {
        let accts = accounts();
        let d = display(BarUnit::Account);
        let disabled = vec!["c@x.dev".to_string()];
        let panel = build(&PanelInputs {
            disabled_accounts: &disabled,
            ..inputs(&d, &[], Some(&accts))
        });
        assert_eq!(keys(&panel), ["b@x.dev"]);
    }

    #[test]
    fn account_unit_falls_back_to_providers_without_accounts() {
        let providers = vec![provider("openai", false), provider("antigravity", false)];
        let d = display(BarUnit::Account);
        let panel = build(&inputs(&d, &providers, None));
        assert_eq!(panel.unit, BarUnit::Provider);
        assert_eq!(keys(&panel), ["antigravity", "openai"], "primary first");
        assert_eq!(panel.items[0].title, "ANTIGRAVITY");
    }

    #[test]
    fn defaults_and_clamps_come_from_the_config() {
        let d = DisplayConfig {
            bar_count: Some(40),
            carousel_interval: Some(0),
            bar_mode: Some(BarMode::Carousel),
            hover_mode: Some(HoverMode::Pager),
            ..DisplayConfig::default()
        };
        let panel = build(&inputs(&d, &[], None));
        assert_eq!(panel.count, 6);
        assert_eq!(panel.interval, 2);
        assert_eq!(panel.mode, BarMode::Carousel);
        assert_eq!(panel.hover, HoverMode::Pager);
        assert_eq!(DisplayConfig::default().bar_count(), 3);
    }

    #[test]
    fn serialises_with_lowercase_enums() {
        let d = DisplayConfig::default();
        let v = serde_json::to_value(build(&inputs(&d, &[], None))).unwrap();
        assert_eq!(v["mode"], "expanded");
        assert_eq!(v["unit"], "provider");
        assert_eq!(v["hover"], "blocks");
    }

    #[test]
    fn display_section_parses_from_toml() {
        let cfg: crate::config::Config = toml::from_str(
            "[display]\nbar_mode = \"carousel\"\nbar_unit = \"account\"\nhover_mode = \"pager\"\nbar_count = 2\n",
        )
        .unwrap();
        assert_eq!(cfg.display.bar_mode(), BarMode::Carousel);
        assert_eq!(cfg.display.bar_unit(), BarUnit::Account);
        assert_eq!(cfg.display.hover_mode(), HoverMode::Pager);
        assert_eq!(cfg.display.bar_count(), 2);
    }
}
