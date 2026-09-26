//! Background quota renewal monitor and desktop notification daemon.
//!
//! Tracks quota reset timestamps across active and stored account snapshots.
//! When a 5-hour or weekly quota window expires (renews), it triggers a native
//! desktop notification (e.g. via `notify-send` on KDE Plasma / Wayland) so the
//! user knows the account is ready to be used again.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::account_store::{AccountSnapshot, ReportEntry, ReportSection, snapshots_path};
use crate::error::{AppError, Result};

/// How long a sent or dismissed key is remembered. A renewal older than
/// [`MAX_RENEWAL_AGE_HOURS`] is never actionable again, so anything past this
/// is dead weight in a file every refresh reads.
const KEY_RETENTION_DAYS: i64 = 7;

/// A renewal stays on screen for this long after its reset.
const MAX_RENEWAL_AGE_HOURS: i64 = 48;

/// Track notifications that have already been sent, and renewals the user
/// dismissed, so neither comes back.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotificationStore {
    #[serde(default)]
    pub sent: BTreeMap<String, DateTime<Utc>>,
    /// Renewal keys cleared by the user. The monitor keeps comparing the same
    /// stale snapshots until an account is refreshed, and without this every
    /// cleared renewal would be recorded again on the next tick.
    #[serde(default)]
    pub dismissed: BTreeMap<String, DateTime<Utc>>,
}

impl NotificationStore {
    /// Forget keys older than the retention window.
    pub fn prune(&mut self, now: DateTime<Utc>) {
        let keep = |at: &DateTime<Utc>| (now - *at).num_days() < KEY_RETENTION_DAYS;
        self.sent.retain(|_, at| keep(at));
        self.dismissed.retain(|_, at| keep(at));
    }
}

/// Represents an account quota window that has renewed (reset).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaRenewal {
    pub id: String,
    pub account_label: String,
    pub user: Option<String>,
    pub provider_id: String,
    pub provider_name: String,
    pub metric_label: String,
    pub window_type: String,
    pub reset_at: DateTime<Utc>,
    pub percent_before: u64,
}

/// Stable identity for one reset cycle.
///
/// Providers occasionally return the same reset boundary with a one-second
/// difference on consecutive requests.  Persisting their raw RFC3339 value
/// made Plasma re-notify after a restart.  A quota cycle is minute-granular
/// for notification purposes, while the account/provider/metric fields keep
/// distinct windows separate.
///
/// The window type is part of the key: Antigravity reports its 5h and weekly
/// pools under the same label, and without it one masked the other.
pub fn notification_key(renewal: &QuotaRenewal) -> String {
    renewal_key(
        &renewal.account_label,
        &renewal.provider_id,
        &renewal.metric_label,
        &renewal.window_type,
        renewal.reset_at,
    )
}

fn renewal_key(
    account: &str,
    provider: &str,
    label: &str,
    window_type: &str,
    reset_at: DateTime<Utc>,
) -> String {
    format!(
        "{account}:{provider}:{label}:{window_type}:{}",
        reset_at.timestamp().div_euclid(60)
    )
}

/// "5h" or "Semanal" for a metric window. The label is the fallback for a
/// vendor that omits the window length, so a weekly pool without one is not
/// announced as a 5h renewal.
pub fn window_type(window_secs: Option<u64>, label: &str) -> &'static str {
    match window_secs {
        Some(s) if (500_000..=700_000).contains(&s) => "Semanal",
        Some(s) if s <= 21_600 => "5h",
        _ => {
            let l = label.to_ascii_lowercase();
            if l.contains("week") || l.contains("7d") || l.contains("seman") {
                "Semanal"
            } else {
                "5h"
            }
        }
    }
}

/// One quota window inside an entry, identified by label, window length and
/// its position among windows sharing both. The label alone is not unique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct WindowKey<'a> {
    label: &'a str,
    window_secs: Option<u64>,
    nth: usize,
}

struct Window<'a> {
    key: WindowKey<'a>,
    percent: u16,
    reset_at: Option<DateTime<Utc>>,
}

fn windows(entry: &ReportEntry) -> Vec<Window<'_>> {
    let mut seen: BTreeMap<(&str, Option<u64>), usize> = BTreeMap::new();
    entry
        .sections
        .iter()
        .filter_map(|section| match section {
            ReportSection::Metric {
                label,
                percent,
                reset_at,
                window_secs,
                ..
            } => {
                let nth = seen.entry((label.as_str(), *window_secs)).or_default();
                let key = WindowKey {
                    label,
                    window_secs: *window_secs,
                    nth: *nth,
                };
                *nth += 1;
                Some(Window {
                    key,
                    percent: *percent,
                    reset_at: *reset_at,
                })
            }
            _ => None,
        })
        .collect()
}

