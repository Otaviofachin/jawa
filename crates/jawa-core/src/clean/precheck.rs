//! Travas conferidas antes de agir em cada item, na ordem da spec.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::finding::{Action, Finding, Reversibility, fingerprint_of};
use crate::safety::boundary::{BoundaryViolation, check_target};
use crate::system::env::Environment;
use crate::system::measure::measure;
use crate::system::processes::{holders_of, running_named};

use super::{CleanRequest, ExecContext, SkipReason};

/// Devolve o motivo para pular o item, parando na primeira trava que falha.
pub(super) fn precheck(
    finding: &Finding,
    request: &CleanRequest,
    ctx: &ExecContext,
    cancel: &AtomicBool,
) -> Option<SkipReason> {
    if cancel.load(Ordering::SeqCst) {
        return Some(SkipReason::Cancelled);
    }
    if matches!(finding.action, Action::Guide(_)) {
        return Some(SkipReason::GuidedOnly);
    }
    if finding.reversibility == Reversibility::Irreversible && !request.allow_irreversible {
        return Some(SkipReason::IrreversibleNotConfirmed);
    }
    let path = finding.action.path()?;
    check_path(finding, path, ctx).err()
}

/// Travas de caminho, na ordem: impressao digital (S3), fronteira (S4),
/// processos (S6), dono (S7) e montagem (S8). A impressao digital vem antes
/// porque a fronteira trata alvo sumido como ponto de montagem.
fn check_path(finding: &Finding, path: &Path, ctx: &ExecContext) -> Result<(), SkipReason> {
    let unchanged = finding
        .fingerprint
        .is_some_and(|measured| fingerprint_of(path).is_ok_and(|current| current == measured));
    if !unchanged {
        return Err(SkipReason::Changed);
    }
    let resolved =
        check_target(path, &(ctx.roots_for)(finding), ctx.env).map_err(SkipReason::Boundary)?;
    check_not_in_use(finding, &resolved, ctx)?;
    check_whole(&resolved, ctx.env)
}

fn check_not_in_use(
    finding: &Finding,
    resolved: &Path,
    ctx: &ExecContext,
) -> Result<(), SkipReason> {
    let scope = (ctx.in_use_scope_for)(finding);
    let mut holders = holders_of(scope.as_deref().unwrap_or(resolved), &ctx.env.proc_root);
    let watched = (ctx.watched_names_for)(finding);
    holders.extend(running_named(&watched, &ctx.env.proc_root));
    if holders.is_empty() {
        Ok(())
    } else {
        Err(SkipReason::InUse(holders))
    }
}

/// Nada pela metade (S7) e um sistema de arquivos so (S8). Se a medicao
/// falhar, o item e pulado sem agir.
fn check_whole(resolved: &Path, env: &Environment) -> Result<(), SkipReason> {
    let measurement = measure(resolved, env.uid).map_err(|error| {
        tracing::warn!(path = ?resolved, %error, "não foi possível medir o alvo antes de limpar");
        SkipReason::Unmeasurable(error.to_string())
    })?;
    if measurement.foreign_owned > 0 {
        return Err(SkipReason::ForeignOwned {
            fix: chown_fix(resolved),
        });
    }
    if measurement.crosses_mount {
        return Err(SkipReason::Boundary(BoundaryViolation::IsMountPoint));
    }
    Ok(())
}

/// Sugestao da S7: nunca `rm` com sudo, so devolver a posse.
pub(crate) fn chown_fix(path: &Path) -> String {
    let quoted = path.display().to_string().replace('\'', r"'\''");
    format!("sudo chown -R \"$USER\": '{quoted}'")
}
