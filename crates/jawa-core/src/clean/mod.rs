//! Limpeza: resolve os ids, passa cada item pelas travas, executa e audita.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Instant, UNIX_EPOCH};

use crate::error::CoreError;
use crate::model::finding::{Finding, FindingId};
use crate::safety::audit::{AuditEntry, AuditLog};
use crate::safety::ledger::Ledger;
use crate::system::disk::free_bytes;
use crate::system::env::Environment;
use crate::system::runner::CommandRunner;

mod perform;
mod precheck;
mod report;
#[cfg(test)]
mod tests;

use perform::perform;
pub(crate) use precheck::chown_fix;
use precheck::precheck;
pub use report::{CleanReport, ItemOutcome, Progress, SkipReason};

/// Destino dos itens da categoria `downloads`: a Lixeira.
pub trait Discard: Sync {
    fn discard(&self, path: &Path) -> io::Result<()>;
}

/// Lixeira do freedesktop, via crate `trash`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTrash;

impl Discard for SystemTrash {
    fn discard(&self, path: &Path) -> io::Result<()> {
        trash::delete(path).map_err(|error| io::Error::other(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanRequest {
    pub ids: Vec<FindingId>,
    pub allow_irreversible: bool,
}

pub struct ExecContext<'a> {
    pub env: &'a Environment,
    pub runner: &'a dyn CommandRunner,
    pub trash: &'a dyn Discard,
    pub audit: &'a AuditLog,
    pub roots_for: &'a dyn Fn(&Finding) -> Vec<PathBuf>,
    pub watched_names_for: &'a dyn Fn(&Finding) -> Vec<&'static str>,
    /// Pasta vigiada por processos (S6); `None` e o proprio alvo.
    pub in_use_scope_for: &'a dyn Fn(&Finding) -> Option<PathBuf>,
}

/// Limpa os itens pedidos (S2-S8, S11, S12). Resolve todos os ids antes de
/// agir; cada item passa pelas travas de novo e vai para o historico.
pub fn clean(
    ledger: &mut Ledger,
    request: &CleanRequest,
    ctx: &ExecContext,
    on_progress: &dyn Fn(&Progress),
    cancel: &AtomicBool,
) -> Result<CleanReport, CoreError> {
    let selected = resolve_all(ledger, &request.ids)?;
    let started = Instant::now();
    let free_before = free_space(ctx.env);
    let total = selected.len();
    let mut items = Vec::with_capacity(total);
    let mut warnings = Vec::new();
    for (index, finding) in selected.into_iter().enumerate() {
        let skip = precheck(&finding, request, ctx, cancel);
        on_progress(&Progress {
            done: index,
            total,
            current: Some(finding.label.clone()),
        });
        let outcome = match skip {
            Some(reason) => ItemOutcome::Skipped(reason),
            None => perform(&finding, ctx),
        };
        warnings.extend(record_audit(ctx, started, &finding, &outcome));
        items.push((finding, outcome));
    }
    on_progress(&Progress {
        done: total,
        total,
        current: None,
    });
    Ok(finish(ledger, items, warnings, free_before, ctx))
}

/// Qualquer id fora do ledger aborta tudo (S2). A ordem segue a do ledger.
fn resolve_all(ledger: &Ledger, ids: &[FindingId]) -> Result<Vec<Finding>, CoreError> {
    if let Some(unknown) = ids.iter().find(|id| ledger.find(id).is_none()) {
        return Err(CoreError::UnknownId(unknown.clone()));
    }
    Ok(ledger
        .findings
        .iter()
        .filter(|finding| ids.contains(&finding.id))
        .cloned()
        .collect())
}

/// Devolve um aviso em PT-BR quando a linha do historico nao pode ser gravada.
/// O horario de cada linha e o fim do item: o relogio do ambiente mais o tempo
/// desde o inicio da limpeza (testavel e sem pular para tras).
fn record_audit(
    ctx: &ExecContext,
    started: Instant,
    finding: &Finding,
    outcome: &ItemOutcome,
) -> Option<String> {
    let finished_at = ctx.env.now + started.elapsed();
    let entry = AuditEntry {
        at_secs: finished_at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs()),
        id: finding.id.clone(),
        category: finding.category.to_string(),
        location: finding.location.clone(),
        bytes: finding.bytes,
        outcome: outcome.to_string(),
    };
    let error = ctx.audit.record(&entry).err()?;
    tracing::warn!(%error, id = %finding.id, "não foi possível gravar o histórico");
    Some(format!(
        "não foi possível gravar o histórico do item {} ({}): {error}",
        finding.id, finding.location
    ))
}

/// Esquece os itens feitos, com falha ou que mudaram, e grava o ledger. Uma
/// falha ao gravar vira aviso: o relatorio do que ja foi feito vale mais.
fn finish(
    ledger: &mut Ledger,
    items: Vec<(Finding, ItemOutcome)>,
    mut warnings: Vec<String>,
    free_before: u64,
    ctx: &ExecContext,
) -> CleanReport {
    let settled: Vec<FindingId> = items
        .iter()
        .filter(|(_, outcome)| is_settled(outcome))
        .map(|(finding, _)| finding.id.clone())
        .collect();
    ledger.forget(&settled);
    if let Err(error) = ledger.save(ctx.env) {
        tracing::warn!(%error, "não foi possível gravar o ledger depois da limpeza");
        warnings.push(format!(
            "não foi possível gravar a medição atualizada: {error}"
        ));
    }
    let estimated_bytes = items
        .iter()
        .filter(|(_, outcome)| matches!(outcome, ItemOutcome::Done { .. }))
        .map(|(finding, _)| finding.bytes)
        .sum();
    CleanReport {
        items,
        estimated_bytes,
        free_before,
        free_after: free_space(ctx.env),
        warnings,
    }
}

fn is_settled(outcome: &ItemOutcome) -> bool {
    matches!(
        outcome,
        ItemOutcome::Done { .. }
            | ItemOutcome::Failed(_)
            | ItemOutcome::Skipped(SkipReason::Changed)
    )
}

fn free_space(env: &Environment) -> u64 {
    free_bytes(&env.home).unwrap_or_else(|error| {
        tracing::warn!(%error, "não foi possível medir o espaço livre");
        0
    })
}