fn renewal_for(
    account_label: &str,
    user: Option<&str>,
    entry: &ReportEntry,
    window: &Window<'_>,
    reset_at: DateTime<Utc>,
) -> QuotaRenewal {
    let window_type = window_type(window.key.window_secs, window.key.label).to_string();
    QuotaRenewal {
        id: renewal_key(
            account_label,
            &entry.id,
            window.key.label,
            &window_type,
            reset_at,
        ),
        account_label: account_label.to_string(),
        user: user.map(str::to_string),
        provider_id: entry.id.clone(),
        provider_name: entry.display_name.clone(),
        metric_label: window.key.label.to_string(),
        window_type,
        reset_at,
        percent_before: u64::from(window.percent),
    }
}

/// The one renewal rule, shared by the monitor and the snapshot recorder.
///
/// A window renewed when it had recorded use (>= 1%) and its reset moment has
/// passed. A provider rollover, a drop to 0% and a reset that fell while the
/// machine slept are all this same case, so none needs its own branch. The
/// window must still exist in `after`: an entry that came back as an error
/// says nothing about its windows.
pub fn entry_renewals(
    account_label: &str,
    user: Option<&str>,
    before: &ReportEntry,
    after: &ReportEntry,
    now: DateTime<Utc>,
) -> Vec<QuotaRenewal> {
    let current: Vec<WindowKey<'_>> = windows(after).into_iter().map(|w| w.key).collect();
    windows(before)
        .iter()
        .filter(|w| w.percent >= 1 && current.contains(&w.key))
        .filter_map(|w| match w.reset_at {
            Some(r) if r <= now => Some(renewal_for(account_label, user, after, w, r)),
            _ => None,
        })
        .collect()
}

/// Windows in a snapshot whose reset has passed with use recorded. Used by the
/// administrative [`check_resets_with`] path, which has no earlier state.
pub fn expired_windows(snapshot: &AccountSnapshot, now: DateTime<Utc>) -> Vec<QuotaRenewal> {
    snapshot
        .entries
        .iter()
        .flat_map(|entry| {
            entry_renewals(
                &snapshot.account_label,
                snapshot.user.as_deref(),
                entry,
                entry,
                now,
            )
        })
        .collect()
}

/// A renewal becomes user-visible only after its previous cycle expired and
/// the account actually consumed quota in that cycle.  Keeping this predicate
/// at the persistence boundary prevents stale or malformed producers from
/// filling the Plasmoid with zero-usage events.
fn is_actionable_renewal(renewal: &QuotaRenewal, now: DateTime<Utc>) -> bool {
    renewal.percent_before >= 1
        && now >= renewal.reset_at
        && (now - renewal.reset_at).num_hours() <= MAX_RENEWAL_AGE_HOURS
}

/// Path to the persisted notifications-sent cache file.
pub fn notifications_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| AppError::Other("could not resolve XDG cache dir".into()))?;
    Ok(base
        .cache_dir()
        .join("ai-monitor")
        .join("notifications_sent.json"))
}

/// Path to the simulated test renewals file.
pub fn test_renewals_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| AppError::Other("could not resolve XDG cache dir".into()))?;
    Ok(base
        .cache_dir()
        .join("ai-monitor")
        .join("test_renewals.json"))
}

