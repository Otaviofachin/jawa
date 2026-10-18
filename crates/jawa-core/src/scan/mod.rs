//! Medicao: contexto, medicao de caminhos, categorias, descoberta de projetos e avisos cruzados.

pub mod categories;
pub mod paths;
pub mod projects;
pub mod warnings;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::finding::CategoryId;
use crate::safety::ledger::Ledger;
use crate::system::config::Config;
use crate::system::env::Environment;
use crate::system::runner::CommandRunner;
use categories::{ScanOutput, ScanProblem, registry};

/// Item com caminho abaixo disso nao e oferecido.
pub const MIN_ITEM_BYTES: u64 = 1_048_576;
/// Tempo maximo de um comando de leitura (medicao).
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Tempo maximo de um comando de limpeza.
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Caminhos do sistema, trocados por pastas temporarias nos testes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPaths {
    pub docker_desktop_install: PathBuf,
    pub snaps_dir: PathBuf,
    pub apt_archives: PathBuf,
    pub crash_dir: PathBuf,
}

impl SystemPaths {
    /// Os mesmos caminhos, debaixo de `root` (a raiz `/` da maquina real).
    pub fn under(root: &Path) -> SystemPaths {
        SystemPaths {
            docker_desktop_install: root.join("opt/docker-desktop"),
            snaps_dir: root.join("var/lib/snapd/snaps"),
            apt_archives: root.join("var/cache/apt/archives"),
            crash_dir: root.join("var/crash"),
        }
    }
}

impl Default for SystemPaths {
    fn default() -> SystemPaths {
        SystemPaths::under(Path::new("/"))
    }
}

pub struct ScanContext<'a> {
    pub env: &'a Environment,
    pub config: &'a Config,
    pub project_roots: Vec<PathBuf>,
    pub runner: &'a dyn CommandRunner,
    pub read_timeout: Duration,
    pub system: SystemPaths,
    /// Variaveis extras para o git; vazio em producao.
    pub git_env: Vec<(String, String)>,
}

/// Junta o rodape da medicao dos caminhos com os avisos da propria categoria.
pub(crate) fn join_footnotes(parts: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    let joined = parts.into_iter().flatten().collect::<Vec<_>>().join(" ");
    (!joined.is_empty()).then_some(joined)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryScan {
    pub id: CategoryId,
    pub result: Result<ScanOutput, ScanProblem>,
}

/// Mede todas as categorias (ou so `only`) e monta o ledger com os itens.
pub fn scan_all(
    ctx: &ScanContext,
    only: Option<CategoryId>,
    on_done: &dyn Fn(&CategoryScan),
) -> (Ledger, Vec<CategoryScan>) {
    let mut findings = Vec::new();
    let mut scans = Vec::new();
    for category in registry() {
        let id = category.descriptor().id;
        if only.is_some_and(|wanted| wanted != id) {
            continue;
        }
        let scan = CategoryScan {
            id,
            result: category.scan(ctx),
        };
        if let Ok(output) = &scan.result {
            findings.extend(output.findings.iter().cloned());
        }
        on_done(&scan);
        scans.push(scan);
    }
    (Ledger::new(ctx.env, findings), scans)
}
