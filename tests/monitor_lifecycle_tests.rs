//! Integration tests for background account monitoring, sleep/resume detection,
//! session lifecycle, and quota renewal deduplication.

use std::collections::BTreeMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use tempfile::TempDir;

use ai_monitor::account_monitor::{RenewalEngine, WallClockTracker};
use ai_monitor::account_store::{
    AccountSnapshot, ReportEntry, ReportSection, detect_and_sync_account_session_detailed_at,
    get_snapshot_at, load_store_from,
};
use ai_monitor::config::Config;
use ai_monitor::monitor::check_resets_with;

fn create_mock_report_entry(
    id: &str,
    name: &str,
    metric_label: &str,
    percent: u16,
    reset_at: Option<chrono::DateTime<Utc>>,
    window_secs: Option<u64>,
) -> ReportEntry {
    ReportEntry {
        id: id.to_string(),
        name: name.to_string(),
        display_name: name.to_string(),
        short_name: id.chars().take(2).collect(),
        icon: "󰧑".into(),
        brand: None,
        plan: None,
        sections: vec![ReportSection::Metric {
            label: metric_label.to_string(),
            percent,
            value: format!("{percent}%"),
            detail: format!("{percent}% used"),
            severity: if percent > 80 {
                "critical".into()
            } else {
                "low".into()
            },
            reset_at,
            window_secs,
        }],
        error: None,
        stale: false,
        fetched_at: Some(Utc::now()),
    }
}

#[test]
fn test_new_account_detection_and_registration() {
    let dir = TempDir::new().unwrap();
    let store_path = dir.path().join("account_snapshots.json");
    let config_path = dir.path().join("config.toml");
    let mut config = Config::default();

    // Enable OpenAI
    config.openai.enabled = true;
    // Hermetic: never let Claude identity detection fall back to the real ~/.claude.json.
    config.anthropic.credentials_path = Some(dir.path().join("claude").join(".credentials.json"));
    let auth_path = dir.path().join("auth.json");
    let jwt_payload = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        r#"{"https://api.openai.com/profile":{"email":"alice@example.com"}}"#,
    );
    let fake_jwt = format!("header.{jwt_payload}.sig");
    let auth_json = format!(
        r#"{{"tokens":{{"id_token":"{fake_jwt}","access_token":"dummy","refresh_token":"dummy"}}}}"#
    );
    fs::write(&auth_path, auth_json).unwrap();
    config.openai.codex_auth_path = Some(auth_path);

    // Initial sync detects Alice
    let res =
        detect_and_sync_account_session_detailed_at(&store_path, &mut config, Some(&config_path))
            .unwrap();

    assert!(res.changed);
    assert_eq!(res.changes.len(), 1);
    assert_eq!(res.changes[0].provider, "openai");
    assert_eq!(res.changes[0].new_identity, "alice@example.com");
    assert!(res.changes[0].is_new_account);
    assert_eq!(res.changes[0].previous_identity, None);

    // Snapshot store has Alice
    let snap = get_snapshot_at(&store_path, "alice@example.com").expect("snapshot must exist");
    assert_eq!(snap.account_label, "alice@example.com");
    assert!(snap.providers.contains(&"openai".to_string()));

    // Repeated sync with the same account does not flag new account or change
    let res2 =
        detect_and_sync_account_session_detailed_at(&store_path, &mut config, Some(&config_path))
            .unwrap();
    assert!(!res2.changed);
    assert!(res2.changes.is_empty());
}

