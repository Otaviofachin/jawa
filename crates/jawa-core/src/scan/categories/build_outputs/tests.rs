use std::fs;

use super::*;
use crate::safety::boundary::check_target;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};
use crate::system::git::test_repo::{commit_all, git, git_exe, init_repo};
use crate::system::runner::SystemRunner;

const TWO_MIB: usize = 2 * 1024 * 1024;

fn git_fixture() -> Fixture {
    let mut fx = fixture();
    fx.env.tools.git = Some(git_exe());
    fx
}

fn repo_with_ignored_builds(fx: &Fixture) -> PathBuf {
    let repo = fx.home.join("field/app");
    init_repo(&repo, 0);
    fs::write(repo.join(".gitignore"), "dist/\nwww/\n.angular/\n").unwrap();
    repo
}

fn names_of(output: &ScanOutput) -> Vec<String> {
    output
        .findings
        .iter()
        .map(|finding| finding.location.clone())
        .collect()
}

#[test]
fn offers_only_ignored_known_names() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    write_bytes(&repo.join("dist/bundle.js"), TWO_MIB);
    write_bytes(&repo.join("build/versionado.js"), TWO_MIB);
    write_bytes(&repo.join("client/.angular/cache/blob"), TWO_MIB);
    write_bytes(&repo.join("src/outro/blob"), TWO_MIB);
    commit_all(&repo, 0);
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(
        names_of(&output),
        vec!["~/field/app/dist", "~/field/app/client/.angular/cache"]
    );
    let dist = &output.findings[0];
    assert_eq!(dist.label, "app");
    assert_eq!(dist.risk, Risk::Safe);
    assert_eq!(dist.reversibility, Reversibility::Regenerable);
    assert!(dist.note.as_deref().unwrap().starts_with("dist: "));
    assert_eq!(output.findings[1].label, "app (client)");
}

#[test]
fn nested_clone_uses_its_own_ignore_rules() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    fs::write(repo.join(".gitignore"), "dist/\nvendor/\n").unwrap();
    write_bytes(&repo.join("dist/bundle.js"), TWO_MIB);
    let inner = repo.join("vendor/lib");
    init_repo(&inner, 0);
    write_bytes(&inner.join("dist/versionado.js"), TWO_MIB);
    commit_all(&inner, 0);
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(names_of(&output), vec!["~/field/app/dist"]);
    assert!(output.footnote.is_none(), "{:?}", output.footnote);
}

#[test]
fn submodule_does_not_hide_the_outer_repo() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    fs::write(repo.join(".gitignore"), "dist/\n").unwrap();
    commit_all(&repo, 0);
    write_bytes(&repo.join("dist/bundle.js"), TWO_MIB);
    let library = fx.home.join("fontes/lib");
    init_repo(&library, 0);
    write_bytes(&library.join("dist/versionado.js"), TWO_MIB);
    commit_all(&library, 0);
    let library_url = library.display().to_string();
    git(
        &repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &library_url,
            "sub",
        ],
        0,
    );
    commit_all(&repo, 0);
    assert!(repo.join("sub/.git").is_file());
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(names_of(&output), vec!["~/field/app/dist"]);
    assert!(output.footnote.is_none(), "{:?}", output.footnote);
}

#[test]
fn ignored_build_with_force_added_file_is_not_offered() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    fs::write(repo.join(".gitignore"), "build/\ndist/\n").unwrap();
    write_bytes(&repo.join("build/gerado.js"), TWO_MIB);
    write_bytes(&repo.join("build/config/forcado.json"), 10);
    write_bytes(&repo.join("dist/bundle.js"), TWO_MIB);
    git(&repo, &["add", "-f", "build/config/forcado.json"], 0);
    commit_all(&repo, 0);
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(names_of(&output), vec!["~/field/app/dist"]);
}

#[test]
fn signed_artifact_turns_review() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    fs::write(repo.join(".gitignore"), "build/\n").unwrap();
    write_bytes(
        &repo.join("android/app/build/outputs/APP-release.APK"),
        TWO_MIB,
    );
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), 1);
    let finding = &output.findings[0];
    assert_eq!(finding.risk, Risk::Review);
    assert!(finding.note.as_deref().unwrap().contains("APP-release.APK"));
}

#[test]
fn signed_artifact_ignores_symlinks_and_other_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("app.txt"), "x").unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("a.apk"), "x").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert_eq!(first_signed_artifact(dir.path()), None);
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/release.keystore"), "x").unwrap();
    assert_eq!(
        first_signed_artifact(dir.path()).as_deref(),
        Some("release.keystore")
    );
}

#[test]
fn needs_git_and_roots() {
    let mut fx = fixture();
    let roots = vec![fx.home.join("field")];
    let ctx = fx.ctx_with(&SystemRunner, roots);
    assert_eq!(
        BuildOutputs.scan(&ctx),
        Err(ScanProblem::ToolMissing("git"))
    );
    fx.env.tools.git = Some(git_exe());
    let ctx = fx.ctx_with(&SystemRunner, Vec::new());
    assert_eq!(BuildOutputs.scan(&ctx), Err(ScanProblem::NoProjects));
}

#[test]
fn items_pass_boundary() {
    let fx = git_fixture();
    let repo = repo_with_ignored_builds(&fx);
    write_bytes(&repo.join("dist/bundle.js"), TWO_MIB);
    let ctx = fx.ctx_with(&SystemRunner, vec![fx.home.join("field")]);
    let roots = BuildOutputs.roots(&ctx);
    let output = BuildOutputs.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), 1);
    let path = output.findings[0].action.path().unwrap();
    assert!(check_target(path, &roots, &fx.env).is_ok());
}
