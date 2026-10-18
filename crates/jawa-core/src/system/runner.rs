use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::model::finding::CommandSpec;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const DRAIN_POLL: Duration = Duration::from_millis(5);
const DRAIN_GRACE: Duration = Duration::from_secs(1);
const READ_CHUNK: usize = 8192;
pub const MAX_STREAM_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    /// Codigo de saida; `None` quando o processo foi encerrado por sinal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunError {
    #[error("programa não encontrado")]
    NotFound,
    #[error("o comando passou do tempo limite e foi encerrado")]
    TimedOut,
    #[error("falha ao rodar o comando: {0}")]
    Io(String),
}

/// Toda execucao de comando externo passa por aqui, para os testes trocarem
/// o sistema por respostas prontas.
pub trait CommandRunner: Sync {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    /// Le stdout e stderr em threads separadas (sem travar com saida grande)
    /// e mata o processo quando passa de `spec.timeout`.
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        let mut child = spawn(spec)?;
        feed_stdin(&mut child, spec.stdin.clone());
        let stdout_reader = drain(child.stdout.take());
        let stderr_reader = drain(child.stderr.take());
        let status = wait_or_kill(&mut child, spec.timeout)?;
        let grace_deadline = Instant::now() + DRAIN_GRACE;
        Ok(CommandOutput {
            success: status.success(),
            code: status.code(),
            stdout: collect(stdout_reader, grace_deadline),
            stderr: collect(stderr_reader, grace_deadline),
        })
    }
}

fn spawn(spec: &CommandSpec) -> Result<Child, RunError> {
    let stdin = if spec.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    Command::new(&spec.program)
        .args(&spec.args)
        .env("LC_ALL", "C")
        .envs(spec.env.iter().map(|(name, value)| (name, value)))
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => RunError::NotFound,
            _ => RunError::Io(error.to_string()),
        })
}

/// Escreve a entrada numa thread solta: se o programa nao ler, o pipe cheio
/// nao trava a espera. Fechar o pipe ao fim manda EOF.
fn feed_stdin(child: &mut Child, input: Option<Vec<u8>>) {
    let (Some(bytes), Some(mut pipe)) = (input, child.stdin.take()) else {
        return;
    };
    thread::spawn(move || {
        if let Err(error) = pipe.write_all(&bytes) {
            tracing::debug!(%error, "o comando não leu toda a entrada");
        }
    });
}

/// Saida lida aos pedacos por uma thread, visivel mesmo se ela nao terminar.
struct Drained {
    bytes: Arc<Mutex<Vec<u8>>>,
    handle: JoinHandle<()>,
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Option<Drained> {
    let mut pipe = pipe?;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bytes);
    let handle = thread::spawn(move || {
        let mut chunk = [0u8; READ_CHUNK];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => append(&sink, &chunk[..count]),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    tracing::debug!(%error, "falha ao ler a saída do comando");
                    break;
                }
            }
        }
    });
    Some(Drained { bytes, handle })
}

/// Guarda ate `MAX_STREAM_BYTES`; o excedente e lido e descartado, para o
/// filho nunca travar com o pipe cheio.
fn append(sink: &Mutex<Vec<u8>>, chunk: &[u8]) {
    if let Ok(mut bytes) = sink.lock() {
        let room = MAX_STREAM_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
    }
}

/// Espera a leitura acabar ate `deadline`: um neto que herdou o pipe e
/// continua vivo nao segura o retorno alem do prazo.
fn collect(drained: Option<Drained>, deadline: Instant) -> String {
    let Some(drained) = drained else {
        return String::new();
    };
    while !drained.handle.is_finished() && Instant::now() < deadline {
        thread::sleep(DRAIN_POLL);
    }
    let bytes = drained
        .bytes
        .lock()
        .map(|bytes| bytes.clone())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// No estouro, mata e espera o filho; as threads de leitura ficam soltas e
/// terminam quando o ultimo dono do pipe fechar.
fn wait_or_kill(child: &mut Child, timeout: Duration) -> Result<ExitStatus, RunError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => return Err(RunError::Io(error.to_string())),
        }
    }
}