#[test]
fn test_account_switching_preserves_previous_and_registers_new() {
    let dir = TempDir::new().unwrap();
    let store_path = dir.path().join("account_snapshots.json");
    let config_path = dir.path().join("config.toml");
    let mut config = Config::default();

    config.openai.enabled = true;
    // Hermetic: never let Claude identity detection fall back to the real ~/.claude.json.
    config.anthropic.credentials_path = Some(dir.path().join("claude").join(".credentials.json"));
    let auth_path = dir.path().join("auth.json");

    // 1. First login as User A
    let jwt_a = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        r#"{"https://api.openai.com/profile":{"email":"user_a@dev.com"}}"#,
    );
    fs::write(&auth_path, format!(r#"{{"tokens":{{"id_token":"header.{jwt_a}.sig","access_token":"dummy","refresh_token":"dummy"}}}}"#)).unwrap();
    config.openai.codex_auth_path = Some(auth_path.clone());

    let res1 =
        detect_and_sync_account_session_detailed_at(&store_path, &mut config, Some(&config_path))
            .unwrap();
    assert_eq!(res1.changes.len(), 1);
    assert_eq!(res1.changes[0].new_identity, "user_a@dev.com");
    assert!(res1.changes[0].is_new_account);

    // 2. User switches to User B
    let jwt_b = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        r#"{"https://api.openai.com/profile":{"email":"user_b@dev.com"}}"#,
    );
    fs::write(&auth_path, format!(r#"{{"tokens":{{"id_token":"header.{jwt_b}.sig","access_token":"dummy","refresh_token":"dummy"}}}}"#)).unwrap();

    let res2 =
        detect_and_sync_account_session_detailed_at(&store_path, &mut config, Some(&config_path))
            .unwrap();
    assert_eq!(res2.changes.len(), 1);
    let change = &res2.changes[0];
    assert_eq!(change.provider, "openai");
    assert_eq!(change.new_identity, "user_b@dev.com");
    assert_eq!(change.previous_identity.as_deref(), Some("user_a@dev.com"));
    assert!(!change.is_new_account);

    // Both User A and User B exist in snapshots store independently
    let store = load_store_from(&store_path);
    assert!(store.snapshots.contains_key("user_a@dev.com"));
    assert!(store.snapshots.contains_key("user_b@dev.com"));
    assert_eq!(
        store.active_sessions.get("openai").map(|s| s.as_str()),
        Some("user_b@dev.com")
    );
}

#[test]
fn test_sleep_and_resume_gap_detection() {
    let now = Utc::now();
    let mut tracker = WallClockTracker::new(now, 5);
    let interval = Duration::from_secs(30);

    // Normal tick (30s) -> No gap
    assert_eq!(
        tracker.check_gap(now + ChronoDuration::seconds(30), interval),
        None
    );

    // Jitter tick (34s) -> No gap
    assert_eq!(
        tracker.check_gap(now + ChronoDuration::seconds(64), interval),
        None
    );

    // Suspend for 4 hours (14400s) -> Gap detected
    let wake_time = now + ChronoDuration::seconds(64) + ChronoDuration::hours(4);
    let gap = tracker.check_gap(wake_time, interval);
    assert!(gap.is_some());
    assert!(gap.unwrap() >= 14400);
}

#[test]
fn test_renewal_detection_during_sleep() {
    let sleep_time = Utc::now() - ChronoDuration::hours(6);
    let renewal_due_in_sleep = sleep_time + ChronoDuration::hours(2);
    let wake_time = Utc::now();

    let before_snap = AccountSnapshot {
        account_label: "dev@corp".into(),
        user: Some("dev@corp".into()),
        saved_at: sleep_time,
        providers: vec!["antigravity".into()],
        entries: vec![create_mock_report_entry(
            "antigravity",
            "Google Antigravity",
            "Gemini",
            95,
            Some(renewal_due_in_sleep),
            Some(18000),
        )],
    };

    let mut before_map = BTreeMap::new();
    before_map.insert("dev@corp".into(), before_snap);

    // On resume: provider resets usage to 0% and gives new 5h window
    let after_snap = AccountSnapshot {
        account_label: "dev@corp".into(),
        user: Some("dev@corp".into()),
        saved_at: wake_time,
        providers: vec!["antigravity".into()],
        entries: vec![create_mock_report_entry(
            "antigravity",
            "Google Antigravity",
            "Gemini",
            0,
            Some(wake_time + ChronoDuration::hours(5)),
            Some(18000),
        )],
    };

    let renewals = RenewalEngine::detect_transitions(
        &before_map,
        &[after_snap],
        wake_time,
        Some((sleep_time, wake_time)),
    );

    assert_eq!(renewals.len(), 1);
    assert_eq!(renewals[0].metric_label, "Gemini");
    assert_eq!(renewals[0].reset_at, renewal_due_in_sleep);
    assert_eq!(renewals[0].percent_before, 95);
}

#[test]
fn test_renewal_detection_window_rollover() {
    let now = Utc::now();
    let old_reset = now - ChronoDuration::minutes(2);
    let new_reset = now + ChronoDuration::hours(5);

    let before = AccountSnapshot {
        account_label: "user@test".into(),
        user: Some("user@test".into()),
        saved_at: now - ChronoDuration::minutes(15),
        providers: vec!["openai".into()],
        entries: vec![create_mock_report_entry(
            "openai",
            "Codex",
            "5-Hour Limit",
            80,
            Some(old_reset),
            Some(18000),
        )],
    };

    let mut before_map = BTreeMap::new();
    before_map.insert("user@test".into(), before);

    let after = vec![AccountSnapshot {
        account_label: "user@test".into(),
        user: Some("user@test".into()),
        saved_at: now,
        providers: vec!["openai".into()],
        entries: vec![create_mock_report_entry(
            "openai",
            "Codex",
            "5-Hour Limit",
            0,
            Some(new_reset),
            Some(18000),
        )],
    }];

    let renewals = RenewalEngine::detect_transitions(&before_map, &after, now, None);
    assert_eq!(renewals.len(), 1);
    assert_eq!(renewals[0].metric_label, "5-Hour Limit");
    assert_eq!(renewals[0].reset_at, old_reset);
}

#[test]
fn test_notification_deduplication_is_strictly_idempotent() {
    let dir = TempDir::new().unwrap();
    let notif_path = dir.path().join("notifications.json");

    let past_reset = Utc::now() - ChronoDuration::minutes(15);
    let snapshots = vec![AccountSnapshot {
        account_label: "maximus@dev".into(),
        user: Some("maximus@dev".into()),
        saved_at: Utc::now(),
        providers: vec!["openai".into()],
        entries: vec![create_mock_report_entry(
            "openai",
            "Codex",
            "5-Hour Limit",
            75,
            Some(past_reset),
            Some(18000),
        )],
    }];

    let notifications_fired = std::sync::Arc::new(AtomicUsize::new(0));
    let fired_clone = notifications_fired.clone();
    let mut notifier = move |_: &str, _: &str| {
        fired_clone.fetch_add(1, Ordering::SeqCst);
    };

    // Poll 1: Must notify exactly once
    let count1 = check_resets_with(&notif_path, &snapshots, Utc::now(), &mut notifier);
    assert_eq!(count1, 1);
    assert_eq!(notifications_fired.load(Ordering::SeqCst), 1);

    // Poll 2 to Poll 10: Must NOT notify again (idempotent deduplication)
    for _ in 2..=10 {
        let count = check_resets_with(&notif_path, &snapshots, Utc::now(), &mut notifier);
        assert_eq!(count, 0);
    }
    assert_eq!(notifications_fired.load(Ordering::SeqCst), 1);
}
