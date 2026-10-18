use super::*;
use crate::model::finding::Action;
use crate::safety::boundary::check_target;
use crate::scan::categories::test_support::{fixture, write_bytes};

const TWO_MIB: usize = 2 * 1024 * 1024;

fn seeded() -> crate::scan::categories::test_support::Fixture {
    let fx = fixture();
    write_bytes(&fx.home.join(".npm/_cacache/content/blob"), TWO_MIB);
    write_bytes(&fx.home.join(".npm/_npx/abc/blob"), TWO_MIB);
    fx
}

#[test]
fn npm_cache_finds_cacache_and_npx() {
    let fx = seeded();
    let output = NpmCache.scan(&fx.ctx()).unwrap();
    assert_eq!(output.findings.len(), 2);
    for finding in &output.findings {
        assert!(matches!(finding.action, Action::Delete(_)));
        assert_eq!(finding.risk, Risk::Safe);
        assert_eq!(finding.reversibility, Reversibility::Regenerable);
        assert_eq!(finding.category, CategoryId::NpmCache);
    }
    assert_eq!(output.findings[0].location, "~/.npm/_cacache");
    assert_eq!(output.findings[1].location, "~/.npm/_npx");
}

#[test]
fn npm_cache_reports_unreadable() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let fx = seeded();
    let cacache = fx.home.join(".npm/_cacache");
    fs::set_permissions(&cacache, fs::Permissions::from_mode(0o000)).unwrap();
    let partial = NpmCache.scan(&fx.ctx());
    let npx = fx.home.join(".npm/_npx");
    fs::set_permissions(&npx, fs::Permissions::from_mode(0o000)).unwrap();
    let total = NpmCache.scan(&fx.ctx());
    fs::set_permissions(&cacache, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&npx, fs::Permissions::from_mode(0o755)).unwrap();
    let output = partial.unwrap();
    assert_eq!(output.findings.len(), 1);
    assert_eq!(
        output.footnote.as_deref(),
        Some("1 caminho(s) não puderam ser lidos.")
    );
    assert_eq!(
        total,
        Err(ScanProblem::NoAccess(cacache.display().to_string()))
    );
}

#[test]
fn npm_cache_skips_small() {
    let fx = fixture();
    write_bytes(&fx.home.join(".npm/_cacache/blob"), 1024);
    let output = NpmCache.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
}

#[test]
fn npm_cache_items_pass_boundary() {
    let fx = seeded();
    let ctx = fx.ctx();
    let roots = NpmCache.roots(&ctx);
    let output = NpmCache.scan(&ctx).unwrap();
    for finding in &output.findings {
        let path = finding.action.path().unwrap();
        assert!(check_target(path, &roots, &fx.env).is_ok());
    }
}
