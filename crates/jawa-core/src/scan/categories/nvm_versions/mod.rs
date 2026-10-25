mod texts;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::paths::{PathItem, collect_findings};
use crate::scan::projects::all_repositories;
use crate::scan::{ScanContext, join_footnotes};

const MAX_ALIAS_HOPS: usize = 5;
const NVMRC_MAX_DEPTH: usize = 3;
const NO_ACTIVE_NODE_NOTE: &str =
    "Não achei o node ativo no PATH; só o alias default e os .nvmrc protegem versões.";
const DEFAULT_ALIAS: &str = "default";
const NVMRC_NAME: &str = ".nvmrc";
const IGNORED_GLOBALS: [&str; 2] = ["npm", "corepack"];
const LTS_CODENAMES: [(&str, u32); 11] = [
    ("argon", 4),
    ("boron", 6),
    ("carbon", 8),
    ("dubnium", 10),
    ("erbium", 12),
    ("fermium", 14),
    ("gallium", 16),
    ("hydrogen", 18),
    ("iron", 20),
    ("jod", 22),
    ("krypton", 24),
];

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::NvmVersions,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct NvmVersions;

impl Category for NvmVersions {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        vec![versions_dir(ctx)]
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        scan_with(ctx, std::env::var_os("PATH"))
    }
}

fn versions_dir(ctx: &ScanContext) -> PathBuf {
    ctx.env.nvm_dir.join("versions/node")
}

fn aliases_dir(ctx: &ScanContext) -> PathBuf {
    ctx.env.nvm_dir.join("alias")
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NodeVersion {
    numbers: [u32; 3],
    dir: PathBuf,
}

fn scan_with(ctx: &ScanContext, path_var: Option<OsString>) -> Result<ScanOutput, ScanProblem> {
    let installed = installed_versions(&versions_dir(ctx))?;
    let protection = protected(ctx, &installed, path_var);
    if !protection.unknown.is_empty() {
        return Ok(ScanOutput {
            findings: Vec::new(),
            footnote: Some(unknown_requests_note(&protection.unknown)),
        });
    }
    let unprotected: Vec<&NodeVersion> = installed
        .iter()
        .filter(|version| !protection.versions.contains(&version.numbers))
        .collect();
    let items = unprotected.iter().map(|version| version_item(version));
    let mut output = collect_findings(ctx, &NvmVersions, items)?;
    let protected_count = installed.len() - unprotected.len();
    output.footnote = join_footnotes([
        output.footnote.take(),
        projects_warning(ctx),
        protected_note(protected_count),
        (!protection.active_found).then(|| NO_ACTIVE_NODE_NOTE.to_string()),
    ]);
    Ok(output)
}

fn unknown_requests_note(unknown: &[UnknownRequest]) -> String {
    let mut parts: Vec<String> = unknown
        .iter()
        .map(|request| {
            format!(
                "Não entendi a versão pedida em {}: '{}'.",
                request.source.display(),
                request.text
            )
        })
        .collect();
    parts.push("Nenhuma versão foi oferecida.".to_string());
    parts.join(" ")
}

fn projects_warning(ctx: &ScanContext) -> Option<String> {
    ctx.project_roots.is_empty().then(|| {
        "Sem pastas de projetos, o Jawa não sabe quais versões eles pedem: confira os .nvmrc \
         antes de limpar."
            .to_string()
    })
}

fn protected_note(count: usize) -> Option<String> {
    const WHY: &str = "(padrão, .nvmrc de projeto ou ativa no PATH)";
    match count {
        0 => None,
        1 => Some(format!("1 versão ficou protegida {WHY}.")),
        _ => Some(format!("{count} versões ficaram protegidas {WHY}.")),
    }
}

fn version_item(version: &NodeVersion) -> PathItem<'_> {
    let [major, minor, patch] = version.numbers;
    let mut note = "Nenhum projeto pede esta versão.".to_string();
    let globals = global_packages(&version.dir);
    if !globals.is_empty() {
        note.push_str(&format!(" Globais que vão junto: {}.", globals.join(", ")));
    }
    note.push_str(&format!(
        " Para voltar: nvm install {major}.{minor}.{patch}."
    ));
    PathItem {
        category: CategoryId::NvmVersions,
        path: &version.dir,
        label: format!("Node v{major}.{minor}.{patch}"),
        risk: Risk::Review,
        reversibility: Reversibility::Regenerable,
        note: Some(note),
        trash: false,
    }
}

