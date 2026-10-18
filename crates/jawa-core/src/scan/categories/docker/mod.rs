//! `docker-imagens` e `docker-volumes`. Medir so roda comandos de leitura (S1);
//! cada volume solto vira um item proprio, removido por nome (S10).

mod client;
mod desktop;
mod images;
mod texts;
mod volumes;

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{
    Action, CategoryId, CommandSpec, Finding, FindingId, Reversibility, Risk,
};
use crate::scan::{ACTION_TIMEOUT, MIN_ITEM_BYTES, ScanContext};

use client::{Docker, context_env};
pub use desktop::{DockerDesktopDisk, RECLAIM_COMMAND};
pub use images::DockerImages;
pub use volumes::DockerVolumes;

/// Um item que roda `docker <args>` na limpeza.
pub(super) struct CommandItem {
    pub(super) category: CategoryId,
    pub(super) args: Vec<String>,
    pub(super) label: String,
    pub(super) bytes: u64,
    pub(super) risk: Risk,
    pub(super) reversibility: Reversibility,
    pub(super) note: String,
}

const DESKTOP_VM_NOTE: &str = "Roda dentro da VM do Docker Desktop: o espaço só volta ao disco \
                               depois de recuperar o disco virtual (comando na tecla g).";

pub(super) fn command_finding(docker: &Docker, item: CommandItem) -> Finding {
    let location = format!("docker {}", item.args.join(" "));
    let action = Action::Run(CommandSpec {
        program: docker.exe.clone(),
        args: item.args,
        stdin: None,
        env: context_env(docker),
        timeout: ACTION_TIMEOUT,
    });
    let in_vm = matches!(&action, Action::Run(spec) if spec.runs_in_docker_desktop());
    let note = if in_vm {
        format!("{} {DESKTOP_VM_NOTE}", item.note)
    } else {
        item.note
    };
    Finding {
        id: FindingId::of(item.category, &action),
        category: item.category,
        label: item.label,
        location,
        bytes: item.bytes,
        risk: item.risk,
        reversibility: item.reversibility,
        note: Some(note),
        action,
        fingerprint: None,
        blocked: None,
    }
}

pub(super) fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

#[cfg(test)]
pub(crate) mod test_fixtures;

#[cfg(test)]
mod tests;
