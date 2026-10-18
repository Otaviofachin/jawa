//! Leituras do Docker (S1: so comandos de leitura) e interpretacao das saidas.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use super::{ScanContext, ScanProblem};
use crate::model::finding::{CommandSpec, DOCKER_CONTEXT_VAR};
use crate::system::runner::{CommandOutput, CommandRunner, RunError};

pub(super) const DF_ARGS: [&str; 4] = ["system", "df", "--format", "{{json .}}"];
pub(super) const DF_VERBOSE_ARGS: [&str; 5] = ["system", "df", "-v", "--format", "{{json .}}"];
pub(super) const DANGLING_ARGS: [&str; 6] = [
    "volume",
    "ls",
    "--filter",
    "dangling=true",
    "--format",
    "{{json .}}",
];
/// Containers parados (os estados que nao rodam), com id e nome.
pub(super) const STOPPED_ARGS: [&str; 10] = [
    "ps",
    "-a",
    "--filter",
    "status=exited",
    "--filter",
    "status=created",
    "--filter",
    "status=dead",
    "--format",
    "{{.ID}}\t{{.Names}}",
];
pub(super) const CONTEXT_ARGS: [&str; 2] = ["context", "show"];
pub(super) const ENGINE_DOWN_MARKERS: [&str; 3] = [
    "Cannot connect",
    "Is the docker daemon running",
    "error during connect",
];
pub(super) const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
pub(super) const UNKNOWN_CONTEXT: &str = "docker (contexto desconhecido)";
pub(super) const NO_ANSWER: &str = "motor parado ou sem resposta";

/// O executavel do Docker e o tempo maximo de cada leitura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Docker {
    pub exe: PathBuf,
    pub read_timeout: Duration,
    /// Contexto ativo na medicao; a limpeza roda nele mesmo.
    pub context: Option<String>,
}

/// Bytes recuperaveis segundo `docker system df`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reclaimable {
    pub images: u64,
    pub build_cache: u64,
    pub containers: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub name: String,
    pub compose_project: Option<String>,
    pub bytes: u64,
}

#[derive(Deserialize)]
pub(super) struct DfRow {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Reclaimable", default)]
    reclaimable: String,
}

#[derive(Deserialize)]
pub(super) struct VolumeRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Labels", default)]
    labels: String,
}

#[derive(Deserialize)]
pub(super) struct VolumeSize {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Size", default)]
    size: String,
    #[serde(rename = "Links", default)]
    links: String,
}

/// Tamanho e numero de containers ligados, segundo `docker system df -v`.
pub(super) struct VolumeUsage {
    bytes: u64,
    linked: bool,
}

#[derive(Deserialize)]
pub(super) struct DfVerbose {
    #[serde(rename = "Volumes", default)]
    volumes: Vec<VolumeSize>,
}

/// Tamanho em unidades decimais do Docker (`3.844GB`, `512kB`, `0B`).
pub fn parse_decimal_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let unit_start = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = text.split_at(unit_start);
    let value: f64 = number.trim().parse().ok()?;
    let multiplier = match unit {
        "B" => 1.0,
        "kB" | "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        "PB" => 1e15,
        _ => return None,
    };
    let bytes = (value * multiplier).round();
    (bytes.is_finite() && bytes >= 0.0).then_some(bytes as u64)
}

/// Espaco recuperavel de imagens, cache de build e containers parados.
pub fn reclaimable(
    runner: &dyn CommandRunner,
    docker: &Docker,
) -> Result<Reclaimable, ScanProblem> {
    let stdout = read(runner, docker, &DF_ARGS)?;
    let mut space = Reclaimable::default();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let row: DfRow = serde_json::from_str(line).map_err(|_| unexpected_output(&DF_ARGS))?;
        let slot = match row.kind.as_str() {
            "Images" => &mut space.images,
            "Build Cache" => &mut space.build_cache,
            "Containers" => &mut space.containers,
            _ => continue,
        };
        let size = row
            .reclaimable
            .split_whitespace()
            .next()
            .unwrap_or_default();
        *slot = parse_decimal_size(size).ok_or_else(|| unexpected_output(&DF_ARGS))?;
    }
    Ok(space)
}

/// Volumes sem nenhum container (nem parado), com o projeto do compose e o
/// tamanho do `docker system df -v`.
pub fn dangling_volumes(
    runner: &dyn CommandRunner,
    docker: &Docker,
) -> Result<Vec<VolumeInfo>, ScanProblem> {
    let listing = read(runner, docker, &DANGLING_ARGS)?;
    let rows = listing
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<VolumeRow>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| unexpected_output(&DANGLING_ARGS))?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let usage = volume_usage(runner, docker)?;
    Ok(rows
        .into_iter()
        .filter(|row| !usage.get(&row.name).is_some_and(|volume| volume.linked))
        .map(|row| VolumeInfo {
            bytes: usage.get(&row.name).map_or(0, |volume| volume.bytes),
            compose_project: compose_project(&row.labels),
            name: row.name,
        })
        .collect())
}

