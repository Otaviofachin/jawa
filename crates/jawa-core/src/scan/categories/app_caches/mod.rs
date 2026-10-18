//! `caches-apps`: caches de navegadores e apps, bloqueados com o app aberto (S6)
//! e, em `~/.config`, so as subpastas da lista (S19).

mod texts;

use std::path::{Path, PathBuf};

use super::tool_caches::{Base, CacheEntry, KnownCache, parent_roots};
use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Finding};
use crate::scan::ScanContext;
use crate::scan::paths::collect_findings_watched;

/// Um cache e os processos do app que o usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppCache {
    pub cache: KnownCache,
    pub watched: &'static [&'static str],
}

const CHROME: &[&str] = &["chrome"];
/// `chromium-browser` aparece cortado em 15 caracteres no `comm`.
const CHROMIUM: &[&str] = &["chromium", "chromium-browse"];
const FIREFOX: &[&str] = &["firefox", "firefox-esr"];
const SPOTIFY: &[&str] = &["spotify"];
const VSCODE: &[&str] = &["code"];
const SLACK: &[&str] = &["slack"];
const DISCORD: &[&str] = &["Discord"];

const BROWSER_NOTE: &str = "O navegador recria ao abrir; logins e favoritos ficam.";
const ELECTRON_NOTE: &str = "O app recria ao abrir; configurações e sessões ficam.";

const fn app(
    base: Base,
    relative: &'static str,
    label: &'static str,
    note: &'static str,
    watched: &'static [&'static str],
) -> AppCache {
    AppCache {
        cache: KnownCache {
            base,
            relative,
            label,
            note,
        },
        watched,
    }
}

pub const APP_CACHES: [AppCache; 17] = [
    app(
        Base::Cache,
        "google-chrome",
        "Cache do Google Chrome",
        BROWSER_NOTE,
        CHROME,
    ),
    app(
        Base::Cache,
        "chromium",
        "Cache do Chromium",
        BROWSER_NOTE,
        CHROMIUM,
    ),
    app(
        Base::Cache,
        "mozilla",
        "Cache do Firefox",
        BROWSER_NOTE,
        FIREFOX,
    ),
    app(
        Base::Cache,
        "spotify",
        "Cache do Spotify",
        "Músicas baixadas para ouvir offline somem.",
        SPOTIFY,
    ),
    app(
        Base::Config,
        "Code/Cache",
        "VS Code: Cache",
        ELECTRON_NOTE,
        VSCODE,
    ),
    app(
        Base::Config,
        "Code/CachedData",
        "VS Code: CachedData",
        ELECTRON_NOTE,
        VSCODE,
    ),
    app(
        Base::Config,
        "Code/CachedExtensionVSIXs",
        "VS Code: pacotes de extensões",
        "Instaladores de extensões já instaladas.",
        VSCODE,
    ),
    app(
        Base::Config,
        "Code/Code Cache",
        "VS Code: Code Cache",
        ELECTRON_NOTE,
        VSCODE,
    ),
    app(
        Base::Config,
        "Code/GPUCache",
        "VS Code: GPUCache",
        ELECTRON_NOTE,
        VSCODE,
    ),
    app(
        Base::Config,
        "Code/logs",
        "VS Code: logs",
        "Logs antigos do editor.",
        VSCODE,
    ),
    app(
        Base::Config,
        "Slack/Cache",
        "Slack: Cache",
        ELECTRON_NOTE,
        SLACK,
    ),
    app(
        Base::Config,
        "Slack/Code Cache",
        "Slack: Code Cache",
        ELECTRON_NOTE,
        SLACK,
    ),
    app(
        Base::Config,
        "Slack/GPUCache",
        "Slack: GPUCache",
        ELECTRON_NOTE,
        SLACK,
    ),
    app(
        Base::Config,
        "Slack/logs",
        "Slack: logs",
        "Logs antigos do Slack.",
        SLACK,
    ),
    app(
        Base::Config,
        "discord/Cache",
        "Discord: Cache",
        ELECTRON_NOTE,
        DISCORD,
    ),
    app(
        Base::Config,
        "discord/Code Cache",
        "Discord: Code Cache",
        ELECTRON_NOTE,
        DISCORD,
    ),
    app(
        Base::Config,
        "discord/GPUCache",
        "Discord: GPUCache",
        ELECTRON_NOTE,
        DISCORD,
    ),
];

/// Uniao dos processos de todas as entradas; usada pelo executor.
const WATCHED: [&str; 9] = [
    "chrome",
    "chromium",
    "chromium-browse",
    "firefox",
    "firefox-esr",
    "code",
    "slack",
    "spotify",
    "Discord",
];

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::AppCaches,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct AppCaches;

impl Category for AppCaches {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        let paths: Vec<PathBuf> = APP_CACHES
            .iter()
            .map(|app| app.cache.path(ctx.env))
            .collect();
        parent_roots(paths.iter().map(PathBuf::as_path))
    }

    fn watched_processes(&self) -> &'static [&'static str] {
        &WATCHED
    }

    /// Na hora de limpar, o item so fica bloqueado pelo proprio app. Item
    /// que nao casa com nenhuma entrada fica com todos (falha fechado).
    fn watched_processes_for(&self, finding: &Finding) -> Vec<&'static str> {
        let Some(path) = finding.action.path() else {
            return WATCHED.to_vec();
        };
        APP_CACHES
            .iter()
            .find(|app| path.ends_with(Path::new(app.cache.relative)))
            .map_or_else(|| WATCHED.to_vec(), |app| app.watched.to_vec())
    }

    /// Cada item so fica bloqueado pelo proprio app (S6).
    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let entries: Vec<(&[&str], CacheEntry)> = APP_CACHES
            .iter()
            .map(|app| (app.watched, CacheEntry::known(&app.cache, ctx.env)))
            .collect();
        collect_findings_watched(
            ctx,
            entries
                .iter()
                .map(|(watched, entry)| (*watched, entry.item(CategoryId::AppCaches))),
        )
    }
}

#[cfg(test)]
mod tests;
