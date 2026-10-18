use std::fs;

use super::*;
use crate::model::finding::{BlockReason, Reversibility, Risk};
use crate::safety::boundary::{CONFIG_CACHE_ALLOWLIST, check_target};
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};

const TWO_MIB: usize = 2 * 1024 * 1024;

fn add_process(fx: &Fixture, pid: &str, comm: &str) {
    let process_dir = fx.env.proc_root.join(pid);
    fs::create_dir_all(&process_dir).unwrap();
    fs::write(process_dir.join("comm"), format!("{comm}\n")).unwrap();
}

#[test]
fn app_cache_blocked_while_app_runs() {
    let fx = fixture();
    write_bytes(&fx.home.join(".cache/google-chrome/Default/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".config/Slack/Cache/blob"), TWO_MIB);
    add_process(&fx, "77", "chrome");
    let output = AppCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(output.findings.len(), 2);
    let chrome = output
        .findings
        .iter()
        .find(|f| f.location == "~/.cache/google-chrome")
        .unwrap();
    let Some(BlockReason::InUse(holders)) = &chrome.blocked else {
        panic!("esperava InUse, veio {:?}", chrome.blocked);
    };
    assert_eq!(holders[0].name, "chrome");
    let slack = output
        .findings
        .iter()
        .find(|f| f.location == "~/.config/Slack/Cache")
        .unwrap();
    assert!(slack.blocked.is_none());
    assert_eq!(slack.risk, Risk::Safe);
    assert_eq!(slack.reversibility, Reversibility::Regenerable);
}

#[test]
fn watched_for_follows_the_item_path() {
    let fx = fixture();
    write_bytes(&fx.home.join(".cache/google-chrome/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".config/Slack/Code Cache/blob"), TWO_MIB);
    let output = AppCaches.scan(&fx.ctx()).unwrap();
    let watched: Vec<Vec<&str>> = output
        .findings
        .iter()
        .map(|finding| AppCaches.watched_processes_for(finding))
        .collect();
    assert_eq!(watched, [vec!["chrome"], vec!["slack"]]);
    let mut unknown = output.findings[0].clone();
    unknown.action = crate::model::finding::Action::Delete(fx.home.join(".cache/outro"));
    assert_eq!(AppCaches.watched_processes_for(&unknown), WATCHED.to_vec());
}

struct NoTrash;

impl crate::clean::Discard for NoTrash {
    fn discard(&self, path: &std::path::Path) -> std::io::Result<()> {
        panic!("nada deveria ir para a Lixeira: {path:?}")
    }
}

#[test]
fn clean_skips_only_the_running_app() {
    use crate::clean::{CleanRequest, ExecContext, ItemOutcome, SkipReason, clean};
    let fx = fixture();
    let chrome_cache = fx.home.join(".cache/google-chrome");
    let slack_cache = fx.home.join(".config/Slack/Cache");
    write_bytes(&chrome_cache.join("blob"), TWO_MIB);
    write_bytes(&slack_cache.join("blob"), TWO_MIB);
    let ctx = fx.ctx();
    let findings = AppCaches.scan(&ctx).unwrap().findings;
    add_process(&fx, "77", "chrome");
    let mut ledger = crate::safety::ledger::Ledger::new(&fx.env, findings.clone());
    let request = CleanRequest {
        ids: findings.iter().map(|f| f.id.clone()).collect(),
        allow_irreversible: false,
    };
    let audit = crate::safety::audit::AuditLog::open(&fx.env).unwrap();
    let roots_for = |_: &crate::model::finding::Finding| AppCaches.roots(&ctx);
    let watched_names_for =
        |finding: &crate::model::finding::Finding| AppCaches.watched_processes_for(finding);
    let exec = ExecContext {
        env: &fx.env,
        runner: &fx.runner,
        trash: &NoTrash,
        audit: &audit,
        roots_for: &roots_for,
        watched_names_for: &watched_names_for,
        in_use_scope_for: &|_| None,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let report = clean(&mut ledger, &request, &exec, &|_| {}, &cancel).unwrap();
    let outcome_of = |location: &str| {
        report
            .items
            .iter()
            .find(|(finding, _)| finding.location == location)
            .map(|(_, outcome)| outcome.clone())
            .unwrap()
    };
    assert!(matches!(
        outcome_of("~/.cache/google-chrome"),
        ItemOutcome::Skipped(SkipReason::InUse(_))
    ));
    assert_eq!(
        outcome_of("~/.config/Slack/Cache"),
        ItemOutcome::Done { output: None }
    );
    assert!(chrome_cache.exists());
    assert!(!slack_cache.exists());
}

#[test]
fn distro_process_names_block_their_browser() {
    let fx = fixture();
    write_bytes(&fx.home.join(".cache/chromium/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".cache/mozilla/blob"), TWO_MIB);
    add_process(&fx, "11", "chromium-browse");
    add_process(&fx, "12", "firefox-esr");
    let output = AppCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(output.findings.len(), 2);
    assert!(
        output
            .findings
            .iter()
            .all(|f| matches!(f.blocked, Some(BlockReason::InUse(_))))
    );
}

#[test]
fn vscode_user_settings_never_offered() {
    let fx = fixture();
    write_bytes(&fx.home.join(".config/Code/User/settings.json"), TWO_MIB);
    write_bytes(&fx.home.join(".config/Code/Cache/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".config/Code/Backups/blob"), TWO_MIB);
    let output = AppCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(output.findings.len(), 1);
    for finding in &output.findings {
        let path = finding.action.path().unwrap();
        assert!(!path.to_string_lossy().contains("User"));
        assert!(!path.to_string_lossy().contains("Backups"));
    }
}

#[test]
fn config_entries_match_the_allowlist() {
    let mut from_config: Vec<&str> = APP_CACHES
        .iter()
        .filter(|app| app.cache.base == Base::Config)
        .map(|app| app.cache.relative)
        .collect();
    from_config.sort_unstable();
    let mut allowlist = CONFIG_CACHE_ALLOWLIST.to_vec();
    allowlist.sort_unstable();
    assert_eq!(from_config, allowlist);
}

#[test]
fn watched_processes_are_the_union() {
    let mut union: Vec<&str> = APP_CACHES
        .iter()
        .flat_map(|app| app.watched.iter().copied())
        .collect();
    union.sort_unstable();
    union.dedup();
    let mut watched = AppCaches.watched_processes().to_vec();
    watched.sort_unstable();
    assert_eq!(union, watched);
    assert_eq!(watched.len(), 9);
}

#[test]
fn every_item_passes_boundary() {
    let fx = fixture();
    for app in &APP_CACHES {
        write_bytes(&app.cache.path(&fx.env).join("blob"), TWO_MIB);
    }
    let ctx = fx.ctx();
    let roots = AppCaches.roots(&ctx);
    let output = AppCaches.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), APP_CACHES.len());
    for finding in &output.findings {
        let path = finding.action.path().unwrap();
        assert!(check_target(path, &roots, &fx.env).is_ok(), "{path:?}");
    }
}
