use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::CoreError;
use crate::model::finding::{
    Action, CommandSpec, Finding, FindingId, Reversibility, fingerprint_of,
};
use crate::safety::audit::AuditLog;
use crate::safety::boundary::BoundaryViolation;
use crate::safety::ledger::Ledger;
use crate::system::env::Environment;
use crate::system::processes::Holder;
use crate::system::runner::{CommandOutput, CommandRunner};

use super::*;
use crate::model::finding::{CategoryId, GuideSpec, Risk};
use crate::system::runner::FakeRunner;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::sync::Mutex;
use std::time::Duration;

const CATEGORY: CategoryId = CategoryId::System;
const FAKE_PID: &str = "9999999";
const FREE_SPACE_SLACK: u64 = 256 * 1024 * 1024;

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
    env: Environment,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    let root = home.join("projects");
    fs::create_dir_all(&root).unwrap();
    let env = Environment::for_tests(&home);
    fs::create_dir_all(&env.proc_root).unwrap();
    Fixture {
        _dir: dir,
        home,
        root,
        env,
    }
}

fn make_dir(path: &Path) -> PathBuf {
    fs::create_dir_all(path.join("sub")).unwrap();
    fs::write(path.join("sub/arquivo.bin"), vec![1u8; 8192]).unwrap();
    path.to_path_buf()
}

fn finding(action: Action, reversibility: Reversibility) -> Finding {
    let fingerprint = action.path().map(|path| fingerprint_of(path).unwrap());
    let location = match &action {
        Action::Delete(path) | Action::Trash(path) => path.display().to_string(),
        Action::Run(spec) => spec.args.join(" "),
        Action::Guide(guide) => guide.command_line.clone(),
    };
    Finding {
        id: FindingId::of(CATEGORY, &action),
        category: CATEGORY,
        label: location.clone(),
        location,
        bytes: 2_000_000,
        risk: Risk::Safe,
        reversibility,
        note: None,
        action,
        fingerprint,
        blocked: None,
    }
}

fn delete_item(path: &Path) -> Finding {
    finding(
        Action::Delete(path.to_path_buf()),
        Reversibility::Regenerable,
    )
}

fn guide_item() -> Finding {
    let guide = GuideSpec {
        command_line: "sudo apt-get clean".to_string(),
    };
    finding(Action::Guide(guide), Reversibility::Irreversible)
}

fn run_item(args: &[&str]) -> Finding {
    let spec = CommandSpec {
        program: PathBuf::from("/usr/bin/docker"),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        stdin: None,
        env: Vec::new(),
        timeout: Duration::from_secs(5),
    };
    finding(Action::Run(spec), Reversibility::Regenerable)
}

fn ids(findings: &[&Finding]) -> Vec<FindingId> {
    findings.iter().map(|finding| finding.id.clone()).collect()
}

fn request(findings: &[&Finding]) -> CleanRequest {
    CleanRequest {
        ids: ids(findings),
        allow_irreversible: false,
    }
}

#[derive(Default)]
struct RecordingTrash {
    paths: Mutex<Vec<PathBuf>>,
}

