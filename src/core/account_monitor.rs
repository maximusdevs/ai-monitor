//! Reactive, persistent background account and quota renewal monitoring engine.
//!
//! Provides:
//! - Event-driven architecture responding instantly to new accounts, account switches,
//!   and system resume from sleep/hibernation.
//! - Wall-clock jump tracking to detect computer suspend/sleep and trigger immediate
//!   revalidation without waiting for delayed timers.
//! - Live fetching of new and switched accounts so widget snapshots are immediately populated.
//! - State-transition renewal detection (handling provider rollovers, 0% usage drops,
//!   and renewals that occurred during sleep).
//! - Thread-safe, cross-process flock deduplication of notifications.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use crate::account_store::{
    AccountSnapshot, all_snapshots, detect_and_sync_account_session_detailed, load_store,
};
use crate::config::Config;
use crate::monitor::{QuotaRenewal, entry_renewals, notifications_path, record_active_renewal};

/// High-priority lifecycle and scheduler events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorEvent {
    /// A new account was discovered in local credentials.
    NewAccountDetected { provider: String, account: String },
    /// An existing provider session switched identity.
    AccountSwitched {
        provider: String,
        from: Option<String>,
        to: String,
    },
    /// Computer returned from suspend / sleep / hibernate.
    SystemResumed { gap_secs: i64 },
    /// Configuration file changed on disk.
    ConfigChanged,
    /// Manual refresh requested by user or widget.
    ManualRefresh,
    /// Regular scheduler tick.
    Tick,
}

/// Tracks wall-clock progression to detect system sleep / hibernation gaps.
#[derive(Debug, Clone)]
pub struct WallClockTracker {
    last_tick: DateTime<Utc>,
    tolerance_secs: i64,
}

impl WallClockTracker {
    pub fn new(now: DateTime<Utc>, tolerance_secs: i64) -> Self {
        Self {
            last_tick: now,
            tolerance_secs: tolerance_secs.max(2),
        }
    }

    /// Check if the time between the previous tick and `now` exceeded the expected interval.
    /// Returns `Some(gap_secs)` if a sleep/suspend gap was detected.
    pub fn check_gap(&mut self, now: DateTime<Utc>, expected_interval: Duration) -> Option<i64> {
        let elapsed = (now - self.last_tick).num_seconds();
        let expected = expected_interval.as_secs() as i64;
        let threshold = expected + self.tolerance_secs;
        let gap = if elapsed > threshold {
            Some(elapsed)
        } else {
            None
        };
        self.last_tick = now;
        gap
    }

    pub fn last_tick(&self) -> DateTime<Utc> {
        self.last_tick
    }
}

/// Engine to detect quota renewals by comparing previous state with current state.
#[derive(Debug, Default)]
pub struct RenewalEngine;

