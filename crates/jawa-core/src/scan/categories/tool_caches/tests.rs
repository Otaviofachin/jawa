use std::os::unix::fs::symlink;

use super::*;
use crate::safety::boundary::check_target;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};

const TWO_MIB: usize = 2 * 1024 * 1024;

fn labels(output: &ScanOutput) -> Vec<String> {
    output.findings.iter().map(|f| f.label.clone()).collect()
}

fn seed_claude_versions(fx: &Fixture) -> PathBuf {
    let versions = fx.home.join(".local/share/claude/versions");
    write_bytes(&versions.join("2.1.300"), TWO_MIB);
    write_bytes(&versions.join("2.1.302"), TWO_MIB);
    versions
}

#[test]
fn finds_existing_tool_caches() {
    let fx = fixture();
    write_bytes(&fx.home.join(".cache/yarn/v6/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".cache/ms-playwright/chromium/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".gradle/caches/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".cargo/registry/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".cache/pip/pequeno"), 1024);
    write_bytes(&fx.home.join(".cache/desconhecido/blob"), TWO_MIB);
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    let mut locations: Vec<&str> = output
        .findings
        .iter()
        .map(|f| f.location.as_str())
        .collect();
    locations.sort_unstable();
    assert_eq!(
        locations,
        [
            "~/.cache/ms-playwright",
            "~/.cache/yarn",
            "~/.cargo/registry",
            "~/.gradle/caches"
        ]
    );
    for finding in &output.findings {
        assert_eq!(finding.risk, Risk::Safe);
        assert_eq!(finding.reversibility, Reversibility::Regenerable);
        assert!(finding.note.is_some());
    }
}

#[test]
fn keeps_claude_version_in_use() {
    let fx = fixture();
    let versions = seed_claude_versions(&fx);
    let launcher = fx.home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink(versions.join("2.1.302"), &launcher).unwrap();
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(labels(&output), ["Claude Code 2.1.300"]);
    assert!(output.footnote.is_none());
}

#[test]
fn keeps_version_behind_relative_launcher_chain() {
    let fx = fixture();
    let versions = seed_claude_versions(&fx);
    let current = fx.home.join(".local/share/claude/current");
    symlink("versions/2.1.300", &current).unwrap();
    let launcher = fx.home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink("../share/claude/current", &launcher).unwrap();
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(labels(&output), ["Claude Code 2.1.302"]);
    assert!(versions.join("2.1.300").exists());
}

fn link_launcher(fx: &Fixture, target: &Path) {
    let launcher = fx.home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink(target, &launcher).unwrap();
}

#[test]
fn keeps_claude_version_when_launcher_points_inside_version_dir() {
    let fx = fixture();
    let versions = fx.home.join(".local/share/claude/versions");
    write_bytes(&versions.join("2.1.300/bin/claude"), TWO_MIB);
    write_bytes(&versions.join("2.1.302/bin/claude"), TWO_MIB);
    link_launcher(&fx, &versions.join("2.1.302/bin/claude"));
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    assert_eq!(labels(&output), ["Claude Code 2.1.300"]);
}

#[test]
fn launcher_outside_versions_offers_no_version() {
    let fx = fixture();
    seed_claude_versions(&fx);
    let elsewhere = fx.home.join("opt/claude/claude");
    write_bytes(&elsewhere, 10);
    link_launcher(&fx, &elsewhere);
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
    assert!(output.footnote.unwrap().contains("Claude Code"));
}

#[test]
fn no_claude_versions_when_launcher_unknown() {
    let fx = fixture();
    seed_claude_versions(&fx);
    let output = ToolCaches.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
    assert!(output.footnote.unwrap().contains("Claude Code"));
    let launcher = fx.home.join(".local/bin/claude");
    write_bytes(&launcher, 10);
    let regular_file = ToolCaches.scan(&fx.ctx()).unwrap();
    assert!(regular_file.findings.is_empty());
}

#[test]
fn every_item_passes_boundary() {
    let fx = fixture();
    for cache in &TOOL_CACHES {
        write_bytes(&cache.path(&fx.env).join("blob"), TWO_MIB);
    }
    let versions = seed_claude_versions(&fx);
    let launcher = fx.home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    symlink(versions.join("2.1.302"), &launcher).unwrap();
    let ctx = fx.ctx();
    let roots = ToolCaches.roots(&ctx);
    let output = ToolCaches.scan(&ctx).unwrap();
    assert_eq!(output.findings.len(), TOOL_CACHES.len() + 1);
    for finding in &output.findings {
        let path = finding.action.path().unwrap();
        assert!(check_target(path, &roots, &fx.env).is_ok(), "{path:?}");
    }
}
