//! Contrato das categorias (trait, descritor, explicacao, problema) e registro.

use std::path::PathBuf;

use thiserror::Error;

use super::ScanContext;
use crate::model::finding::{CategoryId, Finding};

mod app_caches;
mod build_outputs;
mod docker;
mod downloads;
mod idle_node_modules;
mod npm_cache;
mod nvm_versions;
mod rust_targets;
mod system;
mod tool_caches;

pub use docker::RECLAIM_COMMAND;

/// Os seis campos de `jawa explicar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Explanation {
    pub what_it_is: &'static str,
    pub why_it_grows: &'static str,
    pub effect: &'static str,
    pub reversibility_detail: &'static str,
    pub recreate_cost: &'static str,
    pub recommendation: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    pub id: CategoryId,
    pub title: &'static str,
    pub summary: &'static str,
    pub explanation: Explanation,
}

/// Por que uma categoria nao mostra itens.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScanProblem {
    #[error("a ferramenta {0} não foi encontrada")]
    ToolMissing(&'static str),
    #[error("o serviço não está rodando: {0}")]
    EngineStopped(String),
    #[error("sem acesso: {0}")]
    NoAccess(String),
    #[error("nenhuma pasta de projetos foi encontrada; informe `project_roots` na configuração")]
    NoProjects,
    #[error("a medição falhou: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanOutput {
    pub findings: Vec<Finding>,
    pub footnote: Option<String>,
}

impl From<Vec<Finding>> for ScanOutput {
    fn from(findings: Vec<Finding>) -> ScanOutput {
        ScanOutput {
            findings,
            footnote: None,
        }
    }
}

pub trait Category: Sync {
    fn descriptor(&self) -> &'static Descriptor;
    /// Raizes da fronteira (S4) dos itens desta categoria.
    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf>;
    /// Nomes de processo que bloqueiam a categoria (S6).
    fn watched_processes(&self) -> &'static [&'static str] {
        &[]
    }
    /// Processos que bloqueiam um item na hora de limpar (S6). Uma categoria
    /// que vigia apps diferentes por item responde so os do item.
    fn watched_processes_for(&self, _finding: &Finding) -> Vec<&'static str> {
        self.watched_processes().to_vec()
    }
    /// Pasta onde um processo bloqueia o item na hora de limpar (S6); `None`
    /// e o proprio alvo.
    fn in_use_scope(&self, _finding: &Finding) -> Option<PathBuf> {
        None
    }
    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem>;
}

static REGISTRY: [&dyn Category; 12] = [
    &npm_cache::NpmCache,
    &tool_caches::ToolCaches,
    &app_caches::AppCaches,
    &build_outputs::BuildOutputs,
    &rust_targets::RustTargets,
    &idle_node_modules::IdleNodeModules,
    &nvm_versions::NvmVersions,
    &docker::DockerImages,
    &docker::DockerVolumes,
    &docker::DockerDesktopDisk,
    &downloads::Downloads,
    &system::System,
];

/// Categorias na ordem da tabela da especificacao.
pub fn registry() -> &'static [&'static dyn Category] {
    &REGISTRY
}

/// Categoria de um id. Enquanto as demais nao entram no registro, devolve uma
/// categoria sem raizes: toda acao fica recusada pela fronteira (falha fechado).
pub fn category(id: CategoryId) -> &'static dyn Category {
    registry()
        .iter()
        .copied()
        .find(|category| category.descriptor().id == id)
        .unwrap_or(&NotAvailable)
}

const NOT_AVAILABLE_EXPLANATION: Explanation = Explanation {
    what_it_is: "Categoria prevista, mas ainda não disponível nesta versão.",
    why_it_grows: "Não se aplica.",
    effect: "Nada é feito.",
    reversibility_detail: "Não se aplica.",
    recreate_cost: "Não se aplica.",
    recommendation: "Aguarde uma versão futura do Jawa.",
};

static NOT_AVAILABLE_DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::System,
    title: "Categoria indisponível",
    summary: "Esta categoria ainda não existe nesta versão do Jawa.",
    explanation: NOT_AVAILABLE_EXPLANATION,
};

struct NotAvailable;

impl Category for NotAvailable {
    fn descriptor(&self) -> &'static Descriptor {
        &NOT_AVAILABLE_DESCRIPTOR
    }

    fn roots(&self, _ctx: &ScanContext) -> Vec<PathBuf> {
        Vec::new()
    }

    fn scan(&self, _ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        Err(ScanProblem::Failed(
            "categoria ainda não disponível nesta versão".to_string(),
        ))
    }
}

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