fn installed_versions(base: &Path) -> Result<Vec<NodeVersion>, ScanProblem> {
    let entries = fs::read_dir(base).map_err(|_| ScanProblem::ToolMissing("nvm"))?;
    let mut installed: Vec<NodeVersion> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let numbers = parse_version_name(entry.file_name().to_str()?)?;
            Some(NodeVersion {
                numbers,
                dir: entry.path(),
            })
        })
        .collect();
    installed.sort();
    Ok(installed)
}

fn parse_version_name(name: &str) -> Option<[u32; 3]> {
    let numbers = parse_numbers(name)?;
    <[u32; 3]>::try_from(numbers).ok()
}

fn parse_numbers(text: &str) -> Option<Vec<u32>> {
    let parts: Vec<&str> = text.strip_prefix('v').unwrap_or(text).split('.').collect();
    if !(1..=3).contains(&parts.len()) {
        return None;
    }
    let is_wildcard = |part: &&str| matches!(*part, "x" | "X" | "*");
    let numeric = parts.iter().take_while(|part| !is_wildcard(part)).count();
    if !parts[numeric..].iter().all(is_wildcard) {
        return None;
    }
    parts[..numeric]
        .iter()
        .map(|part| part.parse().ok())
        .collect()
}

enum Request {
    Newest,
    LatestLts,
    Prefix(Vec<u32>),
    Alias(String),
}

fn classify(request: &str) -> Request {
    if matches!(request, "node" | "stable") {
        return Request::Newest;
    }
    if request == "lts/*" {
        return Request::LatestLts;
    }
    let codename_major = request
        .strip_prefix("lts/")
        .and_then(|name| LTS_CODENAMES.iter().find(|(codename, _)| *codename == name))
        .map(|(_, major)| *major);
    if let Some(major) = codename_major {
        return Request::Prefix(vec![major]);
    }
    match parse_numbers(request) {
        Some(prefix) => Request::Prefix(prefix),
        None => Request::Alias(request.to_string()),
    }
}

#[cfg(test)]
fn resolve<'a>(
    request: &str,
    installed: &'a [NodeVersion],
    aliases: &Path,
) -> Option<&'a NodeVersion> {
    match resolve_detailed(request.trim(), installed, aliases, MAX_ALIAS_HOPS) {
        Resolution::Found(version) => Some(version),
        Resolution::NotInstalled | Resolution::Unknown => None,
    }
}

enum Resolution<'a> {
    Found(&'a NodeVersion),
    NotInstalled,
    Unknown,
}

fn resolve_detailed<'a>(
    request: &str,
    installed: &'a [NodeVersion],
    aliases: &Path,
    hops_left: usize,
) -> Resolution<'a> {
    if let Some(target) = read_lts_alias(aliases, request) {
        return match hops_left {
            0 => Resolution::Unknown,
            _ => resolve_detailed(&target, installed, aliases, hops_left - 1),
        };
    }
    let found = match classify(request) {
        Request::Newest => installed.iter().max(),
        Request::LatestLts => installed
            .iter()
            .filter(|version| version.numbers[0] % 2 == 0)
            .max(),
        Request::Prefix(prefix) => installed
            .iter()
            .filter(|version| version.numbers.starts_with(&prefix))
            .max(),
        Request::Alias(name) => {
            return resolve_alias(&name, installed, aliases, hops_left);
        }
    };
    found.map_or(Resolution::NotInstalled, Resolution::Found)
}

fn resolve_alias<'a>(
    name: &str,
    installed: &'a [NodeVersion],
    aliases: &Path,
    hops_left: usize,
) -> Resolution<'a> {
    match read_alias(aliases, name) {
        _ if hops_left == 0 => Resolution::Unknown,
        Some(target) => resolve_detailed(&target, installed, aliases, hops_left - 1),
        None if name == DEFAULT_ALIAS && hops_left == MAX_ALIAS_HOPS => Resolution::NotInstalled,
        None => Resolution::Unknown,
    }
}

