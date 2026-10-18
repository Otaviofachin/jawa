mod texts;

use std::path::PathBuf;

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Reversibility, Risk};
use crate::scan::ScanContext;
use crate::scan::paths::{PathItem, collect_findings};

const CACHE_DIR: &str = ".npm";

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::NpmCache,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

/// `~/.npm/_cacache` e `~/.npm/_npx`.
pub struct NpmCache;

impl Category for NpmCache {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        vec![ctx.env.home.join(CACHE_DIR)]
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let entries = [
            ("_cacache", "Cache de pacotes do npm"),
            ("_npx", "Pacotes rodados pelo npx"),
        ];
        let paths =
            entries.map(|(directory, label)| (ctx.env.home.join(CACHE_DIR).join(directory), label));
        let items = paths.iter().map(|(path, label)| PathItem {
            category: CategoryId::NpmCache,
            path,
            label: label.to_string(),
            risk: Risk::Safe,
            reversibility: Reversibility::Regenerable,
            note: None,
            trash: false,
        });
        collect_findings(ctx, self, items)
    }
}

#[cfg(test)]
mod tests;
