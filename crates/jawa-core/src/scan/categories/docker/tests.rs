use std::time::Duration;

use super::client::*;
use super::test_fixtures::{ok, running_docker};
use super::*;
use crate::model::finding::{Action, CommandSpec, Finding, Reversibility, Risk};
use crate::scan::categories::docker::DockerDesktopDisk;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};
use crate::scan::{ACTION_TIMEOUT, scan_all};
use crate::system::runner::{CommandOutput, FakeRunner};
use std::path::PathBuf;

const DOCKER_EXE: &str = "/usr/bin/docker";

fn with_docker() -> Fixture {
    let mut fx = fixture();
    fx.env.tools.docker = Some(PathBuf::from(DOCKER_EXE));
    fx
}

fn docker() -> Docker {
    Docker {
        exe: PathBuf::from(DOCKER_EXE),
        read_timeout: Duration::from_secs(1),
        context: None,
    }
}

fn spec(finding: &Finding) -> &CommandSpec {
    match &finding.action {
        Action::Run(spec) => spec,
        other => panic!("esperava Run, veio {other:?}"),
    }
}

fn daemon_down() -> FakeRunner {
    FakeRunner::new()
        .on(
            &["system", "df"],
            CommandOutput {
                success: false,
                code: Some(1),
                stdout: String::new(),
                stderr: "Cannot connect to the Docker daemon at \
                         unix:///home/eu/.docker/desktop/docker.sock. Is the docker daemon \
                         running?\n"
                    .to_string(),
            },
        )
        .on(&["context", "show"], ok("desktop-linux\n"))
}

#[test]
fn parses_sizes() {
    assert_eq!(parse_decimal_size("3.844GB"), Some(3_844_000_000));
    assert_eq!(parse_decimal_size("85.95MB"), Some(85_950_000));
    assert_eq!(parse_decimal_size("512kB"), Some(512_000));
    assert_eq!(parse_decimal_size("0B"), Some(0));
    assert_eq!(parse_decimal_size("1.5TB"), Some(1_500_000_000_000));
    assert_eq!(parse_decimal_size(" 61.44kB "), Some(61_440));
    assert_eq!(parse_decimal_size("N/A"), None);
    assert_eq!(parse_decimal_size("12XB"), None);
    assert_eq!(parse_decimal_size(""), None);
    assert_eq!(parse_decimal_size("-1MB"), None);
}

#[test]
fn images_and_build_cache_from_fixture() {
    let runner = running_docker();
    assert_eq!(
        reclaimable(&runner, &docker()),
        Ok(Reclaimable {
            images: 21_170_000_000,
            build_cache: 8_108_000_000,
            containers: 61_440,
        })
    );
    let fx = with_docker();
    let output = DockerImages
        .scan(&fx.ctx_with(&runner, Vec::new()))
        .unwrap();
    let args: Vec<Vec<String>> = output
        .findings
        .iter()
        .map(|f| spec(f).args.clone())
        .collect();
    assert_eq!(
        args,
        [
            ["builder", "prune", "-a", "-f"].map(String::from).to_vec(),
            ["image", "prune", "-a", "-f"].map(String::from).to_vec(),
        ]
    );
    assert_eq!(output.findings[0].bytes, 8_108_000_000);
    assert_eq!(output.findings[1].bytes, 21_170_000_000);
    for finding in &output.findings {
        assert_eq!(finding.risk, Risk::Safe);
        assert_eq!(finding.reversibility, Reversibility::Regenerable);
        assert_eq!(spec(finding).timeout, ACTION_TIMEOUT);
        assert_eq!(spec(finding).program, PathBuf::from(DOCKER_EXE));
        assert!(finding.fingerprint.is_none());
    }
}