impl Discard for RecordingTrash {
    fn discard(&self, path: &Path) -> io::Result<()> {
        self.paths.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

struct Harness<'a> {
    runner: &'a dyn CommandRunner,
    trash: &'a dyn Discard,
    watched: Vec<&'static str>,
    on_progress: &'a dyn Fn(&Progress),
    cancel: &'a AtomicBool,
}

fn run_clean(
    fx: &Fixture,
    ledger: &mut Ledger,
    clean_request: &CleanRequest,
    harness: &Harness,
) -> Result<CleanReport, CoreError> {
    let audit = AuditLog::open(&fx.env).unwrap();
    clean_with_audit(fx, ledger, clean_request, harness, &audit)
}

fn clean_with_audit(
    fx: &Fixture,
    ledger: &mut Ledger,
    clean_request: &CleanRequest,
    harness: &Harness,
    audit: &AuditLog,
) -> Result<CleanReport, CoreError> {
    let roots = vec![fx.root.clone()];
    let roots_for = move |_: &Finding| roots.clone();
    let watched = harness.watched.clone();
    let watched_names_for = move |_: &Finding| watched.clone();
    let ctx = ExecContext {
        env: &fx.env,
        runner: harness.runner,
        trash: harness.trash,
        audit,
        roots_for: &roots_for,
        watched_names_for: &watched_names_for,
        in_use_scope_for: &|_| None,
    };
    clean(
        ledger,
        clean_request,
        &ctx,
        harness.on_progress,
        harness.cancel,
    )
}

fn simple_clean(
    fx: &Fixture,
    ledger: &mut Ledger,
    clean_request: &CleanRequest,
) -> Result<CleanReport, CoreError> {
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    run_clean(fx, ledger, clean_request, &harness)
}

fn saved_ledger(fx: &Fixture, findings: Vec<Finding>) -> Ledger {
    let ledger = Ledger::new(&fx.env, findings);
    ledger.save(&fx.env).unwrap();
    ledger
}

fn audit_entries(fx: &Fixture) -> Vec<crate::safety::audit::AuditEntry> {
    AuditLog::open(&fx.env).unwrap().recent(100)
}

fn only_outcome(report: &CleanReport) -> &ItemOutcome {
    assert_eq!(report.items.len(), 1);
    &report.items[0].1
}

fn add_fake_process(fx: &Fixture, cwd: &Path, comm: &str) {
    let process_dir = fx.env.proc_root.join(FAKE_PID);
    fs::create_dir_all(&process_dir).unwrap();
    symlink(cwd, process_dir.join("cwd")).unwrap();
    fs::write(process_dir.join("comm"), format!("{comm}\n")).unwrap();
}

#[test]
fn unknown_id_aborts_everything() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let real = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![real.clone()]);
    let mut clean_request = request(&[&real]);
    clean_request.ids.push("0000000000".parse().unwrap());
    let result = simple_clean(&fx, &mut ledger, &clean_request);
    assert!(matches!(result, Err(CoreError::UnknownId(id)) if id.to_string() == "0000000000"));
    assert!(target.exists());
    assert!(audit_entries(&fx).is_empty());
    assert_eq!(Ledger::load_fresh(&fx.env).unwrap().findings, vec![real]);
}

#[test]
fn deletes_selected_only() {
    let fx = fixture();
    let first = delete_item(&make_dir(&fx.root.join("a")));
    let second = delete_item(&make_dir(&fx.root.join("b")));
    let mut ledger = saved_ledger(&fx, vec![first.clone(), second.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&first])).unwrap();
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert!(!fx.root.join("a").exists());
    assert!(fx.root.join("b/sub/arquivo.bin").exists());
    assert_eq!(Ledger::load_fresh(&fx.env).unwrap().findings, vec![second]);
    assert_eq!(report.estimated_bytes, first.bytes);
}

#[test]
fn deletes_single_file() {
    let fx = fixture();
    let file = fx.root.join("grande.iso");
    fs::write(&file, vec![0u8; 4096]).unwrap();
    let item = delete_item(&file);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert!(!file.exists());
}

#[test]
fn deletes_symlink_not_its_target() {
    let fx = fixture();
    let outside = make_dir(&fx.home.join("fora"));
    let link = fx.root.join("link");
    symlink(&outside, &link).unwrap();
    let item = delete_item(&link);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert!(fs::symlink_metadata(&link).is_err());
    assert!(outside.join("sub/arquivo.bin").exists());
}

#[test]
fn skips_guided() {
    let fx = fixture();
    let item = guide_item();
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let mut clean_request = request(&[&item]);
    clean_request.allow_irreversible = true;
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    let report = run_clean(&fx, &mut ledger, &clean_request, &harness).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::GuidedOnly)
    );
    assert!(runner.calls().is_empty());
    assert_eq!(Ledger::load_fresh(&fx.env).unwrap().findings, vec![item]);
}