fn read_lts_alias(aliases: &Path, request: &str) -> Option<String> {
    let name = request.strip_prefix("lts/")?;
    let is_plain = name == "*"
        || (!name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-'));
    if !is_plain {
        return None;
    }
    let text = fs::read_to_string(aliases.join("lts").join(name)).ok()?;
    Some(text.trim().to_string()).filter(|target| !target.is_empty())
}

fn read_alias(aliases: &Path, name: &str) -> Option<String> {
    let is_plain = !name.is_empty()
        && !name.starts_with('/')
        && !name.split('/').any(|part| part == ".." || part == ".")
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || "-_./".contains(character));
    if !is_plain {
        return None;
    }
    let text = fs::read_to_string(aliases.join(name)).ok()?;
    Some(text.trim().to_string())
}

struct UnknownRequest {
    source: PathBuf,
    text: String,
}

struct Protection {
    versions: BTreeSet<[u32; 3]>,
    unknown: Vec<UnknownRequest>,
    active_found: bool,
}

fn protected(
    ctx: &ScanContext,
    installed: &[NodeVersion],
    path_var: Option<OsString>,
) -> Protection {
    let aliases = aliases_dir(ctx);
    let mut requests = nvmrc_requests(ctx);
    requests.push((aliases.join(DEFAULT_ALIAS), DEFAULT_ALIAS.to_string()));
    let mut protection = Protection {
        versions: BTreeSet::new(),
        unknown: Vec::new(),
        active_found: false,
    };
    for (source, text) in requests {
        match resolve_detailed(text.trim(), installed, &aliases, MAX_ALIAS_HOPS) {
            Resolution::Found(version) => {
                protection.versions.insert(version.numbers);
            }
            Resolution::NotInstalled => {}
            Resolution::Unknown => {
                let shown = read_alias(&aliases, &text).unwrap_or(text);
                protection.unknown.push(UnknownRequest {
                    source,
                    text: shown,
                });
            }
        }
    }
    let active = active_version(&ctx.env.nvm_dir, path_var, &ctx.env.home);
    protection.active_found = active.is_some();
    protection.versions.extend(active);
    protection
}

fn active_version(nvm_dir: &Path, path_var: Option<OsString>, cwd: &Path) -> Option<[u32; 3]> {
    let node = which::which_in("node", path_var, cwd).ok()?;
    let target = node.canonicalize().ok()?;
    let base = nvm_dir.join("versions/node").canonicalize().ok()?;
    let first = target.strip_prefix(&base).ok()?.components().next()?;
    parse_version_name(first.as_os_str().to_str()?)
}

fn nvmrc_requests(ctx: &ScanContext) -> Vec<(PathBuf, String)> {
    all_repositories(ctx)
        .iter()
        .flat_map(|repo| {
            WalkDir::new(repo)
                .max_depth(NVMRC_MAX_DEPTH)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| !is_walled(entry))
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name() == NVMRC_NAME && entry.file_type().is_file())
                .filter_map(|entry| {
                    let line = first_useful_line(entry.path())?;
                    Some((entry.path().to_path_buf(), line))
                })
        })
        .collect()
}

fn is_walled(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    entry.depth() > 0
        && entry.file_type().is_dir()
        && (name.starts_with('.') || name == "node_modules")
}

fn first_useful_line(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

fn global_packages(version_dir: &Path) -> Vec<String> {
    let modules = version_dir.join("lib/node_modules");
    let mut names: Vec<String> = subdirs(&modules)
        .into_iter()
        .flat_map(|name| {
            if name.starts_with('@') {
                let scoped = subdirs(&modules.join(&name));
                scoped
                    .into_iter()
                    .map(|inner| format!("{name}/{inner}"))
                    .collect()
            } else {
                vec![name]
            }
        })
        .filter(|name| !IGNORED_GLOBALS.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect()
}

#[cfg(test)]
mod tests;