/// Simulate a test quota renewal notice for the widget.
pub fn simulate_test_renewal() -> Result<()> {
    let path = test_renewals_path()?;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let now = Utc::now();
    let items = vec![
        QuotaRenewal {
            id: format!("test:{}:antigravity:claude", now.timestamp()),
            account_label: "user@example.com".into(),
            user: Some("user@example.com".into()),
            provider_id: "antigravity".into(),
            provider_name: "Antigravity".into(),
            metric_label: "Claude & GPT OSS".into(),
            window_type: "5h".into(),
            reset_at: now - chrono::Duration::minutes(15),
            percent_before: 100,
        },
        QuotaRenewal {
            id: format!("test:{}:openai:5h", now.timestamp()),
            account_label: "s2.luan2009@gmail.com".into(),
            user: Some("s2.luan2009@gmail.com".into()),
            provider_id: "openai".into(),
            provider_name: "Codex".into(),
            metric_label: "Codex 5h".into(),
            window_type: "5h".into(),
            reset_at: now - chrono::Duration::minutes(35),
            percent_before: 80,
        },
    ];
    let data = serde_json::to_vec_pretty(&items)?;
    crate::cache::atomic_write(&path, &data)?;

    // Fire desktop notifications immediately so the user sees them right away
    // without needing the monitor daemon running.
    for item in &items {
        let user_display = item.user.as_deref().unwrap_or(&item.account_label);
        let summary = format!(
            "\u{1F514} AI Monitor — Conta Renovada ({} — {})",
            item.provider_name, item.metric_label
        );
        let body = format!(
            "Email {} já está pronto para uso!\n\
             Provedor: {} — Janela: {} renovada.",
            user_display, item.provider_name, item.metric_label
        );
        if let Err(e) = send_desktop_notification(&summary, &body) {
            eprintln!("ai-monitor: falha ao enviar notificação: {e}");
        }
    }
    Ok(())
}

/// Path to the persisted active renewals file.
pub fn active_renewals_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| AppError::Other("could not resolve XDG cache dir".into()))?;
    Ok(base
        .cache_dir()
        .join("ai-monitor")
        .join("active_renewals.json"))
}

/// Load active renewals from disk.
pub fn load_active_renewals() -> Vec<QuotaRenewal> {
    if let Ok(path) = active_renewals_path()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(items) = serde_json::from_str::<Vec<QuotaRenewal>>(&content)
    {
        return items;
    }
    Vec::new()
}

/// Save active renewals to disk.
pub fn save_active_renewals(renewals: &[QuotaRenewal]) -> Result<()> {
    let path = active_renewals_path()?;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let data = serde_json::to_vec_pretty(renewals)?;
    crate::cache::atomic_write(&path, &data)
}

fn lock_path_for(state: &Path) -> PathBuf {
    let mut p = state.as_os_str().to_os_string();
    p.push(".lock");
    PathBuf::from(p)
}

/// Record an active renewal, emit desktop notification if not previously sent, and persist.
pub fn record_active_renewal(renewal: &QuotaRenewal) {
    // Defense in depth: both transition producers should already enforce this,
    // but active_renewals.json is user-visible state and must never accept a
    // zero-usage or future-cycle entry.
    if !is_actionable_renewal(renewal, Utc::now()) {
        return;
    }
    if let Ok(notif_path) = notifications_path()
        && load_notification_store_from(&notif_path)
            .dismissed
            .contains_key(&notification_key(renewal))
    {
        return;
    }
    let mut list = load_active_renewals();
    let now = Utc::now();
    let previous_len = list.len();
    list.retain(|item| is_actionable_renewal(item, now));
    let mut changed = list.len() != previous_len;
    if !list.iter().any(|r| r.id == renewal.id) {
        list.push(renewal.clone());
        changed = true;
    }
    if changed {
        let _ = save_active_renewals(&list);
    }
    // Delivery is intentionally centralized in `deliver_renewal_batch`.
    // Recording happens while snapshot writes are in progress; notifying here
    // used to acquire the notification lock recursively and could wedge a
    // refresh. It also produced one toast per metric instead of one useful
    // summary for the refresh cycle.
}

/// Clear all renewals: simulated test renewals, active renewals, and reset expired metrics in snapshots.
pub fn clear_all_renewals() -> Result<()> {
    if let Ok(notif_path) = notifications_path() {
        dismiss_renewals_at(&notif_path, &load_active_renewals(), Utc::now())?;
    }
    if let Ok(path) = test_renewals_path()
        && path.exists()
    {
        let _ = fs::remove_file(path);
    }
    if let Ok(path) = active_renewals_path()
        && path.exists()
    {
        let _ = fs::remove_file(path);
    }
    // Also reset metrics in snapshots where now >= reset_at so they don't re-trigger
    if let Ok(path) = snapshots_path() {
        let mut store = crate::account_store::load_store_from(&path);
        let now = Utc::now();
        let mut dirty = false;
        for snap in store.snapshots.values_mut() {
            for entry in &mut snap.entries {
                for section in &mut entry.sections {
                    if let ReportSection::Metric {
                        reset_at: Some(reset_at),
                        percent,
                        value,
                        severity,
                        detail,
                        ..
                    } = section
                        && now >= *reset_at
                        && *percent > 0
                    {
                        *percent = 0;
                        *value = "0%".to_string();
                        *severity = "low".to_string();
                        *detail =
                            format!("Renewed at {} (ready to use)", reset_at.format("%H:%M UTC"));
                        dirty = true;
                    }
                }
            }
        }
        if dirty {
            let _ = crate::account_store::save_store_to(&path, &store);
        }
    }
    Ok(())
}