#[test]
fn requires_irreversible_confirmation() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("volume"));
    let item = finding(Action::Delete(target.clone()), Reversibility::Irreversible);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::IrreversibleNotConfirmed)
    );
    assert!(target.exists());
    let mut confirmed = request(&[&item]);
    confirmed.allow_irreversible = true;
    let report = simple_clean(&fx, &mut ledger, &confirmed).unwrap();
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert!(!target.exists());
}

#[test]
fn skips_changed_target() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    fs::rename(&target, fx.home.join("antiga")).unwrap();
    make_dir(&target);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::Changed)
    );
    assert!(target.join("sub/arquivo.bin").exists());
    assert!(Ledger::load_fresh(&fx.env).unwrap().findings.is_empty());
}

#[test]
fn vanished_target_is_changed() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    fs::remove_dir_all(&target).unwrap();
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::Changed)
    );
}

#[test]
fn missing_fingerprint_is_changed() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let mut item = delete_item(&target);
    item.fingerprint = None;
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::Changed)
    );
    assert!(target.exists());
}

#[test]
fn skips_target_in_use() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    add_fake_process(&fx, &target.join("sub"), "node");
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    let expected = vec![Holder {
        pid: FAKE_PID.parse().unwrap(),
        name: "node".to_string(),
    }];
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::InUse(expected))
    );
    assert!(target.exists());
    assert_eq!(Ledger::load_fresh(&fx.env).unwrap().findings, vec![item]);
}

#[test]
fn skips_while_watched_app_runs() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("cache-do-slack"));
    add_fake_process(&fx, &fx.home, "slack");
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: vec!["slack"],
        on_progress: &|_| {},
        cancel: &cancel,
    };
    let report = run_clean(&fx, &mut ledger, &request(&[&item]), &harness).unwrap();
    assert!(matches!(
        only_outcome(&report),
        ItemOutcome::Skipped(SkipReason::InUse(holders)) if holders[0].name == "slack"
    ));
    assert!(target.exists());
}

#[test]
fn skips_foreign_owned() {
    let mut fx = fixture();
    fx.env.uid += 1;
    let target = make_dir(&fx.root.join("a"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    let ItemOutcome::Skipped(SkipReason::ForeignOwned { fix }) = only_outcome(&report) else {
        panic!("esperava ForeignOwned: {report:?}");
    };
    assert!(fix.starts_with("sudo chown -R"), "{fix}");
    assert!(fix.contains(&target.display().to_string()), "{fix}");
    assert!(target.join("sub/arquivo.bin").exists());
}

#[test]
fn chown_fix_quotes_single_quotes() {
    assert_eq!(
        chown_fix(Path::new("/x/it's")),
        r#"sudo chown -R "$USER": '/x/it'\''s'"#
    );
}

#[test]
fn refuses_outside_roots() {
    let fx = fixture();
    let target = make_dir(&fx.home.join("outra/coisa"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::Boundary(BoundaryViolation::OutsideRoots))
    );
    assert!(target.exists());
}

#[test]
fn refuses_category_root_itself() {
    let fx = fixture();
    make_dir(&fx.root.join("a"));
    let item = delete_item(&fx.root);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Skipped(SkipReason::Boundary(BoundaryViolation::IsRoot))
    );
    assert!(fx.root.join("a").exists());
}

#[test]
fn trash_action_uses_discard() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("download.iso"));
    let item = finding(Action::Trash(target.clone()), Reversibility::Trash);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    let report = run_clean(&fx, &mut ledger, &request(&[&item]), &harness).unwrap();
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert_eq!(*trash.paths.lock().unwrap(), vec![target.clone()]);
    assert!(target.exists());
}

