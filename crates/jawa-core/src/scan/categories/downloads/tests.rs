use std::fs::{self, File, FileTimes};
use std::path::Path;
use std::time::SystemTime;

use super::*;
use crate::model::finding::{Action, Reversibility, Risk};
use crate::safety::boundary::check_target;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};

const MB: u64 = 1_000_000;
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn days(count: u64) -> Option<Duration> {
    Some(DAY * count as u32)
}

fn with_downloads() -> (Fixture, PathBuf) {
    let mut fx = fixture();
    let downloads = fx.home.join("Downloads");
    fs::create_dir_all(&downloads).unwrap();
    fx.env.downloads_dir = Some(downloads.clone());
    (fx, downloads)
}

fn age_file(path: &Path, age: Duration) {
    let modified = SystemTime::now() - age;
    let file = File::options().write(true).open(path).unwrap();
    file.set_times(FileTimes::new().set_modified(modified))
        .unwrap();
}

#[test]
fn picks_packages_large_and_old() {
    assert!(classify(12 * MB, days(1), "app.AppImage").is_some());
    assert!(classify(101 * MB, days(1), "dataset.csv").is_some());
    assert!(classify(11 * MB, days(120), "video.mp4").is_some());
    assert_eq!(classify(11 * MB, days(10), "recente.mp4"), None);
    assert_eq!(classify(MB, days(400), "nota.txt"), None);
    assert_eq!(classify(9 * MB, days(1), "pequeno.deb"), None);
    assert!(classify(10 * MB, None, "ubuntu.ISO").is_some());
    assert!(classify(10 * MB, None, "fonte.tar.xz").is_some());
    assert_eq!(classify(11 * MB, None, "sem-idade.mp4"), None);
    assert!(
        classify(12 * MB, days(1), "app.AppImage")
            .unwrap()
            .contains("instalação")
    );
    assert!(
        classify(11 * MB, days(120), "video.mp4")
            .unwrap()
            .contains("120 dias")
    );
}

#[test]
fn skips_partial_and_hidden() {
    assert_eq!(classify(500 * MB, days(200), "filme.mkv.part"), None);
    assert_eq!(classify(500 * MB, days(200), "imagem.iso.crdownload"), None);
    assert_eq!(classify(500 * MB, days(200), "imagem.iso.CRDOWNLOAD"), None);
    assert_eq!(classify(500 * MB, days(200), ".oculto.iso"), None);
    let (fx, downloads) = with_downloads();
    write_bytes(&downloads.join(".escondido.zip"), 12 * MB as usize);
    write_bytes(&downloads.join("baixando.deb.part"), 12 * MB as usize);
    let output = Downloads.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
}

#[test]
fn actions_go_to_trash() {
    let (fx, downloads) = with_downloads();
    let package = downloads.join("app.AppImage");
    write_bytes(&package, 12 * MB as usize);
    let old_video = downloads.join("video.mp4");
    write_bytes(&old_video, 11 * MB as usize);
    age_file(&old_video, DAY * 120);
    let recent = downloads.join("recente.mp4");
    write_bytes(&recent, 11 * MB as usize);
    write_bytes(&downloads.join("nota.txt"), MB as usize);
    let ctx = fx.ctx();
    let output = Downloads.scan(&ctx).unwrap();
    let mut labels: Vec<&str> = output.findings.iter().map(|f| f.label.as_str()).collect();
    labels.sort_unstable();
    assert_eq!(labels, ["app.AppImage", "video.mp4"]);
    let roots = Downloads.roots(&ctx);
    for finding in &output.findings {
        assert!(matches!(finding.action, Action::Trash(_)));
        assert_eq!(finding.risk, Risk::Review);
        assert_eq!(finding.reversibility, Reversibility::Trash);
        let path = finding.action.path().unwrap();
        assert!(check_target(path, &roots, &fx.env).is_ok());
    }
}

#[test]
fn no_downloads_dir() {
    let fx = fixture();
    let output = Downloads.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
    let mut missing = fixture();
    missing.env.downloads_dir = Some(missing.home.join("Downloads"));
    assert!(Downloads.scan(&missing.ctx()).unwrap().findings.is_empty());
}

#[test]
fn unreadable_item_is_counted_not_offered() {
    use std::os::unix::fs::PermissionsExt;
    let (fx, downloads) = with_downloads();
    let closed = downloads.join("fechada");
    write_bytes(&closed.join("grande.bin"), 12 * MB as usize);
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o000)).unwrap();
    let output = Downloads.scan(&fx.ctx());
    fs::set_permissions(&closed, fs::Permissions::from_mode(0o755)).unwrap();
    let output = output.unwrap();
    assert!(output.findings.is_empty());
    assert_eq!(
        output.footnote.as_deref(),
        Some("1 item(ns) não puderam ser lidos.")
    );
}

#[test]
fn downloads_equal_to_home_offers_nothing() {
    let mut fx = fixture();
    fx.env.downloads_dir = Some(fx.home.clone());
    write_bytes(&fx.home.join("projeto.zip"), 12 * MB as usize);
    let output = Downloads.scan(&fx.ctx()).unwrap();
    assert!(output.findings.is_empty());
    assert!(output.footnote.is_some());
}