/// Detect active renewal events previously observed by the monitor.
///
/// Do not manufacture events by scanning saved snapshots here: opening the
/// Plasmoid would otherwise turn every old, expired metric into a fresh card.
pub fn detect_all_renewals(now: DateTime<Utc>) -> Vec<QuotaRenewal> {
    let mut results: Vec<QuotaRenewal> = Vec::new();

    // 1. Load active renewals recorded when APIs returned resets
    let active = load_active_renewals();
    for r in active {
        // Keep renewals within the last 48 hours
        if is_actionable_renewal(&r, now) && !results.iter().any(|item| item.id == r.id) {
            results.push(r);
        }
    }

    // 2. Load simulated test renewals if any
    if let Ok(path) = test_renewals_path()
        && let Ok(content) = fs::read_to_string(path)
        && let Ok(items) = serde_json::from_str::<Vec<QuotaRenewal>>(&content)
    {
        for item in items {
            if !results.iter().any(|r| r.id == item.id) {
                results.push(item);
            }
        }
    }

    results
}

/// Remember `renewals` as dismissed so the monitor does not record them again.
pub fn dismiss_renewals_at(
    notif_path: &Path,
    renewals: &[QuotaRenewal],
    now: DateTime<Utc>,
) -> Result<()> {
    let _lock = crate::cache::acquire_lock(
        &lock_path_for(notif_path),
        std::time::Duration::from_secs(5),
    );
    let mut store = load_notification_store_from(notif_path);
    for renewal in renewals {
        store.dismissed.insert(notification_key(renewal), now);
    }
    store.prune(now);
    save_notification_store_to(notif_path, &store)
}

pub fn load_notification_store_from(path: &Path) -> NotificationStore {
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return NotificationStore::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_notification_store_to(path: &Path, store: &NotificationStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(store)?;
    crate::cache::atomic_write(path, &data)
}

/// AppUserModelID the Windows toast is posted under. Windows silently drops a
/// toast whose app id has no Start-menu registration, and an unpackaged CLI has
/// none, so borrow Windows PowerShell's: it ships registered on every install.
#[cfg(target_os = "windows")]
const WINDOWS_TOAST_APP_ID: &str =
    r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe";

/// Send a native desktop notification across Linux, macOS, and Windows.
pub fn send_desktop_notification(summary: &str, body: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('\\', "\\\\").replace('"', "\\\""),
            summary.replace('\\', "\\\\").replace('"', "\\\""),
        );
        let mut cmd = std::process::Command::new("osascript");
        cmd.args(["-e", &script]);
        cmd.status()?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        let escaped_summary = summary.replace('\'', "''");
        let escaped_body = body.replace('\'', "''");
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $template = [Windows.UI.Notifications.ToastTemplateType]::ToastText02; \
             $xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent($template); \
             $text = $xml.GetElementsByTagName('text'); \
             $text[0].AppendChild($xml.CreateTextNode('{escaped_summary}')) > $null; \
             $text[1].AppendChild($xml.CreateTextNode('{escaped_body}')) > $null; \
             $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{WINDOWS_TOAST_APP_ID}').Show($toast);"
        );
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(crate::process::CREATE_NO_WINDOW);
        }
        cmd.status()?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let mut cmd = std::process::Command::new("notify-send");
        cmd.args([
            "-a",
            "AI Monitor",
            "-i",
            "dialog-information",
            "-u",
            "normal",
            summary,
            body,
        ]);
        cmd.status()?;
        Ok(())
    }
}

/// Check all snapshots for expired reset windows and fire notifications.
/// Accepts a custom notifier function for hermetic testability.
pub fn check_resets_with<F>(
    notif_path: &Path,
    snapshots: &[AccountSnapshot],
    now: DateTime<Utc>,
    mut notify: F,
) -> usize
where
    F: FnMut(&str, &str),
{
    let renewals: Vec<QuotaRenewal> = snapshots
        .iter()
        .flat_map(|snap| expired_windows(snap, now))
        .collect();
    deliver_renewal_batch(notif_path, &renewals, now, |summary, body| {
        notify(summary, body);
        Ok(())
    })
}

