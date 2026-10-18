use std::fs;
use std::os::unix::fs::symlink;

use super::*;
use crate::model::finding::{BlockReason, Reversibility, Risk};
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};
use crate::system::git::test_repo::{git_exe, init_repo};
use crate::system::runner::SystemRunner;

const TWO_MIB: usize = 2 * 1024 * 1024;

fn git_fixture() -> Fixture {
    let mut fx = fixture();
    fx.env.tools.git = Some(git_exe());
    fx
}

fn scan(fx: &Fixture) -> Result<ScanOutput, ScanProblem> {
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    RustTargets.scan(&ctx)
}

#[test]
fn rust_target_needs_manifest() {
    let fx = git_fixture();
    let repo = fx.home.join("field/com-cargo");
    init_repo(&repo, 0);
    fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&repo.join("target/debug/blob"), TWO_MIB);
    let without = fx.home.join("field/sem-cargo");
    init_repo(&without, 0);
    fs::write(without.join(".gitignore"), "target/\n").unwrap();
    write_bytes(&without.join("target/debug/blob"), TWO_MIB);
    let output = scan(&fx).unwrap();
    assert_eq!(output.findings.len(), 1);
    let finding = &output.findings[0];
    assert_eq!(finding.location, "~/field/com-cargo/target");
    assert_eq!(finding.label, "com-cargo");
    assert_eq!(finding.risk, Risk::Safe);
    assert_eq!(finding.reversibility, Reversibility::Regenerable);
}

#[test]
fn rust_target_finds_tauri_and_loose_projects() {
    let fx = git_fixture();
    let repo = fx.home.join("field/app");
    init_repo(&repo, 0);
    fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    fs::create_dir_all(repo.join("src-tauri")).unwrap();
    fs::write(repo.join("src-tauri/Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&repo.join("src-tauri/target/debug/blob"), TWO_MIB);
    let loose = fx.home.join("experimentos/cli");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&loose.join("target/debug/blob"), TWO_MIB);
    let output = scan(&fx).unwrap();
    let locations: Vec<&str> = output
        .findings
        .iter()
        .map(|finding| finding.location.as_str())
        .collect();
    assert_eq!(
        locations,
        vec!["~/field/app/src-tauri/target", "~/experimentos/cli/target"]
    );
    assert_eq!(output.findings[0].label, "app (src-tauri)");
}

#[test]
fn rust_target_inside_repo_needs_ignore() {
    let fx = git_fixture();
    let repo = fx.home.join("field/versionado");
    init_repo(&repo, 0);
    fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&repo.join("target/debug/blob"), TWO_MIB);
    assert!(scan(&fx).unwrap().findings.is_empty());
}

#[test]
fn rust_target_blocked_when_running() {
    let fx = git_fixture();
    let repo = fx.home.join("field/vivo");
    init_repo(&repo, 0);
    fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    fs::write(repo.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&repo.join("target/debug/app"), TWO_MIB);
    let process_dir = fx.env.proc_root.join("999");
    fs::create_dir_all(&process_dir).unwrap();
    symlink(repo.join("target/debug/app"), process_dir.join("exe")).unwrap();
    fs::write(process_dir.join("comm"), "app\n").unwrap();
    let output = scan(&fx).unwrap();
    assert!(matches!(
        output.findings[0].blocked,
        Some(BlockReason::InUse(_))
    ));
}

#[test]
fn rust_target_needs_git() {
    let fx = fixture();
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    assert_eq!(RustTargets.scan(&ctx), Err(ScanProblem::ToolMissing("git")));
}

#[test]
fn rust_target_scans_home_without_roots() {
    let fx = git_fixture();
    let loose = fx.home.join("experimentos/cli");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&loose.join("target/debug/blob"), TWO_MIB);
    let ctx = fx.ctx_with(&SystemRunner, Vec::new());
    let output = RustTargets.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), 1);
    assert_eq!(output.findings[0].location, "~/experimentos/cli/target");
}

#[test]
fn rust_target_ignores_symlinked_target() {
    let fx = git_fixture();
    let loose = fx.home.join("experimentos/cli");
    fs::create_dir_all(&loose).unwrap();
    fs::write(loose.join("Cargo.toml"), "[package]\n").unwrap();
    write_bytes(&fx.home.join("outro/blob"), TWO_MIB);
    symlink(fx.home.join("outro"), loose.join("target")).unwrap();
    let ctx = fx.ctx_with(&SystemRunner, Vec::new());
    assert!(RustTargets.scan(&ctx).unwrap().findings.is_empty());
}

#[test]
fn roots_include_home() {
    let fx = fixture();
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    assert_eq!(
        RustTargets.roots(&ctx),
        vec![fx.home.join("field"), fx.home.clone()]
    );
}
