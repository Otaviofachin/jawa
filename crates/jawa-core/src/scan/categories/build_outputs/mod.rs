mod texts;

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::paths::{PathItem, collect_findings};
use crate::scan::projects::find_named_dirs;
use crate::scan::projects::{all_repositories, git_for, repo_label, require_roots};
use crate::scan::{ScanContext, join_footnotes};
use crate::system::git::Git;
use crate::system::runner::RunError;

pub const BUILD_NAMES: [&str; 6] = [
    "dist",
    "build",
    "www",
    "coverage",
    ".angular/cache",
    ".nx/cache",
];
pub const SIGNED_EXTENSIONS: [&str; 6] = ["apk", "aab", "ipa", "keystore", "jks", "p12"];
const MAX_DEPTH: usize = 3;

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::BuildOutputs,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct BuildOutputs;

impl Category for BuildOutputs {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        ctx.project_roots.clone()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let git = git_for(ctx)?;
        require_roots(ctx)?;
        let mut candidates = Vec::new();
        let mut failed_repos = 0usize;
        for repo in all_repositories(ctx) {
            match ignored_builds(&git, &repo) {
                Ok(found) => candidates.extend(found.into_iter().map(|path| (repo.clone(), path))),
                Err(error) => {
                    tracing::warn!(?repo, %error, "o git não respondeu para o repositório");
                    failed_repos += 1;
                }
            }
        }
        let items = candidates.iter().map(|(repo, path)| build_item(repo, path));
        let mut output = collect_findings(ctx, self, items)?;
        output.footnote = join_footnotes([output.footnote.take(), git_failure_note(failed_repos)]);
        Ok(output)
    }
}

fn git_failure_note(failed_repos: usize) -> Option<String> {
    (failed_repos > 0)
        .then(|| format!("{failed_repos} repositório(s) não puderam ser consultados pelo git."))
}

fn ignored_builds(git: &Git, repo: &Path) -> Result<Vec<PathBuf>, RunError> {
    let found = find_named_dirs(repo, &BUILD_NAMES, MAX_DEPTH);
    let ignored = git.ignored_among(repo, &found)?;
    Ok(found
        .into_iter()
        .filter(|path| ignored.contains(path))
        .collect())
}

fn build_item<'a>(repo: &Path, path: &'a Path) -> PathItem<'a> {
    let name = BUILD_NAMES
        .iter()
        .find(|name| path.ends_with(name))
        .copied()
        .unwrap_or("build");
    let mut note = format!("{name}: {}.", describe_build_name(name));
    let signed = first_signed_artifact(path);
    if let Some(file) = &signed {
        note.push_str(&format!(
            " Contém {file}, um artefato assinado: confira se há outra cópia."
        ));
    }
    PathItem {
        category: CategoryId::BuildOutputs,
        path,
        label: repo_label(repo, container_of(path, name)),
        risk: if signed.is_some() {
            Risk::Review
        } else {
            Risk::Safe
        },
        reversibility: Reversibility::Regenerable,
        note: Some(note),
        trash: false,
    }
}

fn container_of<'a>(path: &'a Path, name: &str) -> &'a Path {
    path.ancestors()
        .nth(name.split('/').count())
        .unwrap_or(path)
}

fn describe_build_name(name: &str) -> &'static str {
    match name {
        "dist" => "saída do build empacotada",
        "build" => "saída do build",
        "www" => "build web de app híbrido (Ionic/Cordova)",
        "coverage" => "relatórios de cobertura de testes",
        ".angular/cache" => "cache de build do Angular",
        ".nx/cache" => "cache de build do Nx",
        _ => "saída de build",
    }
}

fn first_signed_artifact(dir: &Path) -> Option<String> {
    WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| is_signed_artifact(name))
}

fn is_signed_artifact(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SIGNED_EXTENSIONS
                .iter()
                .any(|signed| signed.eq_ignore_ascii_case(extension))
        })
}

#[cfg(test)]
mod tests;