#[test]
fn cancel_stops_before_next_item() {
    let fx = fixture();
    let items: Vec<Finding> = ["a", "b", "c"]
        .iter()
        .map(|name| delete_item(&make_dir(&fx.root.join(name))))
        .collect();
    let mut ledger = saved_ledger(&fx, items.clone());
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let set_cancel = |progress: &Progress| {
        if progress.current.is_some() {
            cancel.store(true, Ordering::SeqCst);
        }
    };
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &set_cancel,
        cancel: &cancel,
    };
    let all: Vec<&Finding> = items.iter().collect();
    let report = run_clean(&fx, &mut ledger, &request(&all), &harness).unwrap();
    let outcomes: Vec<&ItemOutcome> = report.items.iter().map(|(_, outcome)| outcome).collect();
    assert_eq!(
        outcomes,
        vec![
            &ItemOutcome::Done { output: None },
            &ItemOutcome::Skipped(SkipReason::Cancelled),
            &ItemOutcome::Skipped(SkipReason::Cancelled),
        ]
    );
    assert!(!fx.root.join("a").exists());
    assert!(fx.root.join("b").exists() && fx.root.join("c").exists());
    let saved = Ledger::load_fresh(&fx.env).unwrap();
    assert_eq!(saved.findings, items[1..].to_vec());
}

#[test]
fn progress_reports_each_item_and_the_end() {
    let fx = fixture();
    let first = delete_item(&make_dir(&fx.root.join("a")));
    let second = guide_item();
    let mut ledger = saved_ledger(&fx, vec![first.clone(), second.clone()]);
    let seen = Mutex::new(Vec::new());
    let record = |progress: &Progress| seen.lock().unwrap().push(progress.clone());
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &record,
        cancel: &cancel,
    };
    run_clean(&fx, &mut ledger, &request(&[&first, &second]), &harness).unwrap();
    let seen = seen.into_inner().unwrap();
    let summary: Vec<(usize, usize, bool)> = seen
        .iter()
        .map(|progress| (progress.done, progress.total, progress.current.is_some()))
        .collect();
    assert_eq!(summary, vec![(0, 2, true), (1, 2, true), (2, 2, false)]);
}

#[test]
fn run_action_reports_tail_of_stderr() {
    let fx = fixture();
    let item = run_item(&["volume", "rm", "dados"]);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let stderr = (1..=8).map(|line| format!("linha {line}\n")).collect();
    let runner = FakeRunner::new().on(
        &["volume", "rm"],
        CommandOutput {
            success: false,
            code: Some(1),
            stdout: String::new(),
            stderr,
        },
    );
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    let report = run_clean(&fx, &mut ledger, &request(&[&item]), &harness).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Failed("linha 4\nlinha 5\nlinha 6\nlinha 7\nlinha 8".to_string())
    );
    assert_eq!(runner.calls(), vec![vec!["volume", "rm", "dados"]]);
    assert!(Ledger::load_fresh(&fx.env).unwrap().findings.is_empty());
}

#[test]
fn run_action_success_keeps_output() {
    let fx = fixture();
    let item = run_item(&["image", "prune", "-a", "-f"]);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let runner = FakeRunner::new().on(
        &["image", "prune"],
        CommandOutput {
            success: true,
            code: Some(0),
            stdout: "Total reclaimed space: 2GB\n".to_string(),
            stderr: String::new(),
        },
    );
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    let report = run_clean(&fx, &mut ledger, &request(&[&item]), &harness).unwrap();
    assert_eq!(
        only_outcome(&report),
        &ItemOutcome::Done {
            output: Some("Total reclaimed space: 2GB".to_string())
        }
    );
}