#[test]
fn one_item_per_dangling_volume_with_compose_project() {
    let runner = running_docker();
    let fx = with_docker();
    let output = DockerVolumes
        .scan(&fx.ctx_with(&runner, Vec::new()))
        .unwrap();
    let volumes: Vec<&Finding> = output
        .findings
        .iter()
        .filter(|f| spec(f).args[0] == "volume")
        .collect();
    let labels: Vec<&str> = volumes.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "3d7eadcce010e0ecbb3f83267ce22d3e25f6a0780748dfca710286dca7d765a5",
            "loja-dev_loja_volume (projeto loja-dev)",
            "supabase_db_estoque-app (projeto estoque-app)",
        ]
    );
    assert_eq!(
        output.footnote.as_deref(),
        Some("1 volume(s) pequeno(s) ou de tamanho desconhecido ficaram de fora.")
    );
    let loja = volumes[1];
    assert_eq!(spec(loja).args, ["volume", "rm", "loja-dev_loja_volume"]);
    assert_eq!(loja.bytes, 144_100_000);
    assert_eq!(volumes[0].bytes, 158_700_000);
    for finding in &output.findings {
        assert_eq!(finding.risk, Risk::Review);
        assert_eq!(finding.reversibility, Reversibility::Irreversible);
        assert!(!spec(finding).args.iter().any(|arg| arg == "-a"));
    }
    assert!(
        loja.note
            .as_deref()
            .unwrap()
            .contains("Pode ter dados de banco local")
    );
    let containers = output.findings.last().unwrap();
    assert_eq!(
        spec(containers).args,
        ["container", "rm", "deefb295e0d8", "bada0fb47932"]
    );
    assert!(
        output
            .findings
            .iter()
            .all(|f| !spec(f).args.iter().any(|arg| arg == "prune" || arg == "-f"))
    );
    assert_eq!(containers.bytes, 61_440);
    assert!(
        containers
            .note
            .as_deref()
            .unwrap()
            .contains("relatorios-db")
    );
    assert_eq!(output.findings.len(), 4);
    let ps_call = runner
        .calls()
        .into_iter()
        .find(|call| call[0] == "ps")
        .unwrap();
    assert!(
        ps_call.iter().any(|arg| arg.contains("{{.ID}}")),
        "{ps_call:?}"
    );
    for status in ["status=exited", "status=created", "status=dead"] {
        assert!(ps_call.iter().any(|arg| arg == status), "{ps_call:?}");
    }
}

#[test]
fn reclaimable_ignores_rows_it_does_not_use() {
    let df = format!(
        "{}{}\n{}\n",
        super::test_fixtures::SYSTEM_DF,
        r#"{"Type":"Local Volumes","Reclaimable":"???"}"#,
        r#"{"Type":"Algo Novo","Reclaimable":"muito"}"#
    );
    let runner = FakeRunner::new().on(&["system", "df"], ok(&df));
    let space = reclaimable(&runner, &docker()).unwrap();
    assert_eq!(space.images, 21_170_000_000);
}

#[test]
fn no_stopped_containers_no_container_item() {
    let runner = running_docker().on(&["ps", "-a"], ok(""));
    let fx = with_docker();
    let output = DockerVolumes
        .scan(&fx.ctx_with(&runner, Vec::new()))
        .unwrap();
    assert!(output.findings.iter().all(|f| spec(f).args[0] == "volume"));
}

#[test]
fn dangling_volumes_reads_names_labels_and_sizes() {
    let volumes = dangling_volumes(&running_docker(), &docker()).unwrap();
    assert_eq!(volumes.len(), 4);
    assert_eq!(
        volumes[3],
        VolumeInfo {
            name: "supabase_db_estoque-app".to_string(),
            compose_project: Some("estoque-app".to_string()),
            bytes: 77_980_000,
        }
    );
    assert_eq!(volumes[1].compose_project, None);
    assert_eq!(volumes[1].bytes, 0);
}

#[test]
fn volume_linked_to_a_container_is_never_offered() {
    let listing = r#"{"Labels":"","Name":"ligado"}
{"Labels":"","Name":"solto"}
"#;
    let verbose = r#"{"Volumes":[{"Name":"ligado","Size":"5MB","Links":"1"},{"Name":"solto","Size":"5MB","Links":"0"}]}"#;
    let runner = FakeRunner::new()
        .on(&["volume", "ls"], ok(listing))
        .on(&["system", "df", "-v"], ok(verbose));
    let volumes = dangling_volumes(&runner, &docker()).unwrap();
    let names: Vec<&str> = volumes.iter().map(|volume| volume.name.as_str()).collect();
    assert_eq!(names, ["solto"]);
}

#[test]
fn no_dangling_volumes_skips_verbose_df() {
    let runner = FakeRunner::new().on(&["volume", "ls"], ok(""));
    assert_eq!(dangling_volumes(&runner, &docker()), Ok(Vec::new()));
    assert_eq!(runner.calls().len(), 1);
}

