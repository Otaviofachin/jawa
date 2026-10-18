//! `docker-volumes`: um item por volume solto (S10) e os containers parados medidos.

use std::path::PathBuf;

use super::{Category, Descriptor, MIN_ITEM_BYTES, ScanContext, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Finding, Reversibility, Risk};

use super::client::{
    StoppedContainer, VolumeInfo, dangling_volumes, docker_for, reclaimable, stopped_containers,
};
use super::{CommandItem, args, command_finding, texts};

pub(super) const VOLUME_NOTE: &str =
    "Pode ter dados de banco local (Postgres, MySQL, Redis). Apagar é definitivo.";
pub(super) const CONTAINERS_NOTE: &str =
    "Apaga os containers parados e o que foi gravado dentro deles. Apagar é definitivo.";

static VOLUMES_DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::DockerVolumes,
    title: texts::VOLUMES_TITLE,
    summary: texts::VOLUMES_SUMMARY,
    explanation: texts::VOLUMES_EXPLANATION,
};

/// Um item por volume solto (S10) e um item para os containers parados.
pub struct DockerVolumes;

impl Category for DockerVolumes {
    fn descriptor(&self) -> &'static Descriptor {
        &VOLUMES_DESCRIPTOR
    }

    fn roots(&self, _ctx: &ScanContext) -> Vec<PathBuf> {
        Vec::new()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let docker = docker_for(ctx)?;
        let space = reclaimable(ctx.runner, &docker)?;
        let (volumes, small): (Vec<VolumeInfo>, Vec<VolumeInfo>) =
            dangling_volumes(ctx.runner, &docker)?
                .into_iter()
                .partition(|volume| volume.bytes >= MIN_ITEM_BYTES);
        let mut findings: Vec<Finding> = volumes
            .into_iter()
            .map(|volume| command_finding(&docker, volume_item(volume)))
            .collect();
        let stopped = stopped_containers(ctx.runner, &docker);
        if !stopped.is_empty() {
            findings.push(command_finding(
                &docker,
                containers_item(space.containers, &stopped),
            ));
        }
        Ok(ScanOutput {
            findings,
            footnote: small_volumes_note(small.len()),
        })
    }
}

/// Volumes abaixo de 1 MiB (ou sem tamanho no `df -v`) nao sao oferecidos.
pub(super) fn small_volumes_note(count: usize) -> Option<String> {
    (count > 0).then(|| {
        format!("{count} volume(s) pequeno(s) ou de tamanho desconhecido ficaram de fora.")
    })
}

pub(super) fn volume_item(volume: VolumeInfo) -> CommandItem {
    let label = match &volume.compose_project {
        Some(project) => format!("{} (projeto {project})", volume.name),
        None => volume.name.clone(),
    };
    CommandItem {
        category: CategoryId::DockerVolumes,
        args: args(&["volume", "rm", &volume.name]),
        label,
        bytes: volume.bytes,
        risk: Risk::Review,
        reversibility: Reversibility::Irreversible,
        note: VOLUME_NOTE.to_string(),
    }
}

/// Remove so os containers medidos, pelo id (nunca `prune`, que levaria
/// tambem os que pararem depois da medicao).
pub(super) fn containers_item(bytes: u64, stopped: &[StoppedContainer]) -> CommandItem {
    let names: Vec<&str> = stopped
        .iter()
        .map(|container| container.name.as_str())
        .collect();
    let mut words = vec!["container", "rm"];
    words.extend(stopped.iter().map(|container| container.id.as_str()));
    CommandItem {
        category: CategoryId::DockerVolumes,
        args: args(&words),
        label: "Containers parados".to_string(),
        bytes,
        risk: Risk::Review,
        reversibility: Reversibility::Irreversible,
        note: format!("{CONTAINERS_NOTE} Parados: {}.", names.join(", ")),
    }
}