/// Um volume que o `df -v` mostra ligado a algum container fica de fora,
/// mesmo que o filtro `dangling` o tenha listado (S10).
pub(super) fn volume_usage(
    runner: &dyn CommandRunner,
    docker: &Docker,
) -> Result<HashMap<String, VolumeUsage>, ScanProblem> {
    let stdout = read(runner, docker, &DF_VERBOSE_ARGS)?;
    let verbose: DfVerbose =
        serde_json::from_str(stdout.trim()).map_err(|_| unexpected_output(&DF_VERBOSE_ARGS))?;
    Ok(verbose
        .volumes
        .into_iter()
        .map(|volume| {
            let usage = VolumeUsage {
                bytes: parse_decimal_size(&volume.size).unwrap_or(0),
                linked: volume.links.parse::<u64>().is_ok_and(|links| links > 0),
            };
            (volume.name, usage)
        })
        .collect())
}

/// `Labels` vem como `chave=valor,chave=valor`.
pub(super) fn compose_project(labels: &str) -> Option<String> {
    labels
        .split(',')
        .filter_map(|label| label.split_once('='))
        .find(|(key, _)| *key == COMPOSE_PROJECT_LABEL)
        .map(|(_, value)| value.to_string())
        .filter(|project| !project.is_empty())
}

/// Um container parado medido: o id vai para o `container rm`, o nome para a nota.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoppedContainer {
    pub id: String,
    pub name: String,
}

/// Containers parados agora; vazio se a leitura falhar (nada e oferecido).
pub(super) fn stopped_containers(
    runner: &dyn CommandRunner,
    docker: &Docker,
) -> Vec<StoppedContainer> {
    let Ok(stdout) = read(runner, docker, &STOPPED_ARGS) else {
        return Vec::new();
    };
    stdout
        .lines()
        .filter_map(|line| {
            let (id, name) = line.trim().split_once('\t')?;
            let is_id = !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit());
            is_id.then(|| StoppedContainer {
                id: id.to_string(),
                name: name.trim().to_string(),
            })
        })
        .collect()
}

/// Roda uma leitura. Tempo esgotado ou motor fora do ar viram
/// `EngineStopped` com o nome do contexto.
pub(super) fn read(
    runner: &dyn CommandRunner,
    docker: &Docker,
    args: &[&str],
) -> Result<String, ScanProblem> {
    match run(runner, docker, args) {
        Ok(output) if output.success => Ok(output.stdout),
        Ok(output) if engine_is_down(&output) => Err(engine_stopped(runner, docker, None)),
        Ok(output) => Err(ScanProblem::Failed(failure_text(&output, args))),
        Err(RunError::TimedOut) => Err(engine_stopped(runner, docker, Some(NO_ANSWER))),
        Err(RunError::NotFound) => Err(ScanProblem::ToolMissing("docker")),
        Err(RunError::Io(message)) => Err(ScanProblem::Failed(message)),
    }
}

pub(super) fn run(
    runner: &dyn CommandRunner,
    docker: &Docker,
    args: &[&str],
) -> Result<CommandOutput, RunError> {
    runner.run(&CommandSpec {
        program: docker.exe.clone(),
        args: args.iter().map(|arg| arg.to_string()).collect(),
        stdin: None,
        env: context_env(docker),
        timeout: docker.read_timeout,
    })
}

pub(super) fn engine_is_down(output: &CommandOutput) -> bool {
    ENGINE_DOWN_MARKERS
        .iter()
        .any(|marker| output.stderr.contains(marker))
}

/// `docker context show` le so a configuracao local: responde mesmo com o
/// motor parado.
pub(super) fn engine_stopped(
    runner: &dyn CommandRunner,
    docker: &Docker,
    detail: Option<&str>,
) -> ScanProblem {
    let context = docker
        .context
        .clone()
        .or_else(|| current_context(runner, docker))
        .unwrap_or_else(|| UNKNOWN_CONTEXT.to_string());
    ScanProblem::EngineStopped(match detail {
        Some(detail) => format!("{context} ({detail})"),
        None => context,
    })
}

pub(super) fn failure_text(output: &CommandOutput, args: &[&str]) -> String {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        format!(
            "docker {} terminou com código {:?}",
            args.join(" "),
            output.code
        )
    } else {
        stderr.to_string()
    }
}

pub(super) fn unexpected_output(args: &[&str]) -> ScanProblem {
    ScanProblem::Failed(format!("saída inesperada de docker {}", args.join(" ")))
}

pub(super) fn docker_for(ctx: &ScanContext) -> Result<Docker, ScanProblem> {
    let exe = ctx
        .env
        .tools
        .docker
        .clone()
        .ok_or(ScanProblem::ToolMissing("docker"))?;
    let mut docker = Docker {
        exe,
        read_timeout: ctx.read_timeout,
        context: None,
    };
    docker.context = current_context(ctx.runner, &docker);
    Ok(docker)
}

/// `docker context show` le so a configuracao local; `None` se nao responder.
fn current_context(runner: &dyn CommandRunner, docker: &Docker) -> Option<String> {
    run(runner, docker, &CONTEXT_ARGS)
        .ok()
        .filter(|output| output.success)
        .map(|output| output.stdout.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Fixa o contexto medido no comando (leitura ou limpeza).
pub(super) fn context_env(docker: &Docker) -> Vec<(String, String)> {
    docker
        .context
        .iter()
        .map(|name| (DOCKER_CONTEXT_VAR.to_string(), name.clone()))
        .collect()
}
