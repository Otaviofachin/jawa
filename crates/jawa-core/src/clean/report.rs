//! Resultado da limpeza: motivo de cada pulo, desfecho por item e relatorio final.

use std::fmt;

use crate::model::finding::{Action, Finding};
use crate::safety::boundary::BoundaryViolation;
use crate::system::processes::{Holder, describe_holders};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    GuidedOnly,
    IrreversibleNotConfirmed,
    Boundary(BoundaryViolation),
    Changed,
    InUse(Vec<Holder>),
    ForeignOwned { fix: String },
    Unmeasurable(String),
    Cancelled,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkipReason::GuidedOnly => formatter.write_str("modo guiado: rode o comando você mesmo"),
            SkipReason::IrreversibleNotConfirmed => {
                formatter.write_str("irreversível e sem confirmação")
            }
            SkipReason::Boundary(violation) => write!(formatter, "recusado: {violation}"),
            SkipReason::Changed => formatter.write_str("mudou desde a medição"),
            SkipReason::InUse(holders) => {
                write!(formatter, "em uso por {}", describe_holders(holders))
            }
            SkipReason::ForeignOwned { fix } => {
                write!(formatter, "há arquivos de outro dono; para corrigir: {fix}")
            }
            SkipReason::Unmeasurable(error) => {
                write!(formatter, "não foi possível medir o item: {error}")
            }
            SkipReason::Cancelled => formatter.write_str("cancelado"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    Done { output: Option<String> },
    Skipped(SkipReason),
    Failed(String),
}

impl fmt::Display for ItemOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemOutcome::Done { .. } => formatter.write_str("feito"),
            ItemOutcome::Skipped(reason) => write!(formatter, "pulado: {reason}"),
            ItemOutcome::Failed(message) => write!(formatter, "falhou: {message}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CleanReport {
    pub items: Vec<(Finding, ItemOutcome)>,
    pub estimated_bytes: u64,
    pub free_before: u64,
    pub free_after: u64,
    pub warnings: Vec<String>,
}

impl CleanReport {
    /// Espaco realmente liberado: diferenca do espaco livre (nunca negativa).
    pub fn measured_bytes(&self) -> u64 {
        self.free_after.saturating_sub(self.free_before)
    }

    /// Itens feitos que foram para a Lixeira: so voltam ao disco quando ela e
    /// esvaziada.
    pub fn trashed_bytes(&self) -> u64 {
        self.done_bytes(|finding| matches!(finding.action, Action::Trash(_)))
    }

    /// Itens feitos dentro da VM do Docker Desktop: so voltam ao disco quando
    /// o disco virtual e recuperado.
    pub fn docker_desktop_bytes(&self) -> u64 {
        self.done_bytes(
            |finding| matches!(&finding.action, Action::Run(spec) if spec.runs_in_docker_desktop()),
        )
    }

    /// O que deveria aparecer como espaco livre agora: o estimado sem a
    /// Lixeira e sem a VM do Docker Desktop.
    pub fn on_disk_estimate(&self) -> u64 {
        self.estimated_bytes
            .saturating_sub(self.trashed_bytes())
            .saturating_sub(self.docker_desktop_bytes())
    }

    /// Medido abaixo da metade do que deveria ter voltado ao disco: o espaco
    /// pode estar preso em snapshots.
    pub fn space_may_be_stuck(&self) -> bool {
        let expected = self.on_disk_estimate();
        expected > 0 && self.measured_bytes() < expected / 2
    }

    fn done_bytes(&self, matches: impl Fn(&Finding) -> bool) -> u64 {
        self.items
            .iter()
            .filter(|(finding, outcome)| {
                matches!(outcome, ItemOutcome::Done { .. }) && matches(finding)
            })
            .map(|(finding, _)| finding.bytes)
            .sum()
    }
}