#[test]
fn engine_stopped_names_context() {
    let runner = daemon_down();
    let fx = with_docker();
    let ctx = fx.ctx_with(&runner, Vec::new());
    let stopped = Err(ScanProblem::EngineStopped("desktop-linux".to_string()));
    assert_eq!(DockerImages.scan(&ctx), stopped);
    assert_eq!(DockerVolumes.scan(&ctx), stopped);
}

#[test]
fn other_docker_errors_are_failures() {
    let runner = FakeRunner::new().on(
        &["system", "df"],
        CommandOutput {
            success: false,
            code: Some(1),
            stdout: String::new(),
            stderr: "permission denied while trying to connect\n".to_string(),
        },
    );
    let result = reclaimable(&runner, &docker());
    assert!(
        matches!(result, Err(ScanProblem::Failed(message)) if message.contains("permission denied"))
    );
}

#[test]
fn hanging_docker_times_out_as_stopped() {
    let runner = FakeRunner::new().delay(Duration::from_secs(1));
    let fx = with_docker();
    write_bytes(&fx.home.join(".npm/_cacache/blob"), 2 * 1024 * 1024);
    let mut ctx = fx.ctx_with(&runner, Vec::new());
    ctx.read_timeout = Duration::from_millis(100);
    let Err(ScanProblem::EngineStopped(message)) = DockerImages.scan(&ctx) else {
        panic!("esperava EngineStopped");
    };
    assert!(
        message.contains("motor parado ou sem resposta"),
        "{message}"
    );
    let (ledger, scans) = scan_all(&ctx, None, &|_| {});
    let npm = scans
        .iter()
        .find(|scan| scan.id == CategoryId::NpmCache)
        .unwrap();
    assert_eq!(npm.result.as_ref().unwrap().findings.len(), 1);
    let volumes = scans
        .iter()
        .find(|scan| scan.id == CategoryId::DockerVolumes)
        .unwrap();
    assert!(matches!(volumes.result, Err(ScanProblem::EngineStopped(_))));
    assert_eq!(ledger.findings.len(), 1);
}

#[test]
fn scan_runs_only_read_commands() {
    for runner in [running_docker(), daemon_down()] {
        assert_only_reads(&runner);
    }
}

fn assert_only_reads(runner: &FakeRunner) {
    let fx = with_docker();
    write_bytes(
        &fx.home.join(".docker/desktop/vms/0/data/Docker.raw"),
        2 * 1024 * 1024,
    );
    let ctx = fx.ctx_with(runner, Vec::new());
    let categories: [&dyn Category; 3] = [&DockerImages, &DockerVolumes, &DockerDesktopDisk];
    for category in categories {
        let _ = category.scan(&ctx);
    }
    let calls = runner.calls();
    assert!(!calls.is_empty());
    for call in &calls {
        assert!(
            !call.iter().any(|arg| arg == "prune" || arg == "rm"),
            "{call:?}"
        );
        let read_only = matches!(
            call.first().map(String::as_str),
            Some("system" | "volume" | "ps" | "context")
        );
        assert!(read_only, "{call:?}");
        if call[0] == "volume" {
            assert_eq!(call[1], "ls");
        }
    }
}

#[test]
fn missing_docker() {
    let fx = fixture();
    let runner = running_docker();
    let ctx = fx.ctx_with(&runner, Vec::new());
    assert_eq!(
        DockerImages.scan(&ctx),
        Err(ScanProblem::ToolMissing("docker"))
    );
    assert_eq!(
        DockerVolumes.scan(&ctx),
        Err(ScanProblem::ToolMissing("docker"))
    );
    assert!(runner.calls().is_empty());
}

#[test]
fn actions_pin_the_measured_context_and_explain_the_vm() {
    let runner = running_docker();
    let fx = with_docker();
    let output = DockerImages
        .scan(&fx.ctx_with(&runner, Vec::new()))
        .unwrap();
    for finding in &output.findings {
        assert!(
            spec(finding)
                .env
                .contains(&("DOCKER_CONTEXT".to_string(), "desktop-linux".to_string())),
            "{finding:?}"
        );
        assert!(spec(finding).runs_in_docker_desktop());
        assert!(
            finding
                .note
                .as_deref()
                .is_some_and(|note| note.contains("VM do Docker Desktop")),
            "{finding:?}"
        );
    }
}
