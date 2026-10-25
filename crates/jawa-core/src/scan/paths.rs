use std::path::Path;

use super::categories::{Category, ScanOutput, ScanProblem};
use super::{MIN_ITEM_BYTES, ScanContext};
use crate::clean::chown_fix;
use crate::model::finding::{
    Action, BlockReason, CategoryId, Finding, FindingId, Reversibility, Risk, fingerprint_of,
};
use crate::system::measure::{Measurement, measure};
use crate::system::processes::{Holder, holders_of, running_named};

pub struct PathItem<'a> {
    pub category: CategoryId,
    pub path: &'a Path,
    pub label: String,
    pub risk: Risk,
    pub reversibility: Reversibility,
    pub note: Option<String>,
    pub trash: bool,
}

pub fn path_finding(
    ctx: &ScanContext,
    watched: &[&str],
    item: PathItem,
) -> Result<Option<Finding>, ScanProblem> {
    let measurement = match measure(item.path, ctx.env.uid) {
        Ok(measurement) => measurement,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            tracing::warn!(path = ?item.path, %error, "não foi possível medir o caminho");
            return Err(ScanProblem::NoAccess(item.path.display().to_string()));
        }
    };
    if measurement.allocated_bytes < MIN_ITEM_BYTES {
        return Ok(None);
    }
    let action = if item.trash {
        Action::Trash(item.path.to_path_buf())
    } else {
        Action::Delete(item.path.to_path_buf())
    };
    Ok(Some(Finding {
        id: FindingId::of(item.category, &action),
        category: item.category,
        label: item.label,
        location: display_location(item.path, &ctx.env.home),
        bytes: measurement.allocated_bytes,
        risk: item.risk,
        reversibility: item.reversibility,
        note: item.note,
        fingerprint: fingerprint_of(item.path).ok(),
        blocked: block_reason(ctx, item.path, watched, &measurement),
        action,
    }))
}

pub fn collect_findings<'a>(
    ctx: &ScanContext,
    category: &'a dyn Category,
    items: impl IntoIterator<Item = PathItem<'a>>,
) -> Result<ScanOutput, ScanProblem> {
    let watched = category.watched_processes();
    collect_findings_watched(ctx, items.into_iter().map(|item| (watched, item)))
}

pub(crate) fn collect_findings_watched<'a>(
    ctx: &ScanContext,
    items: impl IntoIterator<Item = (&'a [&'a str], PathItem<'a>)>,
) -> Result<ScanOutput, ScanProblem> {
    let mut findings = Vec::new();
    let mut first_failure: Option<ScanProblem> = None;
    let mut unreadable = 0usize;
    let mut readable = 0usize;
    for (watched, item) in items {
        match path_finding(ctx, watched, item) {
            Ok(found) => {
                readable += 1;
                findings.extend(found);
            }
            Err(problem) => {
                unreadable += 1;
                first_failure.get_or_insert(problem);
            }
        }
    }
    match first_failure {
        Some(problem) if readable == 0 => Err(problem),
        Some(_) => Ok(ScanOutput {
            findings,
            footnote: Some(format!("{unreadable} caminho(s) não puderam ser lidos.")),
        }),
        None => Ok(findings.into()),
    }
}

pub(crate) fn display_location(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => path.display().to_string(),
    }
}

fn block_reason(
    ctx: &ScanContext,
    path: &Path,
    watched: &[&str],
    measurement: &Measurement,
) -> Option<BlockReason> {
    let mut holders: Vec<Holder> = holders_of(path, &ctx.env.proc_root);
    holders.extend(running_named(watched, &ctx.env.proc_root));
    if !holders.is_empty() {
        return Some(BlockReason::InUse(holders));
    }
    if measurement.foreign_owned > 0 {
        return Some(BlockReason::ForeignOwned {
            count: measurement.foreign_owned,
            fix: chown_fix(path),
        });
    }
    measurement
        .crosses_mount
        .then_some(BlockReason::CrossesMount)
}
