use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::clean::{CleanRequest, Discard, ExecContext, ItemOutcome, SkipReason, clean};
use crate::model::finding::{Action, Finding, GuideSpec, Risk};
use crate::safety::audit::AuditLog;
use crate::safety::ledger::Ledger;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};
use crate::system::runner::{CommandOutput, FakeRunner};

const MIB: u64 = 1024 * 1024;

const SNAP_LIST: &str = "\
Name                       Version                         Rev    Tracking         Publisher      Notes
bare                       1.0                             5      latest/stable    canonical**    base
code                       7debcd0e                        264    latest/stable    vscode**       disabled,classic
code                       07f806f9                        267    latest/stable    vscode**       classic
core20                     20260410                        2866   latest/stable    canonical**    base,disabled
core20                     20260901                        2922   latest/stable    canonical**    base
firefox                    157.0.1-1                       9036   latest/stable/…  mozilla**      -
firefox                    157.0-1                         8995   latest/stable/…  mozilla**      disabled
";
const JOURNAL_BIG: &str = "Archived and active journals take up 1.2G in the file system.\n";
const JOURNAL_SMALL: &str = "Archived and active journals take up 150.0M in the file system.\n";

fn ok(stdout: &str) -> CommandOutput {
    CommandOutput {
        success: true,
        code: Some(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

fn with_tools() -> Fixture {
    let mut fx = fixture();
    fx.env.tools.journalctl = Some(PathBuf::from("/usr/bin/journalctl"));
    fx.env.tools.snap = Some(PathBuf::from("/usr/bin/snap"));
    fx.env.tools.apt = Some(PathBuf::from("/usr/bin/apt"));
    fx
}

fn runner(journal: &str) -> FakeRunner {
    FakeRunner::new()
        .on(&["--disk-usage"], ok(journal))
        .on(&["list", "--all"], ok(SNAP_LIST))
}

fn guide_line(finding: &Finding) -> &str {
    match &finding.action {
        Action::Guide(GuideSpec { command_line }) => command_line,
        other => panic!("esperava Guide, veio {other:?}"),
    }
}

fn seed_system(fx: &Fixture) {
    let ctx = fx.ctx();
    let apt = &ctx.system.apt_archives;
    write_bytes(&apt.join("pacote_1.0_amd64.deb"), 2 * MIB as usize);
    write_bytes(&apt.join("outro_2.0_amd64.deb"), MIB as usize);
    write_bytes(&apt.join("lock"), 2 * MIB as usize);
    write_bytes(&apt.join("partial/baixando.deb"), 2 * MIB as usize);
    let crash = &ctx.system.crash_dir;
    write_bytes(&crash.join("_usr_bin_app.1000.crash"), 2 * MIB as usize);
    write_bytes(&crash.join("_usr_bin_outro.1000.crash"), MIB as usize);
    write_bytes(&crash.join("kexec_cmd"), 2 * MIB as usize);
    let snaps = &ctx.system.snaps_dir;
    write_bytes(&snaps.join("code_264.snap"), 2 * MIB as usize);
    write_bytes(&snaps.join("core20_2866.snap"), 3 * MIB as usize);
    write_bytes(&snaps.join("firefox_8995.snap"), 2 * MIB as usize);
    write_bytes(&snaps.join("code_267.snap"), 2 * MIB as usize);
}

fn by_command<'a>(findings: &'a [Finding], command: &str) -> Option<&'a Finding> {
    findings.iter().find(|f| guide_line(f) == command)
}

#[test]
fn parses_journal_usage() {
    assert_eq!(parse_journal_usage(JOURNAL_BIG), Some(1_288_490_189));
    assert_eq!(
        parse_journal_usage("Archived and active journals take up 512.0M in the file system."),
        Some(512 * MIB)
    );
    assert_eq!(
        parse_journal_usage("Journals take up 8.0K in the file system."),
        Some(8192)
    );
    assert_eq!(
        parse_journal_usage("Archived and active journals take up 2T in the file system."),
        Some(2 * 1024 * 1024 * MIB)
    );
    assert_eq!(parse_journal_usage("No journal files were found."), None);
    assert_eq!(
        parse_journal_usage("take up muito in the file system"),
        None
    );
}

#[test]
fn journal_under_limit_offers_nothing() {
    let fx = with_tools();
    let journal_runner = runner(JOURNAL_SMALL);
    let output = System
        .scan(&fx.ctx_with(&journal_runner, Vec::new()))
        .unwrap();
    assert!(
        output
            .findings
            .iter()
            .all(|f| !guide_line(f).contains("journalctl"))
    );
    let big_runner = runner(JOURNAL_BIG);
    let output = System.scan(&fx.ctx_with(&big_runner, Vec::new())).unwrap();
    let journal = by_command(&output.findings, "sudo journalctl --vacuum-size=200M").unwrap();
    assert_eq!(journal.bytes, 1_288_490_189 - 200 * MIB);
}

#[test]
fn parses_disabled_snaps() {
    assert_eq!(
        parse_snap_disabled(SNAP_LIST),
        [
            ("code".to_string(), "264".to_string()),
            ("core20".to_string(), "2866".to_string()),
            ("firefox".to_string(), "8995".to_string()),
        ]
    );
    assert!(parse_snap_disabled("").is_empty());
    assert!(parse_snap_disabled("No snaps are installed yet.\n").is_empty());
}

#[test]
fn snap_items_guide_remove_revision() {
    let fx = with_tools();
    seed_system(&fx);
    let snap_runner = runner(JOURNAL_SMALL);
    let output = System.scan(&fx.ctx_with(&snap_runner, Vec::new())).unwrap();
    let mut lines: Vec<&str> = output
        .findings
        .iter()
        .map(guide_line)
        .filter(|line| line.contains("snap remove"))
        .collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "sudo snap remove code --revision=264",
            "sudo snap remove core20 --revision=2866",
            "sudo snap remove firefox --revision=8995",
        ]
    );
    let core = by_command(&output.findings, "sudo snap remove core20 --revision=2866").unwrap();
    assert!(core.bytes >= 3 * MIB);
}

