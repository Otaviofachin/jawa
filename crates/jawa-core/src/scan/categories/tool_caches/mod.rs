mod texts;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::paths::{PathItem, collect_findings};
use crate::scan::{ScanContext, join_footnotes};
use crate::system::env::Environment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Cache,
    Home,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownCache {
    pub base: Base,
    pub relative: &'static str,
    pub label: &'static str,
    pub note: &'static str,
}

impl KnownCache {
    pub fn path(&self, env: &Environment) -> PathBuf {
        let base = match self.base {
            Base::Cache => &env.cache_dir,
            Base::Home => &env.home,
            Base::Config => &env.config_dir,
        };
        base.join(self.relative)
    }
}

const fn tool(
    base: Base,
    relative: &'static str,
    label: &'static str,
    note: &'static str,
) -> KnownCache {
    KnownCache {
        base,
        relative,
        label,
        note,
    }
}

pub const TOOL_CACHES: [KnownCache; 13] = [
    tool(
        Base::Cache,
        "ms-playwright",
        "Navegadores do Playwright",
        "Voltam com `npx playwright install`.",
    ),
    tool(
        Base::Cache,
        "Cypress",
        "Binário do Cypress",
        "Volta no próximo `npm install` ou com `npx cypress install`.",
    ),
    tool(
        Base::Cache,
        "puppeteer",
        "Navegadores do Puppeteer",
        "Voltam na próxima instalação do puppeteer.",
    ),
    tool(
        Base::Cache,
        "yarn",
        "Cache do Yarn",
        "O próximo `yarn install` baixa de novo.",
    ),
    tool(
        Base::Cache,
        "pnpm",
        "Cache do pnpm",
        "O próximo `pnpm install` baixa de novo.",
    ),
    tool(
        Base::Cache,
        "node-gyp",
        "Cabeçalhos do Node (node-gyp)",
        "Voltam ao compilar um módulo nativo.",
    ),
    tool(
        Base::Cache,
        "typescript",
        "Tipos baixados pelo editor (TypeScript)",
        "O editor baixa de novo os tipos que usar.",
    ),
    tool(
        Base::Cache,
        "pip",
        "Cache do pip",
        "O próximo `pip install` baixa de novo.",
    ),
    tool(
        Base::Cache,
        "prisma",
        "Motores do Prisma",
        "Voltam no próximo `prisma generate`.",
    ),
    tool(
        Base::Cache,
        "go-build",
        "Cache de build do Go",
        "O próximo `go build` recompila.",
    ),
    tool(
        Base::Cache,
        "thumbnails",
        "Miniaturas de imagens",
        "O gerenciador de arquivos recria ao abrir as pastas.",
    ),
    tool(
        Base::Home,
        ".gradle/caches",
        "Cache do Gradle",
        "O próximo build do Gradle baixa as dependências de novo.",
    ),
    tool(
        Base::Home,
        ".cargo/registry",
        "Registry do Cargo",
        "O próximo `cargo build` baixa os crates de novo.",
    ),
];

const CLAUDE_VERSIONS: &str = "claude/versions";
const CLAUDE_LAUNCHER: &str = ".local/bin/claude";
const UNKNOWN_LAUNCHER_NOTE: &str = "Não consegui saber a versão em uso pelo link \
                                     ~/.local/bin/claude; nenhuma versão do Claude Code foi \
                                     oferecida.";

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::ToolCaches,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct ToolCaches;

impl Category for ToolCaches {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        let paths: Vec<PathBuf> = TOOL_CACHES
            .iter()
            .map(|cache| cache.path(ctx.env))
            .collect();
        let mut roots = parent_roots(paths.iter().map(PathBuf::as_path));
        roots.push(claude_versions_dir(ctx.env));
        roots
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let mut entries: Vec<CacheEntry> = TOOL_CACHES
            .iter()
            .map(|cache| CacheEntry::known(cache, ctx.env))
            .collect();
        let (versions, footnote) = old_claude_versions(ctx.env);
        entries.extend(versions);
        let items = entries
            .iter()
            .map(|entry| entry.item(CategoryId::ToolCaches));
        let mut output = collect_findings(ctx, self, items)?;
        output.footnote = join_footnotes([output.footnote.take(), footnote]);
        Ok(output)
    }
}

fn claude_versions_dir(env: &Environment) -> PathBuf {
    env.data_dir.join(CLAUDE_VERSIONS)
}

fn old_claude_versions(env: &Environment) -> (Vec<CacheEntry>, Option<String>) {
    let Ok(entries) = fs::read_dir(claude_versions_dir(env)) else {
        return (Vec::new(), None);
    };
    let names: Vec<OsString> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    if names.is_empty() {
        return (Vec::new(), None);
    }
    let Some(in_use) = claude_in_use(env) else {
        return (Vec::new(), Some(UNKNOWN_LAUNCHER_NOTE.to_string()));
    };
    let in_use_text = in_use.to_string_lossy().into_owned();
    let old = names
        .into_iter()
        .filter(|name| *name != in_use)
        .map(|name| claude_version_entry(env, &name, &in_use_text))
        .collect();
    (old, None)
}

fn claude_in_use(env: &Environment) -> Option<OsString> {
    let launcher = env.home.join(CLAUDE_LAUNCHER);
    fs::read_link(&launcher).ok()?;
    let target = fs::canonicalize(&launcher).ok()?;
    let versions = fs::canonicalize(claude_versions_dir(env)).ok()?;
    let inside = target.strip_prefix(&versions).ok()?;
    inside
        .components()
        .next()
        .map(|component| component.as_os_str().to_os_string())
}

fn claude_version_entry(env: &Environment, name: &OsString, in_use: &str) -> CacheEntry {
    let version = name.to_string_lossy();
    CacheEntry {
        path: claude_versions_dir(env).join(name),
        label: format!("Claude Code {version}"),
        note: format!("Versão antiga do Claude Code; a em uso ({in_use}) fica."),
    }
}

pub(super) struct CacheEntry {
    pub path: PathBuf,
    pub label: String,
    pub note: String,
}

impl CacheEntry {
    pub(super) fn known(cache: &KnownCache, env: &Environment) -> CacheEntry {
        CacheEntry {
            path: cache.path(env),
            label: cache.label.to_string(),
            note: cache.note.to_string(),
        }
    }

    pub(super) fn item(&self, category: CategoryId) -> PathItem<'_> {
        PathItem {
            category,
            path: &self.path,
            label: self.label.clone(),
            risk: Risk::Safe,
            reversibility: Reversibility::Regenerable,
            note: Some(self.note.clone()),
            trash: false,
        }
    }
}

pub(super) fn parent_roots<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for parent in paths.into_iter().filter_map(Path::parent) {
        if !roots.iter().any(|root| root == parent) {
            roots.push(parent.to_path_buf());
        }
    }
    roots
}

#[cfg(test)]
mod tests;