#[test]
fn records_audit_for_every_item() {
    let fx = fixture();
    let deleted = delete_item(&make_dir(&fx.root.join("a")));
    let guided = guide_item();
    let failing = run_item(&["volume", "rm", "x"]);
    let all = vec![deleted.clone(), guided.clone(), failing.clone()];
    let mut ledger = saved_ledger(&fx, all.clone());
    simple_clean(&fx, &mut ledger, &request(&[&deleted, &guided, &failing])).unwrap();
    let entries = audit_entries(&fx);
    let summary: Vec<(FindingId, String)> = entries
        .iter()
        .map(|entry| (entry.id.clone(), entry.outcome.clone()))
        .collect();
    assert_eq!(summary.len(), 3);
    assert_eq!(summary[0], (deleted.id.clone(), "feito".to_string()));
    assert!(summary[1].1.starts_with("pulado: modo guiado"));
    assert!(summary[2].1.starts_with("falhou"));
    assert_eq!(entries[0].category, "sistema");
    assert_eq!(entries[0].location, deleted.location);
    assert_eq!(entries[0].bytes, deleted.bytes);
    assert!(entries[0].at_secs > 0);
}

#[test]
fn report_has_free_before_and_after() {
    let fx = fixture();
    let item = delete_item(&make_dir(&fx.root.join("a")));
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    let stats = rustix::fs::statvfs(&fx.home).unwrap();
    let capacity = stats.f_blocks * stats.f_frsize;
    assert!(report.free_before > 0 && report.free_before <= capacity);
    assert!(report.free_after > 0 && report.free_after <= capacity);
    assert!(
        report.free_after + FREE_SPACE_SLACK >= report.free_before,
        "{report:?}"
    );
}

fn with_default_harness<T>(run: impl FnOnce(&Harness) -> T) -> T {
    let runner = FakeRunner::new();
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    run(&harness)
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn audit_failure_is_reported_in_warnings() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let audit = AuditLog::open(&fx.env).unwrap();
    let history = fx.env.state_dir.join("jawa/historico.jsonl");
    set_mode(&history, 0o444);
    let report = with_default_harness(|harness| {
        clean_with_audit(&fx, &mut ledger, &request(&[&item]), harness, &audit)
    })
    .unwrap();
    set_mode(&history, 0o644);
    assert_eq!(only_outcome(&report), &ItemOutcome::Done { output: None });
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("histórico"),
        "{:?}",
        report.warnings
    );
}

#[test]
fn ledger_save_failure_is_reported_in_warnings() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("a"));
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let audit = AuditLog::open(&fx.env).unwrap();
    let state = fx.env.state_dir.join("jawa");
    set_mode(&state, 0o555);
    let report = with_default_harness(|harness| {
        clean_with_audit(&fx, &mut ledger, &request(&[&item]), harness, &audit)
    });
    set_mode(&state, 0o755);
    let report = report.unwrap();
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("medição"),
        "{:?}",
        report.warnings
    );
    assert_eq!(audit.recent(10).len(), 1);
}

