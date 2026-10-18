use std::fs;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::clean::{CleanRequest, Discard, ExecContext, ItemOutcome, SkipReason, clean};
use crate::model::finding::BlockReason;
use crate::model::finding::{Action, GuideSpec, Reversibility, Risk};
use crate::safety::audit::AuditLog;
use crate::safety::boundary::{BoundaryViolation, check_target};
use crate::safety::ledger::Ledger;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};

const TWO_MIB: usize = 2 * 1024 * 1024;

fn seed_raw(fx: &Fixture) -> PathBuf {
    let raw = fx.home.join(".docker/desktop/vms/0/data/Docker.raw");
    write_bytes(&raw, TWO_MIB);
    raw
}

#[test]
fn desktop_installed_only_guides() {
    let fx = fixture();
    seed_raw(&fx);
    let ctx = fx.ctx();
    fs::create_dir_all(&ctx.system.docker_desktop_install).unwrap();
    let output = DockerDesktopDisk.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), 1);
    let finding = &output.findings[0];
    assert_eq!(
        finding.action,
        Action::Guide(GuideSpec {
            command_line: RECLAIM_COMMAND.to_string()
        })
    );
    assert_eq!(finding.risk, Risk::Guided);
    assert!(finding.bytes >= TWO_MIB as u64);
    assert_eq!(finding.location, "~/.docker/desktop/vms/0/data/Docker.raw");
    assert!(
        finding
            .note
            .as_deref()
            .unwrap()
            .contains("Nunca apague o Docker.raw com o Docker Desktop instalado")
    );
    assert!(output.findings.iter().all(|f| f.action.path().is_none()));
}

#[test]
fn desktop_size_is_real_not_apparent() {
    let fx = fixture();
    let raw = fx.home.join(".docker/desktop/vms/0/data/Docker.raw");
    fs::create_dir_all(raw.parent().unwrap()).unwrap();
    let file = fs::File::create(&raw).unwrap();
    file.set_len(64 * 1024 * 1024 * 1024).unwrap();
    let ctx = fx.ctx();
    fs::create_dir_all(&ctx.system.docker_desktop_install).unwrap();
    let output = DockerDesktopDisk.scan(&ctx).unwrap();
    assert!(output.findings.is_empty());
}

#[test]
fn desktop_removed_offers_review_delete() {
    let fx = fixture();
    seed_raw(&fx);
    let ctx = fx.ctx();
    let output = DockerDesktopDisk.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), 1);
    let finding = &output.findings[0];
    let vms = fx.home.join(".docker/desktop/vms");
    assert_eq!(finding.action, Action::Delete(vms.clone()));
    assert_eq!(finding.risk, Risk::Review);
    assert_eq!(finding.reversibility, Reversibility::Irreversible);
    assert!(finding.fingerprint.is_some());
    let roots = DockerDesktopDisk.roots(&ctx);
    assert_eq!(roots, [fx.home.join(".docker/desktop")]);
    assert!(check_target(&vms, &roots, &fx.env).is_ok());
}

#[test]
fn roots_empty_when_desktop_installed() {
    let fx = fixture();
    let ctx = fx.ctx();
    assert_eq!(DockerDesktopDisk.roots(&ctx).len(), 1);
    fs::create_dir_all(&ctx.system.docker_desktop_install).unwrap();
    assert!(DockerDesktopDisk.roots(&ctx).is_empty());
}

struct NoTrash;

impl Discard for NoTrash {
    fn discard(&self, path: &Path) -> std::io::Result<()> {
        panic!("nada deveria ir para a Lixeira: {path:?}")
    }
}

#[test]
fn delete_refused_when_desktop_installed_after_measuring() {
    let fx = fixture();
    seed_raw(&fx);
    let ctx = fx.ctx();
    let findings = DockerDesktopDisk.scan(&ctx).unwrap().findings;
    fs::create_dir_all(&ctx.system.docker_desktop_install).unwrap();
    let mut ledger = Ledger::new(&fx.env, findings.clone());
    let request = CleanRequest {
        ids: findings.iter().map(|f| f.id.clone()).collect(),
        allow_irreversible: true,
    };
    let audit = AuditLog::open(&fx.env).unwrap();
    let roots_for = |_: &Finding| DockerDesktopDisk.roots(&ctx);
    let watched_names_for = |f: &Finding| DockerDesktopDisk.watched_processes_for(f);
    let exec = ExecContext {
        env: &fx.env,
        runner: &fx.runner,
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
    assert_eq!(
        report.items[0].1,
        ItemOutcome::Skipped(SkipReason::Boundary(BoundaryViolation::OutsideRoots))
    );
    assert!(fx.home.join(".docker/desktop/vms").exists());
}

#[test]
fn removed_desktop_blocked_while_backend_runs() {
    let fx = fixture();
    seed_raw(&fx);
    let process_dir = fx.env.proc_root.join("321");
    fs::create_dir_all(&process_dir).unwrap();
    fs::write(process_dir.join("comm"), "com.docker.back\n").unwrap();
    let output = DockerDesktopDisk.scan(&fx.ctx()).unwrap();
    assert!(matches!(
        output.findings[0].blocked,
        Some(BlockReason::InUse(_))
    ));
}

#[test]
fn no_disk_no_items() {
    let fx = fixture();
    let output = DockerDesktopDisk.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
}
