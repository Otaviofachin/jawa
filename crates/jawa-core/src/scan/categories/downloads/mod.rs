mod texts;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::paths::{PathItem, collect_findings};
use crate::scan::{ScanContext, join_footnotes};
use crate::system::measure::measure;

pub const PACKAGE_SUFFIXES: [&str; 9] = [
    ".deb",
    ".rpm",
    ".appimage",
    ".iso",
    ".img",
    ".tar.gz",
    ".tgz",
    ".tar.xz",
    ".zip",
];
pub const PARTIAL_SUFFIXES: [&str; 2] = [".part", ".crdownload"];

const PACKAGE_MIN_BYTES: u64 = 10_000_000;
const LARGE_MIN_BYTES: u64 = 100_000_000;
const IDLE_MIN_BYTES: u64 = 10_000_000;
const IDLE_DAYS: u64 = 90;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const NO_DOWNLOADS_NOTE: &str = "Não há pasta de downloads.";
const HOME_AS_DOWNLOADS_NOTE: &str =
    "A pasta de downloads é a própria pasta pessoal; nada foi oferecido.";

pub fn classify(size: u64, age: Option<Duration>, name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let is_partial = PARTIAL_SUFFIXES
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    if name.starts_with('.') || is_partial {
        return None;
    }
    let package = PACKAGE_SUFFIXES
        .iter()
        .find(|suffix| lower.ends_with(**suffix));
    if let Some(suffix) = package.filter(|_| size >= PACKAGE_MIN_BYTES) {
        return Some(format!(
            "Pacote de instalação ({suffix}): se já instalou ou usou, não precisa mais dele."
        ));
    }
    if size >= LARGE_MIN_BYTES {
        return Some("Item grande (100 MB ou mais).".to_string());
    }
    let idle_days = age.map(|age| age.as_secs() / SECONDS_PER_DAY)?;
    (size >= IDLE_MIN_BYTES && idle_days >= IDLE_DAYS)
        .then(|| format!("Parado há {idle_days} dias."))
}

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::Downloads,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct Downloads;

impl Category for Downloads {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        ctx.env.downloads_dir.iter().cloned().collect()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let Some(dir) = ctx.env.downloads_dir.as_deref() else {
            return Ok(note_only(NO_DOWNLOADS_NOTE));
        };
        if is_home(dir, &ctx.env.home) {
            return Ok(note_only(HOME_AS_DOWNLOADS_NOTE));
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return Ok(note_only(NO_DOWNLOADS_NOTE));
        };
        let mut offers: Vec<Offer> = Vec::new();
        let mut unreadable = 0usize;
        for entry in entries.filter_map(Result::ok) {
            match candidate(ctx, &entry.path()) {
                Candidate::Offer(offer) => offers.push(offer),
                Candidate::Skip => {}
                Candidate::Unreadable => unreadable += 1,
            }
        }
        let items = offers.iter().map(Offer::item);
        let mut output = collect_findings(ctx, self, items)?;
        output.footnote = join_footnotes([
            output.footnote.take(),
            (unreadable > 0).then(|| format!("{unreadable} item(ns) não puderam ser lidos.")),
        ]);
        Ok(output)
    }
}

struct Offer {
    path: PathBuf,
    name: String,
    note: String,
}

impl Offer {
    fn item(&self) -> PathItem<'_> {
        PathItem {
            category: CategoryId::Downloads,
            path: &self.path,
            label: self.name.clone(),
            risk: Risk::Review,
            reversibility: Reversibility::Trash,
            note: Some(self.note.clone()),
            trash: true,
        }
    }
}

enum Candidate {
    Offer(Offer),
    Skip,
    Unreadable,
}

fn note_only(note: &str) -> ScanOutput {
    ScanOutput {
        findings: Vec::new(),
        footnote: Some(note.to_string()),
    }
}

fn is_home(dir: &Path, home: &Path) -> bool {
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical(dir) == canonical(home)
}

fn candidate(ctx: &ScanContext, path: &Path) -> Candidate {
    let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return Candidate::Skip;
    };
    let size = match measure(path, ctx.env.uid) {
        Ok(measurement) => measurement.allocated_bytes,
        Err(_) if classify(u64::MAX, None, &name).is_none() => return Candidate::Skip,
        Err(error) => {
            tracing::warn!(?path, %error, "não foi possível medir o item de downloads");
            return Candidate::Unreadable;
        }
    };
    let age = fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| ctx.env.now.duration_since(modified).ok());
    match classify(size, age, &name) {
        Some(note) => Candidate::Offer(Offer {
            path: path.to_path_buf(),
            name,
            note,
        }),
        None => Candidate::Skip,
    }
}

#[cfg(test)]
mod tests;