/// Runner de teste: responde pelo prefixo mais longo dos `args` e guarda as
/// chamadas recebidas.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct FakeRunner {
    responses: Vec<(Vec<String>, CommandOutput)>,
    delay: Option<Duration>,
    calls: std::sync::Mutex<Vec<Vec<String>>>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeRunner {
    pub fn new() -> FakeRunner {
        FakeRunner::default()
    }

    pub fn on(mut self, args_prefix: &[&str], output: CommandOutput) -> FakeRunner {
        let prefix = args_prefix.iter().map(|arg| arg.to_string()).collect();
        self.responses.push((prefix, output));
        self
    }

    /// Simula um comando travado: dorme `delay` (no maximo o timeout pedido).
    pub fn delay(mut self, delay: Duration) -> FakeRunner {
        self.delay = Some(delay);
        self
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }

    fn response_for(&self, args: &[String]) -> Option<&CommandOutput> {
        self.responses
            .iter()
            .filter(|(prefix, _)| args.starts_with(prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, output)| output)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl CommandRunner for FakeRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(spec.args.clone());
        }
        if let Some(delay) = self.delay {
            thread::sleep(delay.min(spec.timeout));
            if delay >= spec.timeout {
                return Err(RunError::TimedOut);
            }
        }
        self.response_for(&spec.args)
            .cloned()
            .ok_or_else(|| RunError::Io(format!("FakeRunner sem resposta para {:?}", spec.args)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(program: &str, args: &[&str], timeout: Duration) -> CommandSpec {
        CommandSpec {
            program: PathBuf::from(program),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            stdin: None,
            env: Vec::new(),
            timeout,
        }
    }

    #[test]
    fn runs_quick_command() {
        let output = SystemRunner
            .run(&spec("/bin/echo", &["ola"], Duration::from_secs(5)))
            .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout, "ola\n");
        assert_eq!(output.stderr, "");
    }

    #[test]
    fn reports_failure_and_stderr() {
        let output = SystemRunner
            .run(&spec(
                "/bin/sh",
                &["-c", "echo ruim >&2; exit 3"],
                Duration::from_secs(5),
            ))
            .unwrap();
        assert!(!output.success);
        assert_eq!(output.stderr, "ruim\n");
    }

    #[test]
    fn kills_on_timeout() {
        let started = Instant::now();
        let result = SystemRunner.run(&spec("/bin/sleep", &["10"], Duration::from_millis(200)));
        assert_eq!(result, Err(RunError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn drains_large_output() {
        let output = SystemRunner
            .run(&spec(
                "/bin/sh",
                &["-c", "i=0; while [ $i -lt 20000 ]; do echo linha $i; echo erro $i >&2; i=$((i+1)); done"],
                Duration::from_secs(30),
            ))
            .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout.lines().count(), 20_000);
        assert_eq!(output.stderr.lines().count(), 20_000);
    }

    #[test]
    fn returns_when_background_child_keeps_pipe_open() {
        let started = Instant::now();
        let output = SystemRunner
            .run(&spec(
                "/bin/sh",
                &["-c", "sleep 5 & echo pronto"],
                Duration::from_millis(500),
            ))
            .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout, "pronto\n");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn caps_runaway_output() {
        let started = Instant::now();
        let output = SystemRunner
            .run(&spec(
                "/bin/sh",
                &["-c", "head -c 10485760 /dev/zero | tr '\\0' a"],
                Duration::from_secs(30),
            ))
            .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout.len(), MAX_STREAM_BYTES);
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn not_found_program() {
        let result = SystemRunner.run(&spec(
            "/caminho/que/nao/existe",
            &[],
            Duration::from_secs(1),
        ));
        assert_eq!(result, Err(RunError::NotFound));
    }

    #[test]
    fn writes_stdin_and_sets_c_locale() {
        let mut with_input = spec(
            "/bin/sh",
            &["-c", "cat; printf '%s' \"$LC_ALL\""],
            Duration::from_secs(5),
        );
        with_input.stdin = Some(b"entrada|".to_vec());
        let output = SystemRunner.run(&with_input).unwrap();
        assert_eq!(output.stdout, "entrada|C");
    }

    #[test]
    fn applies_extra_env_and_reports_exit_code() {
        let mut with_env = spec(
            "/bin/sh",
            &["-c", "printf '%s' \"$JAWA_TESTE\"; exit 3"],
            Duration::from_secs(5),
        );
        with_env.env = vec![("JAWA_TESTE".to_string(), "ola".to_string())];
        let output = SystemRunner.run(&with_env).unwrap();
        assert_eq!(output.stdout, "ola");
        assert_eq!(output.code, Some(3));
    }

    #[test]
    fn stdin_is_null_without_input() {
        let output = SystemRunner
            .run(&spec("/bin/cat", &[], Duration::from_secs(5)))
            .unwrap();
        assert!(output.success);
        assert_eq!(output.stdout, "");
    }

    #[test]
    fn fake_runner_matches_by_prefix_and_records_calls() {
        let fake = FakeRunner::new()
            .on(
                &["system"],
                CommandOutput {
                    success: true,
                    code: Some(0),
                    stdout: "geral".to_string(),
                    stderr: String::new(),
                },
            )
            .on(
                &["system", "df"],
                CommandOutput {
                    success: true,
                    code: Some(0),
                    stdout: "df".to_string(),
                    stderr: String::new(),
                },
            );
        let df = fake
            .run(&spec(
                "docker",
                &["system", "df", "--format", "json"],
                Duration::from_secs(1),
            ))
            .unwrap();
        assert_eq!(df.stdout, "df");
        let other = fake
            .run(&spec("docker", &["system", "info"], Duration::from_secs(1)))
            .unwrap();
        assert_eq!(other.stdout, "geral");
        assert!(
            fake.run(&spec("docker", &["ps"], Duration::from_secs(1)))
                .is_err()
        );
        assert_eq!(fake.calls().len(), 3);
        assert_eq!(fake.calls()[0], vec!["system", "df", "--format", "json"]);
    }

    #[test]
    fn fake_runner_delay_times_out() {
        let fake = FakeRunner::new().delay(Duration::from_secs(10));
        let started = Instant::now();
        let result = fake.run(&spec("docker", &["ps"], Duration::from_millis(100)));
        assert_eq!(result, Err(RunError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
