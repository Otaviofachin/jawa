mod clean;
mod explain;
mod history;
mod json;
mod measure;

use std::io;
use std::path::Path;
use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
use jawa_core::error::CoreError;
use jawa_core::model::finding::{CategoryId, InvalidFindingId};
use jawa_core::safety::ledger::LedgerError;
use jawa_core::system::env::Environment;
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::tui::{self, TuiError};
use jawa_core::api::Session;

pub const EXIT_ERROR: u8 = 1;
pub const EXIT_PARTIAL: u8 = 2;

const HELP_TEMPLATE: &str = "{about-with-newline}\nUso: {usage}\n\n{all-args}{after-help}";
pub(crate) const OPTIONS_HEADING: &str = "Opções";
pub(crate) const ARGUMENTS_HEADING: &str = "Argumentos";

#[derive(Parser)]
#[command(
    name = "jawa",
    about = "Limpador de disco para desenvolvedores no Linux",
    version,
    help_template = HELP_TEMPLATE,
    override_usage = "jawa   (abre a interface no terminal)",
    subcommand_help_heading = "Comandos",
    subcommand_value_name = "COMANDO",
    next_help_heading = OPTIONS_HEADING,
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(short = 'h', long = "help", action = ArgAction::Help, global = true, help = "Mostra esta ajuda")]
    help: Option<bool>,
    #[arg(short = 'V', long = "version", action = ArgAction::Version, help = "Mostra a versão")]
    version: Option<bool>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        name = "medir",
        about = "Mede o que dá para limpar e grava a medição (nunca apaga nada)",
        hide = true,
        override_usage = "jawa medir [--categoria <ID>] [--json]",
        help_template = HELP_TEMPLATE,
        next_help_heading = OPTIONS_HEADING,
        disable_help_flag = true
    )]
    Measure {
        #[arg(
            long = "categoria",
            value_name = "ID",
            help = "Mede só esta categoria (veja os ids em `jawa medir`)"
        )]
        category: Option<CategoryId>,
        #[arg(long, help = "Mostra o resultado em JSON")]
        json: bool,
    },
    #[command(
        name = "explicar",
        about = "Explica uma categoria: o que é, por que cresce e o que acontece ao limpar",
        hide = true,
        override_usage = "jawa explicar <CATEGORIA>",
        help_template = HELP_TEMPLATE,
        next_help_heading = OPTIONS_HEADING,
        disable_help_flag = true
    )]
    Explain {
        #[arg(value_name = "CATEGORIA", help_heading = ARGUMENTS_HEADING, help = "Id da categoria, por exemplo `npm-cache`")]
        category: CategoryId,
    },
    #[command(
        name = "limpar",
        about = "Limpa itens da última medição",
        hide = true,
        override_usage = "jawa limpar [IDS]... [--seguros] [--simular] [--sim] \
                          [--confirmar-irreversivel]",
        help_template = HELP_TEMPLATE,
        next_help_heading = OPTIONS_HEADING,
        disable_help_flag = true
    )]
    Clean(clean::CleanArgs),
    #[command(
        name = "historico",
        about = "Mostra as últimas ações registradas",
        hide = true,
        override_usage = "jawa historico [--ultimos <N>]",
        help_template = HELP_TEMPLATE,
        next_help_heading = OPTIONS_HEADING,
        disable_help_flag = true
    )]
    History {
        #[arg(
            long = "ultimos",
            value_name = "N",
            default_value_t = 20,
            help = "Quantas entradas mostrar"
        )]
        last: usize,
    },
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    InvalidId(#[from] InvalidFindingId),
    #[error("erro de entrada e saída: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tui(#[from] TuiError),
    #[error("{0}")]
    Message(String),
}

pub type CliResult = Result<ExitCode, CliError>;

const TEXTUAL_INTERFACE: &str = "jawa-tui";

fn open_interface(session: &Session) -> CliResult {
    match which::which(TEXTUAL_INTERFACE) {
        Ok(program) => launch_textual(&program),
        Err(_) => tui::run(session.clone())
            .map(|()| ExitCode::SUCCESS)
            .map_err(CliError::from),
    }
}

fn launch_textual(program: &Path) -> CliResult {
    let status = std::process::Command::new(program)
        .env("JAWA_BIN", std::env::current_exe()?)
        .status()?;
    let code = status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(EXIT_ERROR);
    Ok(ExitCode::from(code))
}

pub fn run(cli: Cli) -> ExitCode {
    let session = match Session::open().map_err(CliError::from) {
        Ok(session) => session,
        Err(error) => return report_failure(&error),
    };
    let _log_guard = init_logging(&session.env);
    let outcome = match cli.command {
        None => open_interface(&session),
        Some(Command::Measure { category, json }) => measure::run(&session, category, json),
        Some(Command::Explain { category }) => explain::run(category),
        Some(Command::Clean(args)) => clean::run(&session, &args),
        Some(Command::History { last }) => history::run(&session, last),
    };
    outcome.unwrap_or_else(|error| report_failure(&error))
}

fn report_failure(error: &CliError) -> ExitCode {
    eprintln!("erro: {error}");
    ExitCode::from(EXIT_ERROR)
}

fn init_logging(env: &Environment) -> Option<WorkerGuard> {
    let directory = env.state_dir.join("jawa");
    std::fs::create_dir_all(&directory).ok()?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::NEVER)
        .filename_prefix("jawa.log")
        .build(&directory)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .ok()?;
    Some(guard)
}
