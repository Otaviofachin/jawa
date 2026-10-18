//! `docker-imagens`: cache de build e imagens sem container.

use std::path::PathBuf;

use super::{Category, Descriptor, MIN_ITEM_BYTES, ScanContext, ScanOutput, ScanProblem};
use crate::model::finding::{CategoryId, Finding, Reversibility, Risk};

use super::client::{docker_for, reclaimable};
use super::{CommandItem, args, command_finding, texts};

pub(super) const BUILD_CACHE_NOTE: &str =
    "Camadas guardadas pelo BuildKit; o próximo build fica mais lento.";
pub(super) const IMAGES_NOTE: &str =
    "Imagens que nenhum container usa; voltam com `docker pull` ou com o próximo build.";

static IMAGES_DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::DockerImages,
    title: texts::IMAGES_TITLE,
    summary: texts::IMAGES_SUMMARY,
    explanation: texts::IMAGES_EXPLANATION,
};

/// Cache de build e imagens sem container, com o tamanho do `docker system df`.
pub struct DockerImages;

impl Category for DockerImages {
    fn descriptor(&self) -> &'static Descriptor {
        &IMAGES_DESCRIPTOR
    }

    fn roots(&self, _ctx: &ScanContext) -> Vec<PathBuf> {
        Vec::new()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let docker = docker_for(ctx)?;
        let space = reclaimable(ctx.runner, &docker)?;
        let findings: Vec<Finding> = [
            prune_item(
                space.build_cache,
                "builder",
                "Cache de build do Docker",
                BUILD_CACHE_NOTE,
            ),
            prune_item(space.images, "image", "Imagens sem container", IMAGES_NOTE),
        ]
        .into_iter()
        .filter(|item| item.bytes >= MIN_ITEM_BYTES)
        .map(|item| command_finding(&docker, item))
        .collect();
        Ok(findings.into())
    }
}

/// `docker <object> prune -a -f`: seguro e regeneravel.
pub(super) fn prune_item(bytes: u64, object: &str, label: &str, note: &str) -> CommandItem {
    CommandItem {
        category: CategoryId::DockerImages,
        args: args(&[object, "prune", "-a", "-f"]),
        label: label.to_string(),
        bytes,
        risk: Risk::Safe,
        reversibility: Reversibility::Regenerable,
        note: note.to_string(),
    }
}