#[test]
fn apt_and_crash_sum_files() {
    let fx = with_tools();
    seed_system(&fx);
    let system_runner = runner(JOURNAL_SMALL);
    let output = System
        .scan(&fx.ctx_with(&system_runner, Vec::new()))
        .unwrap();
    let apt = by_command(&output.findings, "sudo apt-get clean").unwrap();
    assert!(apt.bytes >= 3 * MIB && apt.bytes < 5 * MIB, "{}", apt.bytes);
    let crash = by_command(&output.findings, "sudo rm -f /var/crash/*.crash").unwrap();
    assert!(
        crash.bytes >= 3 * MIB && crash.bytes < 5 * MIB,
        "{}",
        crash.bytes
    );
    for finding in &output.findings {
        assert_eq!(finding.risk, Risk::Guided);
        assert!(finding.fingerprint.is_none());
        assert!(finding.blocked.is_none());
        assert!(finding.action.path().is_none());
        assert!(
            finding
                .note
                .as_deref()
                .unwrap()
                .contains(guide_line(finding))
        );
    }
}

#[test]
fn missing_tools_are_skipped_not_errors() {
    let mut fx = with_tools();
    fx.env.tools.snap = None;
    seed_system(&fx);
    let system_runner = runner(JOURNAL_BIG);
    let output = System
        .scan(&fx.ctx_with(&system_runner, Vec::new()))
        .unwrap();
    let lines: Vec<&str> = output.findings.iter().map(guide_line).collect();
    assert!(lines.contains(&"sudo journalctl --vacuum-size=200M"));
    assert!(lines.contains(&"sudo apt-get clean"));
    assert!(lines.contains(&"sudo rm -f /var/crash/*.crash"));
    assert!(!lines.iter().any(|line| line.contains("snap")));
    assert!(output.footnote.unwrap().contains("snap"));
    assert!(system_runner.calls().iter().all(|call| call[0] != "list"));
}

#[test]
fn nothing_found_without_any_tool() {
    let fx = fixture();
    let output = System.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
    assert!(fx.runner.calls().is_empty());
}

struct NoTrash;

impl Discard for NoTrash {
    fn discard(&self, path: &Path) -> io::Result<()> {
        panic!("nada deveria ir para a Lixeira: {path:?}")
    }
}

#[test]
fn executor_never_runs_guided() {
    let fx = with_tools();
    seed_system(&fx);
    let scan_runner = runner(JOURNAL_BIG);
    let output = System.scan(&fx.ctx_with(&scan_runner, Vec::new())).unwrap();
    assert_eq!(output.findings.len(), 6);
    let mut ledger = Ledger::new(&fx.env, output.findings.clone());
    let request = CleanRequest {
        ids: output.findings.iter().map(|f| f.id.clone()).collect(),
        allow_irreversible: true,
    };
    let clean_runner = FakeRunner::new();
    let audit = AuditLog::open(&fx.env).unwrap();
    let roots_for = |_: &Finding| Vec::new();
    let watched_names_for = |_: &Finding| Vec::new();
    let exec = ExecContext {
        env: &fx.env,
        runner: &clean_runner,
        trash: &NoTrash,
        audit: &audit,
        roots_for: &roots_for,
        watched_names_for: &watched_names_for,
        in_use_scope_for: &|_| None,
    };
    let report = clean(
        &mut ledger,
        &request,
        &exec,
        &|_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(report.items.len(), 6);
    for (_, outcome) in &report.items {
        assert_eq!(outcome, &ItemOutcome::Skipped(SkipReason::GuidedOnly));
    }
    assert!(clean_runner.calls().is_empty());
}