/// Deliver a batch of renewals that was observed during the current monitor
/// transition.  Do not feed this historical snapshots merely because a new
/// account logged in: an old `reset_at` is not a new desktop event.
pub fn notify_renewal_batch(
    notif_path: &Path,
    renewals: &[QuotaRenewal],
    now: DateTime<Utc>,
) -> usize {
    if !crate::config::Config::load()
        .unwrap_or_default()
        .ui
        .notify_resets()
    {
        return 0;
    }
    deliver_renewal_batch(notif_path, renewals, now, send_desktop_notification)
}

/// One desktop toast for every renewal in `renewals` not already sent or
/// dismissed. `send` is the notifier seam; the store is only marked when it
/// succeeds, so a failed toast is retried on the next cycle.
pub fn deliver_renewal_batch<F>(
    notif_path: &Path,
    renewals: &[QuotaRenewal],
    now: DateTime<Utc>,
    mut send: F,
) -> usize
where
    F: FnMut(&str, &str) -> std::io::Result<()>,
{
    let _lock = crate::cache::acquire_lock(
        &lock_path_for(notif_path),
        std::time::Duration::from_secs(5),
    );
    let mut store = load_notification_store_from(notif_path);
    let mut pending_keys = BTreeMap::new();
    let pending: Vec<_> = renewals
        .iter()
        .filter(|renewal| {
            // A reset must be due and must have had recorded consumption. This
            // excludes placeholder/zero windows and keeps the historical
            // timestamp behaviour selected by the user.
            let key = notification_key(renewal);
            is_actionable_renewal(renewal, now)
                && !store.sent.contains_key(&key)
                && !store.dismissed.contains_key(&key)
                && pending_keys.insert(key, ()).is_none()
        })
        .collect();
    if pending.is_empty() {
        return 0;
    }

    let summary = if pending.len() == 1 {
        format!("AI Monitor — Cota renovada ({})", pending[0].provider_name)
    } else {
        format!("AI Monitor — {} cotas renovadas", pending.len())
    };
    let body = pending
        .iter()
        .map(|renewal| {
            format!(
                "{} — {}: {} ({})",
                renewal.account_label,
                renewal.provider_name,
                renewal.metric_label,
                renewal.window_type
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if let Err(err) = send(&summary, &body) {
        eprintln!("ai-monitor monitor: failed to send notification: {err}");
        return 0;
    }
    for renewal in &pending {
        store.sent.insert(notification_key(renewal), now);
    }
    store.prune(now);
    if let Err(error) = save_notification_store_to(notif_path, &store) {
        eprintln!("ai-monitor monitor: failed to save notification state: {error}");
    }
    pending.len()
}

/// Run the background monitor loop.
pub async fn run(
    interval_secs: u64,
    once: bool,
    test: bool,
    simulate_renewal: bool,
    clear_renewals: bool,
) -> i32 {
    if simulate_renewal {
        if let Err(e) = simulate_test_renewal() {
            eprintln!("ai-monitor monitor: falha ao simular renovação: {e}");
            return 1;
        }
        println!("Aviso de renovação simulado com sucesso em test_renewals.json!");
        return 0;
    }
    if clear_renewals {
        if let Err(e) = clear_all_renewals() {
            eprintln!("ai-monitor monitor: falha ao limpar avisos: {e}");
            return 1;
        }
        println!("Avisos de renovação simulados limpos com sucesso!");
        return 0;
    }
    if test {
        let summary = "AI Monitor — Cota Renovada (Google Antigravity)";
        let body = "A conta 'user@example.com' (Google Antigravity) renovou a janela de 'Claude & GPT OSS' e já pode ser utilizada!";
        println!("Enviando notificação desktop de teste...");
        println!("  Título:   {summary}");
        println!("  Mensagem: {body}");
        if let Err(err) = send_desktop_notification(summary, body) {
            eprintln!("ai-monitor monitor: falha ao enviar notificação: {err}");
            return 1;
        }
        println!("Notificação enviada com sucesso para o desktop!");
        return 0;
    }

    // Windows has no service manager holding the pid; record it so
    // `--uninstall-service` stops this instance and no other.
    #[cfg(windows)]
    if !once && let Some(path) = crate::monitor_service::pid_path() {
        let _ = crate::cache::atomic_write(&path, std::process::id().to_string().as_bytes());
    }

    let mut monitor = crate::account_monitor::BackgroundMonitor::new(interval_secs, once);
    monitor.run_loop().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_store::{AccountSnapshot, ReportEntry, ReportSection};
    use tempfile::tempdir;

    #[test]
    fn check_resets_notifies_and_deduplicates() {
        let dir = tempdir().unwrap();
        let notif_path = dir.path().join("notifications.json");

        let past = Utc::now() - chrono::Duration::minutes(10);
        let snapshots = vec![AccountSnapshot {
            account_label: "personal".into(),
            user: Some("personal@dev".into()),
            saved_at: Utc::now(),
            providers: vec!["antigravity".into()],
            entries: vec![ReportEntry {
                id: "antigravity".into(),
                name: "Google Antigravity".into(),
                display_name: "Google Antigravity".into(),
                short_name: "AG".into(),
                icon: "󰧑".into(),
                brand: None,
                plan: None,
                sections: vec![ReportSection::Metric {
                    label: "5-Hour Limit".into(),
                    percent: 50,
                    value: "50%".into(),
                    detail: "resets in 0m".into(),
                    severity: "neutral".into(),
                    reset_at: Some(past),
                    window_secs: Some(5 * 3600),
                }],
                error: None,
                stale: false,
                fetched_at: None,
            }],
        }];

        let mut captured = Vec::new();
        let count1 = check_resets_with(&notif_path, &snapshots, Utc::now(), |s, b| {
            captured.push((s.to_string(), b.to_string()));
        });
        assert_eq!(count1, 1);
        assert_eq!(captured.len(), 1);
        assert!(captured[0].0.contains("Cota renovada"));
        assert!(captured[0].1.contains("personal"));

        // Second pass at same or later time must NOT send duplicate notification
        let mut captured2 = Vec::new();
        let count2 = check_resets_with(&notif_path, &snapshots, Utc::now(), |s, b| {
            captured2.push((s.to_string(), b.to_string()));
        });
        assert_eq!(count2, 0);
        assert_eq!(captured2.len(), 0);
    }

    fn metric(
        label: &str,
        percent: u16,
        reset_at: DateTime<Utc>,
        window_secs: Option<u64>,
    ) -> ReportSection {
        ReportSection::Metric {
            label: label.into(),
            percent,
            value: format!("{percent}%"),
            detail: String::new(),
            severity: "low".into(),
            reset_at: Some(reset_at),
            window_secs,
        }
    }

    fn agy(sections: Vec<ReportSection>) -> ReportEntry {
        ReportEntry {
            id: "antigravity".into(),
            name: "Google Antigravity".into(),
            display_name: "Google Antigravity".into(),
            short_name: "AG".into(),
            icon: String::new(),
            brand: None,
            plan: None,
            sections,
            error: None,
            stale: false,
            fetched_at: None,
        }
    }

    const FIVE_H: Option<u64> = Some(5 * 3600);
    const WEEK: Option<u64> = Some(7 * 86_400);

    #[test]
    fn same_label_windows_are_paired_by_window_not_by_label() {
        // Antigravity reports "Gemini" twice. Pairing by label compared the
        // weekly pool against the 5h one and flooded the panel with renewals.
        let now = Utc::now();
        let before = agy(vec![
            metric("Gemini", 90, now - chrono::Duration::minutes(5), FIVE_H),
            metric("Gemini", 30, now + chrono::Duration::days(3), WEEK),
        ]);
        let after = agy(vec![
            metric("Gemini", 0, now + chrono::Duration::hours(5), FIVE_H),
            metric("Gemini", 31, now + chrono::Duration::days(3), WEEK),
        ]);
        let renewals = entry_renewals("acct", None, &before, &after, now);
        assert_eq!(renewals.len(), 1);
        assert_eq!(renewals[0].window_type, "5h");
        assert_eq!(renewals[0].percent_before, 90);
    }

    #[test]
    fn a_future_reset_that_moves_is_not_a_renewal() {
        let now = Utc::now();
        let before = agy(vec![metric(
            "Claude & GPT OSS",
            40,
            now + chrono::Duration::minutes(10),
            FIVE_H,
        )]);
        let after = agy(vec![metric(
            "Claude & GPT OSS",
            10,
            now + chrono::Duration::minutes(16),
            FIVE_H,
        )]);
        assert!(entry_renewals("acct", None, &before, &after, now).is_empty());
    }

    #[test]
    fn a_window_missing_after_refresh_is_not_announced() {
        let now = Utc::now();
        let before = agy(vec![metric(
            "Gemini",
            90,
            now - chrono::Duration::minutes(5),
            FIVE_H,
        )]);
        assert!(entry_renewals("acct", None, &before, &agy(vec![]), now).is_empty());
    }

    #[test]
    fn five_hour_and_weekly_renewals_have_distinct_keys() {
        let now = Utc::now();
        let reset = now - chrono::Duration::minutes(1);
        let entry = agy(vec![
            metric("Gemini", 50, reset, FIVE_H),
            metric("Gemini", 50, reset, WEEK),
        ]);
        let renewals = entry_renewals("acct", None, &entry, &entry, now);
        assert_eq!(renewals.len(), 2);
        assert_ne!(
            notification_key(&renewals[0]),
            notification_key(&renewals[1])
        );
    }

    #[test]
    fn window_type_falls_back_to_the_label() {
        assert_eq!(window_type(FIVE_H, "Gemini"), "5h");
        assert_eq!(window_type(WEEK, "Gemini"), "Semanal");
        assert_eq!(window_type(None, "Codex weekly"), "Semanal");
        assert_eq!(window_type(None, "Weekly (7d)"), "Semanal");
        assert_eq!(window_type(None, "Session (5h)"), "5h");
    }

    #[test]
    fn dismissed_renewals_are_never_delivered() {
        let dir = tempdir().unwrap();
        let notif_path = dir.path().join("notifications.json");
        let now = Utc::now();
        let entry = agy(vec![metric(
            "Gemini",
            50,
            now - chrono::Duration::minutes(3),
            FIVE_H,
        )]);
        let renewals = entry_renewals("acct", None, &entry, &entry, now);
        dismiss_renewals_at(&notif_path, &renewals, now).unwrap();
        let sent = deliver_renewal_batch(&notif_path, &renewals, now, |_, _| Ok(()));
        assert_eq!(sent, 0);
    }

    #[test]
    fn a_failed_toast_is_retried_next_cycle() {
        let dir = tempdir().unwrap();
        let notif_path = dir.path().join("notifications.json");
        let now = Utc::now();
        let entry = agy(vec![metric(
            "Gemini",
            50,
            now - chrono::Duration::minutes(3),
            FIVE_H,
        )]);
        let renewals = entry_renewals("acct", None, &entry, &entry, now);
        let failed = deliver_renewal_batch(&notif_path, &renewals, now, |_, _| {
            Err(std::io::Error::other("no notification daemon"))
        });
        assert_eq!(failed, 0);
        assert_eq!(
            deliver_renewal_batch(&notif_path, &renewals, now, |_, _| Ok(())),
            1
        );
        assert_eq!(
            deliver_renewal_batch(&notif_path, &renewals, now, |_, _| Ok(())),
            0
        );
    }

    #[test]
    fn prune_forgets_keys_past_retention() {
        let now = Utc::now();
        let mut store = NotificationStore::default();
        store.sent.insert(
            "old".into(),
            now - chrono::Duration::days(KEY_RETENTION_DAYS + 1),
        );
        store
            .sent
            .insert("new".into(), now - chrono::Duration::hours(1));
        store.dismissed.insert(
            "old".into(),
            now - chrono::Duration::days(KEY_RETENTION_DAYS),
        );
        store.prune(now);
        assert_eq!(store.sent.keys().collect::<Vec<_>>(), ["new"]);
        assert!(store.dismissed.is_empty());
    }

    #[test]
    fn zero_usage_or_future_cycles_are_not_actionable() {
        let now = Utc::now();
        let mut renewal = QuotaRenewal {
            id: "account:antigravity:Gemini:test".into(),
            account_label: "account".into(),
            user: None,
            provider_id: "antigravity".into(),
            provider_name: "Antigravity".into(),
            metric_label: "Gemini".into(),
            window_type: "5h".into(),
            reset_at: now - chrono::Duration::minutes(1),
            percent_before: 0,
        };
        assert!(!is_actionable_renewal(&renewal, now));

        renewal.percent_before = 1;
        renewal.reset_at = now + chrono::Duration::minutes(1);
        assert!(!is_actionable_renewal(&renewal, now));

        renewal.reset_at = now - chrono::Duration::minutes(1);
        assert!(is_actionable_renewal(&renewal, now));
    }
}