#[test]
fn clean_without_problems_has_no_warnings() {
    let fx = fixture();
    let item = delete_item(&make_dir(&fx.root.join("a")));
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn unreadable_target_is_unmeasurable() {
    let fx = fixture();
    let target = make_dir(&fx.root.join("trancada"));
    set_mode(&target, 0o000);
    let item = delete_item(&target);
    let mut ledger = saved_ledger(&fx, vec![item.clone()]);
    let report = simple_clean(&fx, &mut ledger, &request(&[&item]));
    set_mode(&target, 0o755);
    let report = report.unwrap();
    let ItemOutcome::Skipped(SkipReason::Unmeasurable(message)) = only_outcome(&report) else {
        panic!("esperava Unmeasurable: {report:?}");
    };
    assert!(!message.is_empty());
    assert!(
        SkipReason::Unmeasurable(message.clone())
            .to_string()
            .starts_with("não foi possível medir o item: ")
    );
    assert!(target.join("sub/arquivo.bin").exists());
}

#[test]
fn audit_uses_environment_clock() {
    let mut fx = fixture();
    fx.env.now = std::time::UNIX_EPOCH + Duration::from_secs(1_234_567);
    let item = guide_item();
    let mut ledger = Ledger::new(&fx.env, vec![item.clone()]);
    simple_clean(&fx, &mut ledger, &request(&[&item])).unwrap();
    assert_eq!(audit_entries(&fx)[0].at_secs, 1_234_567);
}

#[test]
fn audit_time_is_per_item() {
    let mut fx = fixture();
    fx.env.now = std::time::UNIX_EPOCH + Duration::from_secs(1_000);
    let slow = run_item(&["builder", "prune", "-a", "-f"]);
    let after = guide_item();
    let mut ledger = saved_ledger(&fx, vec![slow.clone(), after.clone()]);
    let ok = CommandOutput {
        success: true,
        code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
    };
    let runner = FakeRunner::new()
        .on(&["builder"], ok)
        .delay(Duration::from_millis(1_100));
    let trash = RecordingTrash::default();
    let cancel = AtomicBool::new(false);
    let harness = Harness {
        runner: &runner,
        trash: &trash,
        watched: Vec::new(),
        on_progress: &|_| {},
        cancel: &cancel,
    };
    run_clean(&fx, &mut ledger, &request(&[&slow, &after]), &harness).unwrap();
    let times: Vec<u64> = audit_entries(&fx)
        .iter()
        .map(|entry| entry.at_secs)
        .collect();
    assert_eq!(times, vec![1_001, 1_001]);
}

#[test]
fn follows_ledger_order_and_ignores_duplicates() {
    let fx = fixture();
    let first = delete_item(&make_dir(&fx.root.join("a")));
    let second = delete_item(&make_dir(&fx.root.join("b")));
    let mut ledger = saved_ledger(&fx, vec![first.clone(), second.clone()]);
    let clean_request = request(&[&second, &first, &second]);
    let report = simple_clean(&fx, &mut ledger, &clean_request).unwrap();
    let order: Vec<FindingId> = report
        .items
        .iter()
        .map(|(item, _)| item.id.clone())
        .collect();
    assert_eq!(order, vec![first.id, second.id]);
}

fn report_with(estimated: u64, before: u64, after: u64) -> CleanReport {
    CleanReport {
        items: Vec::new(),
        estimated_bytes: estimated,
        free_before: before,
        free_after: after,
        warnings: Vec::new(),
    }
}

#[test]
fn measured_is_the_free_space_difference() {
    assert_eq!(report_with(10, 100, 130).measured_bytes(), 30);
    assert_eq!(report_with(10, 100, 90).measured_bytes(), 0);
}

#[test]
fn stuck_space_below_half_of_estimate() {
    assert!(report_with(100, 0, 49).space_may_be_stuck());
    assert!(!report_with(100, 0, 50).space_may_be_stuck());
    assert!(!report_with(0, 0, 0).space_may_be_stuck());
}

#[test]
fn report_splits_trash_and_docker_desktop_bytes() {
    let fx = fixture();
    let trashed = finding(
        Action::Trash(make_dir(&fx.root.join("t"))),
        Reversibility::Trash,
    );
    let desktop = |args: &[&str]| {
        let mut item = run_item(args);
        if let Action::Run(spec) = &mut item.action {
            spec.env
                .push(("DOCKER_CONTEXT".to_string(), "desktop-linux".to_string()));
        }
        item
    };
    let done = || ItemOutcome::Done { output: None };
    let report = CleanReport {
        items: vec![
            (trashed, done()),
            (desktop(&["image", "prune", "-a", "-f"]), done()),
            (run_item(&["image", "prune", "-a", "-f"]), done()),
            (
                desktop(&["builder", "prune", "-a", "-f"]),
                ItemOutcome::Skipped(SkipReason::Cancelled),
            ),
        ],
        ..report_with(8_000_000, 0, 1_000_000)
    };
    assert_eq!(report.trashed_bytes(), 2_000_000);
    assert_eq!(report.docker_desktop_bytes(), 2_000_000);
    assert_eq!(report.on_disk_estimate(), 4_000_000);
    assert!(report.space_may_be_stuck());
}
