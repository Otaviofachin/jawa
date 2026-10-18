use crate::model::finding::{Action, FindingId, Reversibility, Risk};
use crate::scan::paths::*;
use crate::scan::*;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use super::test_support::{fixture, write_bytes};
use super::*;
use crate::model::finding::BlockReason;

const TWO_MIB: usize = 2 * 1024 * 1024;

#[test]
fn ids_parse_and_display_round_trip() {
    for id in CategoryId::ALL {
        assert_eq!(id.to_string().parse::<CategoryId>().unwrap(), id);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<CategoryId>(&json).unwrap(), id);
    }
    assert_eq!("npm-cache".parse(), Ok(CategoryId::NpmCache));
    assert!("nada".parse::<CategoryId>().is_err());
    assert!(serde_json::from_str::<CategoryId>("\"nada\"").is_err());
}

#[test]
fn every_category_explains_itself() {
    assert!(!registry().is_empty());
    for category in registry() {
        let descriptor = category.descriptor();
        let explanation = &descriptor.explanation;
        let fields = [
            descriptor.title,
            descriptor.summary,
            explanation.what_it_is,
            explanation.why_it_grows,
            explanation.effect,
            explanation.reversibility_detail,
            explanation.recreate_cost,
            explanation.recommendation,
        ];
        assert!(fields.iter().all(|field| !field.trim().is_empty()));
        assert_eq!(
            super::category(descriptor.id).descriptor().id,
            descriptor.id
        );
    }
}

#[test]
fn system_paths_under_a_root() {
    let paths = SystemPaths::under(Path::new("/tmp/raiz"));
    assert_eq!(
        paths.docker_desktop_install,
        PathBuf::from("/tmp/raiz/opt/docker-desktop")
    );
    assert_eq!(
        paths.snaps_dir,
        PathBuf::from("/tmp/raiz/var/lib/snapd/snaps")
    );
    assert_eq!(
        paths.apt_archives,
        PathBuf::from("/tmp/raiz/var/cache/apt/archives")
    );
    assert_eq!(paths.crash_dir, PathBuf::from("/tmp/raiz/var/crash"));
    assert_eq!(
        SystemPaths::default().docker_desktop_install,
        PathBuf::from("/opt/docker-desktop")
    );
    assert_eq!(
        SystemPaths::default().crash_dir,
        PathBuf::from("/var/crash")
    );
}

#[test]
fn registry_has_every_category_in_spec_order() {
    let ids: Vec<CategoryId> = registry().iter().map(|c| c.descriptor().id).collect();
    assert_eq!(ids, CategoryId::ALL);
}

#[test]
fn registry_ids_are_unique() {
    let mut ids: Vec<CategoryId> = registry().iter().map(|c| c.descriptor().id).collect();
    let total = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), total);
}

#[test]
fn path_finding_marks_in_use() {
    let fx = fixture();
    let target = fx.home.join("projeto/dist");
    write_bytes(&target.join("bundle.js"), TWO_MIB);
    let process_dir = fx.env.proc_root.join("4242");
    fs::create_dir_all(&process_dir).unwrap();
    symlink(&target, process_dir.join("cwd")).unwrap();
    fs::write(process_dir.join("comm"), "node\n").unwrap();
    let ctx = fx.ctx();
    let finding = path_finding(&ctx, &[], item(&target)).unwrap().unwrap();
    let Some(BlockReason::InUse(holders)) = finding.blocked else {
        panic!("esperava InUse, veio {:?}", finding.blocked);
    };
    assert_eq!(holders[0].pid, 4242);
}

#[test]
fn path_finding_marks_watched_process() {
    let fx = fixture();
    let target = fx.home.join(".cache/app");
    write_bytes(&target.join("blob"), TWO_MIB);
    let process_dir = fx.env.proc_root.join("77");
    fs::create_dir_all(&process_dir).unwrap();
    fs::write(process_dir.join("comm"), "chrome\n").unwrap();
    let ctx = fx.ctx();
    let finding = path_finding(&ctx, &["chrome"], item(&target))
        .unwrap()
        .unwrap();
    assert!(matches!(finding.blocked, Some(BlockReason::InUse(_))));
}

