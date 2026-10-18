//! `docker-desktop-disco`: o Docker.raw nunca e apagado com o Docker Desktop
//! instalado (S9).

use std::io;
use std::path::{Path, PathBuf};

use super::texts;
use crate::model::finding::{
    Action, CategoryId, Finding, FindingId, GuideSpec, Reversibility, Risk,
};
use crate::scan::categories::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::scan::paths::{PathItem, collect_findings, display_location};
use crate::scan::{MIN_ITEM_BYTES, ScanContext};
use crate::system::measure::measure;

pub const RECLAIM_COMMAND: &str = "docker run --privileged --pid=host docker/desktop-reclaim-space";
const DESKTOP_DIR: &str = ".docker/desktop";
const DESKTOP_PROCESSES: [&str; 2] = ["com.docker.back", "docker-desktop"];
const VMS_DIR: &str = "vms";
const RAW_DISK: &str = "vms/0/data/Docker.raw";
const INSTALLED_NOTE: &str = "Nunca apague o Docker.raw com o Docker Desktop instalado: ele \
                              contém todas as imagens, containers e volumes.";
const REMOVED_NOTE: &str = "O Docker Desktop não está instalado: este disco sobrou com as \
                            imagens, containers e volumes antigos. Apagar é definitivo.";

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::DockerDesktopDisk,
    title: texts::DESKTOP_TITLE,
    summary: texts::DESKTOP_SUMMARY,
    explanation: texts::DESKTOP_EXPLANATION,
};

/// O disco virtual do Docker Desktop: guiado se instalado (S9), Revisar se nao.
pub struct DockerDesktopDisk;

impl Category for DockerDesktopDisk {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    /// Sem raizes com o Docker Desktop instalado: um `Delete` medido antes da
    /// instalacao e recusado pela fronteira na hora de limpar (S9).
    fn roots(&self, ctx: &ScanContext) -> Vec<PathBuf> {
        if ctx.system.docker_desktop_install.exists() {
            return Vec::new();
        }
        vec![desktop_dir(ctx)]
    }

    /// Processos do Docker Desktop (o `comm` e cortado em 15 caracteres).
    fn watched_processes(&self) -> &'static [&'static str] {
        &DESKTOP_PROCESSES
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        if ctx.system.docker_desktop_install.exists() {
            return Ok(guided_finding(ctx)?.into_iter().collect::<Vec<_>>().into());
        }
        let vms = desktop_dir(ctx).join(VMS_DIR);
        let item = PathItem {
            category: CategoryId::DockerDesktopDisk,
            path: &vms,
            label: "Disco virtual do Docker Desktop (desinstalado)".to_string(),
            risk: Risk::Review,
            reversibility: Reversibility::Irreversible,
            note: Some(REMOVED_NOTE.to_string()),
            trash: false,
        };
        collect_findings(ctx, self, [item])
    }
}

fn desktop_dir(ctx: &ScanContext) -> PathBuf {
    ctx.env.home.join(DESKTOP_DIR)
}

/// Com o Docker Desktop instalado, so o comando oficial. O tamanho e o real
/// (blocos alocados): o Docker.raw e esparso e o aparente engana.
fn guided_finding(ctx: &ScanContext) -> Result<Option<Finding>, ScanProblem> {
    let raw = desktop_dir(ctx).join(RAW_DISK);
    let Some(bytes) = allocated_bytes(&raw, ctx.env.uid)? else {
        return Ok(None);
    };
    if bytes < MIN_ITEM_BYTES {
        return Ok(None);
    }
    let action = Action::Guide(GuideSpec {
        command_line: RECLAIM_COMMAND.to_string(),
    });
    Ok(Some(Finding {
        id: FindingId::of(CategoryId::DockerDesktopDisk, &action),
        category: CategoryId::DockerDesktopDisk,
        label: "Disco virtual do Docker Desktop".to_string(),
        location: display_location(&raw, &ctx.env.home),
        bytes,
        risk: Risk::Guided,
        reversibility: Reversibility::Irreversible,
        note: Some(format!(
            "{INSTALLED_NOTE} Para recuperar espaço, rode: {RECLAIM_COMMAND}"
        )),
        action,
        fingerprint: None,
        blocked: None,
    }))
}

fn allocated_bytes(path: &Path, uid: u32) -> Result<Option<u64>, ScanProblem> {
    match measure(path, uid) {
        Ok(measurement) => Ok(Some(measurement.allocated_bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            tracing::warn!(?path, %error, "não foi possível medir o Docker.raw");
            Err(ScanProblem::NoAccess(path.display().to_string()))
        }
    }
}

#[cfg(test)]
mod tests;
