//! Execucao de cada item: apagar, mandar para a Lixeira ou rodar o comando.

use std::fs;
use std::io;
use std::path::Path;

use crate::model::finding::{Action, CommandSpec, Finding};
use crate::safety::boundary::check_target;
use crate::system::runner::{CommandOutput, CommandRunner};

use super::{ExecContext, ItemOutcome, SkipReason};

const STDERR_TAIL_LINES: usize = 5;

pub(super) fn perform(finding: &Finding, ctx: &ExecContext) -> ItemOutcome {
    match &finding.action {
        Action::Delete(path) => act_on_path(finding, path, ctx, remove_path),
        Action::Trash(path) => act_on_path(finding, path, ctx, |target| ctx.trash.discard(target)),
        Action::Run(spec) => run_command(ctx.runner, spec),
        Action::Guide(_) => ItemOutcome::Skipped(SkipReason::GuidedOnly),
    }
}

/// Resolve a fronteira de novo logo antes de agir e age sobre o caminho
/// resolvido (pai canonico + nome), o mesmo que passou nas travas.
fn act_on_path(
    finding: &Finding,
    path: &Path,
    ctx: &ExecContext,
    act: impl Fn(&Path) -> io::Result<()>,
) -> ItemOutcome {
    let resolved = match check_target(path, &(ctx.roots_for)(finding), ctx.env) {
        Ok(resolved) => resolved,
        Err(violation) => return ItemOutcome::Skipped(SkipReason::Boundary(violation)),
    };
    match act(&resolved) {
        Ok(()) => ItemOutcome::Done { output: None },
        Err(error) => ItemOutcome::Failed(error.to_string()),
    }
}

/// Sem seguir symlink: um link e apagado como link.
fn remove_path(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn run_command(runner: &dyn CommandRunner, spec: &CommandSpec) -> ItemOutcome {
    match runner.run(spec) {
        Ok(output) if output.success => {
            let stdout = output.stdout.trim();
            ItemOutcome::Done {
                output: (!stdout.is_empty()).then(|| stdout.to_string()),
            }
        }
        Ok(output) => ItemOutcome::Failed(failure_message(&output)),
        Err(error) => ItemOutcome::Failed(error.to_string()),
    }
}

fn failure_message(output: &CommandOutput) -> String {
    let lines: Vec<&str> = output.stderr.trim_end().lines().collect();
    let tail = lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n");
    if tail.trim().is_empty() {
        "o comando terminou com erro".to_string()
    } else {
        tail
    }
}