impl RenewalEngine {
    /// Detect renewals across a transition from `before` to `after`.
    ///
    /// Provider rollovers, drops to 0% and resets that fell during sleep are
    /// all one rule, owned by [`crate::monitor::entry_renewals`]: a window with
    /// recorded use whose reset has passed. `sleep_interval` is kept for
    /// callers that report it; the rule already covers a reset inside it.
    pub fn detect_transitions(
        before: &BTreeMap<String, AccountSnapshot>,
        after: &[AccountSnapshot],
        now: DateTime<Utc>,
        _sleep_interval: Option<(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Vec<QuotaRenewal> {
        after
            .iter()
            .flat_map(|new_snap| {
                let old_snap = before.get(&new_snap.account_label);
                new_snap.entries.iter().flat_map(move |new_entry| {
                    old_snap
                        .and_then(|s| s.entries.iter().find(|e| e.id == new_entry.id))
                        .map(|old_entry| {
                            entry_renewals(
                                &new_snap.account_label,
                                new_snap.user.as_deref(),
                                old_entry,
                                new_entry,
                                now,
                            )
                        })
                        .unwrap_or_default()
                })
            })
            .collect()
    }
}

/// Central background monitor service.
pub struct BackgroundMonitor {
    pub interval: Duration,
    pub once: bool,
    wall_clock: WallClockTracker,
    prev_snapshots: BTreeMap<String, AccountSnapshot>,
    event_tx: mpsc::Sender<MonitorEvent>,
    event_rx: mpsc::Receiver<MonitorEvent>,
}

impl BackgroundMonitor {
    /// Create a new background monitor instance.
    pub fn new(interval_secs: u64, once: bool) -> Self {
        let (tx, rx) = mpsc::channel(64);
        let interval = Duration::from_secs(interval_secs.max(5));
        let now = Utc::now();
        let tracker = WallClockTracker::new(now, 5);

        // Load existing snapshots from cache into prev_snapshots
        let store = load_store();
        let prev_snapshots = store.snapshots;

        Self {
            interval,
            once,
            wall_clock: tracker,
            prev_snapshots,
            event_tx: tx,
            event_rx: rx,
        }
    }

    /// Run the monitoring daemon loop.
    pub async fn run_loop(&mut self) -> i32 {
        println!(
            "[{}] AI Monitor: Background monitor iniciado (intervalo: {}s)...",
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            self.interval.as_secs()
        );

        // Initial synchronization pass: discover accounts and fetch immediately
        self.initial_discovery_and_fetch().await;

        if self.once {
            return 0;
        }

        loop {
            tokio::select! {
                maybe_event = self.event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            self.handle_event(event).await;
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(self.interval) => {
                    // Check for sleep/suspend gap on timer wake
                    let now = Utc::now();
                    if let Some(gap_secs) = self.wall_clock.check_gap(now, self.interval) {
                        self.handle_event(MonitorEvent::SystemResumed { gap_secs }).await;
                    } else {
                        self.handle_event(MonitorEvent::Tick).await;
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    println!(
                        "\n[{}] AI Monitor: Monitor encerrado.",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    break;
                }
            }
        }

        0
    }

    /// Perform initial account discovery, live fetching, and quota check on startup.
    async fn initial_discovery_and_fetch(&mut self) {
        let mut config = Config::load().unwrap_or_default();
        let default_cp = crate::config::default_path();

        // 1. Discover local sessions.  Startup is a baseline, never an
        // opportunity to notify every expired entry saved by an older run.
        match detect_and_sync_account_session_detailed(&mut config, default_cp.as_deref()) {
            Ok(sync_res) => {
                if !sync_res.changes.is_empty() {
                    for change in sync_res.changes {
                        if change.is_new_account {
                            println!(
                                "[{}] [NOVA CONTA] Detectada conta para {}: {} (iniciando monitoramento)",
                                Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                change.provider,
                                change.new_identity
                            );
                        } else {
                            println!(
                                "[{}] [TROCA DE CONTA] Troca em {}: {:?} -> {}",
                                Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                                change.provider,
                                change.previous_identity,
                                change.new_identity
                            );
                        }
                        // Live fetch for this account immediately
                        self.fetch_and_update_account(&change.new_identity).await;
                    }
                }
            }
            Err(e) => {
                eprintln!("ai-monitor monitor: erro na sincronização de contas: {e}");
            }
        }

        // 2. Fetch active accounts live on startup to ensure widget has fresh data
        let _ = crate::report::refresh_and_sync_account(None).await;
        self.update_cached_snapshots();
        crate::waybar::request_refresh();
    }

    /// Handle lifecycle events.
    async fn handle_event(&mut self, event: MonitorEvent) {
        match event {
            MonitorEvent::NewAccountDetected { provider, account } => {
                println!(
                    "[{}] [EVENT: NEW_ACCOUNT] Provider: {} | Conta: {} — disparando live fetch...",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    provider,
                    account
                );
                self.fetch_and_update_account(&account).await;
                crate::waybar::request_refresh();
            }
            MonitorEvent::AccountSwitched { provider, from, to } => {
                println!(
                    "[{}] [EVENT: ACCOUNT_SWITCHED] Provider: {} | De: {:?} Para: {} — sincronizando widget...",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    provider,
                    from,
                    to
                );
                self.fetch_and_update_account(&to).await;
                crate::waybar::request_refresh();
            }
            MonitorEvent::SystemResumed { gap_secs } => {
                println!(
                    "[{}] [EVENT: SYSTEM_RESUMED] Retorno de suspensão/hibernação detectado (gap de {}s / {:.1}h). Revalidando...",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    gap_secs,
                    gap_secs as f64 / 3600.0
                );
                let sleep_start = self.wall_clock.last_tick();
                let now = Utc::now();
                let sleep_interval = Some((sleep_start, now));

                // Force immediate live fetch of all accounts
                let _ = crate::report::refresh_and_sync_account(None).await;

                // Re-evaluate quota renewals including those that occurred during sleep
                self.check_and_notify_renewals(sleep_interval);

                self.update_cached_snapshots();
                crate::waybar::request_refresh();
            }
            MonitorEvent::ConfigChanged => {
                if let Ok(config) = Config::load() {
                    let configured_secs = config.ui.refresh_interval();
                    if configured_secs >= 5 && Duration::from_secs(configured_secs) != self.interval
                    {
                        println!(
                            "[{}] [CONFIG] Intervalo de monitoramento atualizado para {}s.",
                            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                            configured_secs
                        );
                        self.interval = Duration::from_secs(configured_secs);
                    }
                }
            }
            MonitorEvent::ManualRefresh => {
                let _ = crate::report::refresh_and_sync_account(None).await;
                self.check_and_notify_renewals(None);
                self.update_cached_snapshots();
                crate::waybar::request_refresh();
            }
            MonitorEvent::Tick => {
                let mut config = Config::load().unwrap_or_default();
                let default_cp = crate::config::default_path();

                // Check interval update from config dynamically
                if let Some(secs) = config.ui.refresh_interval {
                    let d = Duration::from_secs(secs.max(5));
                    if d != self.interval {
                        self.interval = d;
                    }
                }

                // Check for account/session discoveries
                if let Ok(sync_res) =
                    detect_and_sync_account_session_detailed(&mut config, default_cp.as_deref())
                {
                    for change in sync_res.changes {
                        if change.is_new_account {
                            let _ = self
                                .event_tx
                                .send(MonitorEvent::NewAccountDetected {
                                    provider: change.provider,
                                    account: change.new_identity,
                                })
                                .await;
                        } else {
                            let _ = self
                                .event_tx
                                .send(MonitorEvent::AccountSwitched {
                                    provider: change.provider,
                                    from: change.previous_identity,
                                    to: change.new_identity,
                                })
                                .await;
                        }
                    }
                }

                // Auto-switch active vendor when a new login is detected
                if let Some(vendor) = crate::detect::detect_recent_login(&config) {
                    let _ = crate::active::write(vendor);
                    crate::waybar::request_refresh();
                    println!(
                        "[{}] [LOGIN] Auto-switched active vendor para {} (login recente detectado)",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                        vendor.display_name()
                    );
                }

                // Periodic check of quota resets
                self.check_and_notify_renewals(None);
                self.update_cached_snapshots();
            }
        }
    }

    /// Perform a live fetch for a target account and update the snapshot store.
    async fn fetch_and_update_account(&mut self, account_label: &str) {
        match crate::report::refresh_and_sync_account(Some(account_label)).await {
            Ok(entries) => {
                println!(
                    "[{}] [FETCH OK] Conta '{}' atualizada com {} seção(ões) / provider(s).",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    account_label,
                    entries.len()
                );
                self.check_and_notify_renewals(None);
                self.update_cached_snapshots();
            }
            Err(e) => {
                eprintln!(
                    "[{}] [FETCH ERRO] Falha ao atualizar conta '{}': {e}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    account_label
                );
            }
        }
    }

    /// Update internal in-memory cache of snapshots for transition tracking.
    fn update_cached_snapshots(&mut self) {
        let store = load_store();
        self.prev_snapshots = store.snapshots;
    }

    /// Check for renewed quotas, record active renewals, and emit notifications safely.
    fn check_and_notify_renewals(
        &mut self,
        sleep_interval: Option<(DateTime<Utc>, DateTime<Utc>)>,
    ) {
        let now = Utc::now();
        let current_snaps = all_snapshots();

        // 1. Use transition engine to detect rollovers and sleep renewals
        let transitions = RenewalEngine::detect_transitions(
            &self.prev_snapshots,
            &current_snaps,
            now,
            sleep_interval,
        );

        for renewal in &transitions {
            record_active_renewal(renewal);
        }

        // Only deliver transitions observed in this cycle.  Scanning every
        // persisted snapshot here made an account login trigger unrelated,
        // historical notifications.
        if let Ok(notif_path) = notifications_path() {
            let count = crate::monitor::notify_renewal_batch(&notif_path, &transitions, now);
            if count > 0 {
                println!(
                    "[{}] [RENOVAÇÃO] {} notificação(ões) de renovação de cota enviada(s).",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
                    count
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_store::{ReportEntry, ReportSection};
    use chrono::Duration as ChronoDuration;

    #[test]
    fn wall_clock_tracker_detects_sleep_gap() {
        let t0 = Utc::now();
        let mut tracker = WallClockTracker::new(t0, 5);
        let interval = Duration::from_secs(30);

        // Regular tick after 30s: no gap
        let t1 = t0 + ChronoDuration::seconds(30);
        assert_eq!(tracker.check_gap(t1, interval), None);

        // Slight latency of 33s (expected 30s + 5s tolerance = 35s): no gap
        let t2 = t1 + ChronoDuration::seconds(33);
        assert_eq!(tracker.check_gap(t2, interval), None);

        // Resume from sleep after 8 hours (28800s): gap detected!
        let t3 = t2 + ChronoDuration::hours(8);
        let gap = tracker.check_gap(t3, interval);
        assert!(gap.is_some());
        assert_eq!(gap.unwrap(), 28800);
    }

    #[test]
    fn renewal_engine_detects_window_rollover() {
        let now = Utc::now();
        let old_reset = now - ChronoDuration::minutes(5);
        let new_reset = now + ChronoDuration::hours(5);

        let before_snap = AccountSnapshot {
            account_label: "test@dev".into(),
            user: Some("test@dev".into()),
            saved_at: now - ChronoDuration::minutes(10),
            providers: vec!["openai".into()],
            entries: vec![ReportEntry {
                id: "openai".into(),
                name: "Codex".into(),
                display_name: "Codex".into(),
                short_name: "CX".into(),
                icon: "󰧑".into(),
                brand: None,
                plan: None,
                sections: vec![ReportSection::Metric {
                    label: "5-Hour Limit".into(),
                    percent: 85,
                    value: "85%".into(),
                    detail: "resets soon".into(),
                    severity: "warning".into(),
                    reset_at: Some(old_reset),
                    window_secs: Some(18000),
                }],
                error: None,
                stale: false,
                fetched_at: None,
            }],
        };

        let mut before_map = BTreeMap::new();
        before_map.insert("test@dev".into(), before_snap);

        // After refresh: provider rolled over to new_reset and percent dropped to 0
        let after_snaps = vec![AccountSnapshot {
            account_label: "test@dev".into(),
            user: Some("test@dev".into()),
            saved_at: now,
            providers: vec!["openai".into()],
            entries: vec![ReportEntry {
                id: "openai".into(),
                name: "Codex".into(),
                display_name: "Codex".into(),
                short_name: "CX".into(),
                icon: "󰧑".into(),
                brand: None,
                plan: None,
                sections: vec![ReportSection::Metric {
                    label: "5-Hour Limit".into(),
                    percent: 0,
                    value: "0%".into(),
                    detail: "0% used".into(),
                    severity: "low".into(),
                    reset_at: Some(new_reset),
                    window_secs: Some(18000),
                }],
                error: None,
                stale: false,
                fetched_at: None,
            }],
        }];

        let renewals = RenewalEngine::detect_transitions(&before_map, &after_snaps, now, None);
        assert_eq!(renewals.len(), 1);
        assert_eq!(renewals[0].metric_label, "5-Hour Limit");
        assert_eq!(renewals[0].provider_id, "openai");
        assert_eq!(renewals[0].reset_at, old_reset);
        assert_eq!(renewals[0].percent_before, 85);
    }

    #[test]
    fn renewal_engine_detects_renewal_during_sleep() {
        let sleep_start = Utc::now() - ChronoDuration::hours(8);
        let reset_during_sleep = sleep_start + ChronoDuration::hours(2);
        let wake_time = Utc::now();

        let before_snap = AccountSnapshot {
            account_label: "personal".into(),
            user: Some("personal@dev".into()),
            saved_at: sleep_start,
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
                    label: "Gemini".into(),
                    percent: 90,
                    value: "90%".into(),
                    detail: "resets in 2h".into(),
                    severity: "critical".into(),
                    reset_at: Some(reset_during_sleep),
                    window_secs: Some(18000),
                }],
                error: None,
                stale: false,
                fetched_at: None,
            }],
        };

        let mut before_map = BTreeMap::new();
        before_map.insert("personal".into(), before_snap);

        let after_snaps = vec![AccountSnapshot {
            account_label: "personal".into(),
            user: Some("personal@dev".into()),
            saved_at: wake_time,
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
                    label: "Gemini".into(),
                    percent: 0,
                    value: "0%".into(),
                    detail: "0% used".into(),
                    severity: "low".into(),
                    reset_at: Some(wake_time + ChronoDuration::hours(5)),
                    window_secs: Some(18000),
                }],
                error: None,
                stale: false,
                fetched_at: None,
            }],
        }];

        let renewals = RenewalEngine::detect_transitions(
            &before_map,
            &after_snaps,
            wake_time,
            Some((sleep_start, wake_time)),
        );
        assert_eq!(renewals.len(), 1);
        assert_eq!(renewals[0].metric_label, "Gemini");
        assert_eq!(renewals[0].reset_at, reset_during_sleep);
        assert_eq!(renewals[0].percent_before, 90);
    }
}
