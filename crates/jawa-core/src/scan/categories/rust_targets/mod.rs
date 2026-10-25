mod texts;

use std::path::{Path, PathBuf};

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::paths::{PathItem, collect_findings};
use crate::scan::projects::{all_repositories, git_for, repo_label};
use crate::scan::projects::{home_folders, is_repository};
use crate::scan::{ScanContext, join_footnotes};
use crate::system::git::Git;
use crate::system::runner::RunError;

const TARGET_LOCATIONS: [&str; 2] = ["target", "src-tauri/target"];

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::RustTargets,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct RustTargets;

impl Category for RustTargets {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        let mut roots = ctx.project_roots.clone();
        roots.push(ctx.env.home.clone());
        roots
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let git = git_for(ctx)?;
        let mut targets = Vec::new();
        let mut failed_projects = 0usize;
        for project in project_dirs(ctx) {
            match targets_of(&git, ctx, &project) {
                Ok(found) => targets.extend(found.into_iter().map(|path| (project.clone(), path))),
                Err(error) => {
                    tracing::warn!(?project, %error, "o git não respondeu para o projeto");
                    failed_projects += 1;
                }
            }
        }
        let items = targets
            .iter()
            .map(|(project, path)| target_item(project, path));
        let mut output = collect_findings(ctx, self, items)?;
        let git_note = (failed_projects > 0)
            .then(|| format!("{failed_projects} projeto(s) não puderam ser consultados pelo git."));
        output.footnote = join_footnotes([output.footnote.take(), git_note]);
        Ok(output)
    }
}

fn project_dirs(ctx: &ScanContext) -> Vec<PathBuf> {
    let mut projects = all_repositories(ctx);
    for folder in home_folders(ctx.env) {
        if !projects.contains(&folder) {
            projects.push(folder);
        }
    }
    projects
}

fn targets_of(git: &Git, ctx: &ScanContext, project: &Path) -> Result<Vec<PathBuf>, RunError> {
    let existing: Vec<PathBuf> = TARGET_LOCATIONS
        .iter()
        .map(|location| project.join(location))
        .filter(|target| is_real_dir(target) && has_manifest_beside(target))
        .collect();
    let Some(repo) = enclosing_repository(project, &ctx.env.home) else {
        return Ok(existing);
    };
    let ignored = git.ignored_among(&repo, &existing)?;
    Ok(existing
        .into_iter()
        .filter(|target| ignored.contains(target))
        .collect())
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn has_manifest_beside(target: &Path) -> bool {
    target
        .parent()
        .is_some_and(|parent| parent.join("Cargo.toml").is_file())
}

fn enclosing_repository(project: &Path, home: &Path) -> Option<PathBuf> {
    project
        .ancestors()
        .take_while(|ancestor| *ancestor != home)
        .find(|ancestor| is_repository(ancestor))
        .map(Path::to_path_buf)
}

fn target_item<'a>(project: &Path, path: &'a Path) -> PathItem<'a> {
    PathItem {
        category: CategoryId::RustTargets,
        path,
        label: repo_label(project, path.parent().unwrap_or(path)),
        risk: Risk::Safe,
        reversibility: Reversibility::Regenerable,
        note: Some("target: objetos compilados pelo cargo; o próximo build recompila.".to_string()),
        trash: false,
    }
}

#[cfg(test)]
mod tests;