#[test]
fn path_finding_fills_the_finding() {
    let fx = fixture();
    let target = fx.home.join(".npm/_cacache");
    write_bytes(&target.join("blob"), TWO_MIB);
    let ctx = fx.ctx();
    let finding = path_finding(&ctx, &[], item(&target)).unwrap().unwrap();
    assert_eq!(finding.location, "~/.npm/_cacache");
    assert!(finding.bytes >= TWO_MIB as u64);
    assert!(finding.fingerprint.is_some());
    assert!(finding.blocked.is_none());
    assert_eq!(finding.action, Action::Delete(target.clone()));
    assert_eq!(
        finding.id,
        FindingId::of(CategoryId::NpmCache, &finding.action)
    );
}

#[test]
fn path_finding_uses_trash_action_when_asked() {
    let fx = fixture();
    let target = fx.home.join("Downloads/grande.iso");
    write_bytes(&target, TWO_MIB);
    let ctx = fx.ctx();
    let mut trashed = item(&target);
    trashed.trash = true;
    let finding = path_finding(&ctx, &[], trashed).unwrap().unwrap();
    assert_eq!(finding.action, Action::Trash(target));
}

#[test]
fn path_finding_skips_missing_and_small() {
    let fx = fixture();
    let small = fx.home.join("pequeno");
    write_bytes(&small.join("blob"), 1024);
    let ctx = fx.ctx();
    let npm: &[&str] = &[];
    assert_eq!(path_finding(&ctx, npm, item(&small)), Ok(None));
    assert_eq!(
        path_finding(&ctx, npm, item(&fx.home.join("sumiu"))),
        Ok(None)
    );
}

#[test]
fn scan_all_collects_findings_and_reports_each_category() {
    let fx = fixture();
    write_bytes(&fx.home.join(".npm/_cacache/blob"), TWO_MIB);
    let ctx = fx.ctx();
    let done = std::cell::RefCell::new(Vec::new());
    let (ledger, scans) = scan_all(&ctx, None, &|scan| done.borrow_mut().push(scan.id));
    assert_eq!(done.borrow().len(), registry().len());
    assert_eq!(scans.len(), registry().len());
    assert_eq!(ledger.findings.len(), 1);
    assert_eq!(ledger.uid, fx.env.uid);
}

#[test]
fn scan_all_can_filter_one_category() {
    let fx = fixture();
    write_bytes(&fx.home.join(".npm/_cacache/blob"), TWO_MIB);
    let ctx = fx.ctx();
    let (ledger, scans) = scan_all(&ctx, Some(CategoryId::NpmCache), &|_| {});
    assert_eq!(scans.len(), 1);
    assert_eq!(ledger.findings.len(), 1);
}

#[test]
fn scan_problems_have_portuguese_messages() {
    assert_eq!(
        ScanProblem::ToolMissing("docker").to_string(),
        "a ferramenta docker não foi encontrada"
    );
    assert!(ScanProblem::NoProjects.to_string().contains("projetos"));
}

#[test]
fn path_finding_reports_unreadable_path() {
    let fx = fixture();
    let target = fx.home.join("fechado");
    write_bytes(&target.join("blob"), TWO_MIB);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o000)).unwrap();
    let ctx = fx.ctx();
    let result = path_finding(&ctx, &[], item(&target));
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        result,
        Err(ScanProblem::NoAccess(target.display().to_string()))
    );
}

fn item(path: &Path) -> PathItem<'_> {
    PathItem {
        category: CategoryId::NpmCache,
        path,
        label: "item".to_string(),
        risk: Risk::Safe,
        reversibility: Reversibility::Regenerable,
        note: None,
        trash: false,
    }
}
