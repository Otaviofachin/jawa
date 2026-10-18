use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Args;
use jawa_core::clean::{CleanReport, CleanRequest, ItemOutcome, Progress};
use jawa_core::model::finding::{Action, Finding, FindingId, Reversibility, Risk};
use jawa_core::safety::ledger::{InstanceLock, Ledger};
use jawa_core::scan::warnings::selection_warnings;

use super::{CliError, CliResult, EXIT_PARTIAL};
use crate::format::{
    SNAPSHOT_HINT, display_safe, human_bytes, reversibility_label, risk_label, space_lines,
};
use jawa_core::api::Session;

const YES_ANSWERS: [&str; 2] = ["s", "sim"];

#[derive(Args)]
#[command(next_help_heading = super::OPTIONS_HEADING)]
pub struct CleanArgs {
    /// Ids dos itens da última medição.
    #[arg(value_name = "IDS", help_heading = super::ARGUMENTS_HEADING)]
    ids: Vec<String>,
    /// Soma todos os itens seguros e livres de bloqueio.
    #[arg(long = "seguros")]
    safe: bool,
    /// Mostra o plano e sai sem apagar nada.
    #[arg(long = "simular")]
    simulate: bool,
    /// Confirma sem perguntar (obrigatório fora de um terminal).
    #[arg(long = "sim")]
    yes: bool,
    /// Permite apagar itens irreversíveis.
    #[arg(long = "confirmar-irreversivel")]
    allow_irreversible: bool,
}

pub fn run(session: &Session, args: &CleanArgs) -> CliResult {
    let _lock = InstanceLock::acquire(&session.env)?;
    let requested = parse_ids(&args.ids)?;
    let mut ledger = Ledger::load_fresh(&session.env)?;
    let selected = select_findings(&ledger, requested, args.safe)?;
    if selected.is_empty() {
        println!("Nada a limpar: nenhum item seguro disponível na última medição.");
        return Ok(ExitCode::SUCCESS);
    }
    print_plan(&selected);
    if args.simulate {
        println!("\nSimulação: nada foi alterado.");
        return Ok(ExitCode::SUCCESS);
    }
    ensure_irreversible_allowed(&selected, args.allow_irreversible)?;
    if !confirmed(args.yes)? {
        println!("Cancelado: nada foi alterado.");
        return Ok(ExitCode::SUCCESS);
    }
    let request = CleanRequest {
        ids: selected.iter().map(|finding| finding.id.clone()).collect(),
        allow_irreversible: args.allow_irreversible,
    };
    let report = execute(session, &mut ledger, &request)?;
    Ok(print_report(&report))
}

fn parse_ids(texts: &[String]) -> Result<Vec<FindingId>, CliError> {
    texts
        .iter()
        .map(|text| text.parse::<FindingId>().map_err(CliError::from))
        .collect()
}

/// Ids pedidos mais (com `--seguros`) todo item seguro e sem bloqueio. Um id
/// fora da medição aborta antes de qualquer ação (S2).
fn select_findings(
    ledger: &Ledger,
    requested: Vec<FindingId>,
    include_safe: bool,
) -> Result<Vec<Finding>, CliError> {
    if requested.is_empty() && !include_safe {
        return Err(CliError::Message(
            "informe os ids dos itens ou use --seguros; veja os ids com `jawa medir`".to_string(),
        ));
    }
    let mut selected = Vec::new();
    for id in requested {
        let finding = ledger
            .find(&id)
            .ok_or_else(|| CliError::Core(jawa_core::CoreError::UnknownId(id.clone())))?;
        selected.push(finding.clone());
    }
    if include_safe {
        for finding in ledger.findings.iter().filter(|item| is_safe_pick(item)) {
            if !selected.iter().any(|chosen| chosen.id == finding.id) {
                selected.push(finding.clone());
            }
        }
    }
    Ok(selected)
}

fn is_safe_pick(finding: &Finding) -> bool {
    finding.risk == Risk::Safe
        && finding.blocked.is_none()
        && !matches!(finding.action, Action::Guide(_))
}

fn print_plan(selected: &[Finding]) {
    println!("Plano de limpeza:");
    for finding in selected {
        let line = format!(
            "  {}  {:>9}  {} ({}) [{}, {}]",
            finding.id,
            human_bytes(finding.bytes),
            finding.label,
            finding.location,
            risk_label(finding.risk),
            reversibility_label(finding.reversibility)
        );
        println!("{}", display_safe(&line));
        if let Some(note) = &finding.note {
            println!("      nota: {}", display_safe(note));
        }
        if let Some(reason) = &finding.blocked {
            println!("      BLOQUEADO: {}", display_safe(&reason.to_string()));
        }
    }
    let total: u64 = selected.iter().map(|finding| finding.bytes).sum();
    println!("Total estimado: {}", human_bytes(total));
    let refs: Vec<&Finding> = selected
        .iter()
        .filter(|finding| finding.blocked.is_none())
        .collect();
    for warning in selection_warnings(&refs) {
        println!("Aviso: {warning}");
    }
}

/// Item irreversível sem `--confirmar-irreversivel` recusa tudo antes de
/// perguntar (S11): a confirmação não pode levar a um plano que vai pular.
/// Itens guiados nunca rodam e não contam.
fn ensure_irreversible_allowed(selected: &[Finding], allowed: bool) -> Result<(), CliError> {
    let irreversible = selected.iter().any(|finding| {
        finding.reversibility == Reversibility::Irreversible
            && !matches!(finding.action, Action::Guide(_))
    });
    if irreversible && !allowed {
        return Err(CliError::Message(
            "a seleção tem item irreversível: para apagá-lo, rode de novo com \
             --confirmar-irreversivel (nada foi alterado)"
                .to_string(),
        ));
    }
    Ok(())
}

/// Com `--sim` não pergunta; sem terminal e sem `--sim`, recusa.
fn confirmed(yes: bool) -> Result<bool, CliError> {
    if yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() {
        return Err(CliError::Message(
            "sem terminal interativo para confirmar: use --sim para continuar".to_string(),
        ));
    }
    print!("Continuar? [s/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer)?;
    Ok(YES_ANSWERS.contains(&answer.trim().to_lowercase().as_str()))
}

fn execute(
    session: &Session,
    ledger: &mut Ledger,
    request: &CleanRequest,
) -> Result<CleanReport, CliError> {
    let report = session.clean(ledger, request, &print_progress, &AtomicBool::new(false))?;
    Ok(report)
}

fn print_progress(progress: &Progress) {
    if let Some(current) = &progress.current {
        eprintln!(
            "[{}/{}] {}",
            progress.done + 1,
            progress.total,
            display_safe(current)
        );
    }
}

fn print_report(report: &CleanReport) -> ExitCode {
    println!("\nResultado:");
    for (finding, outcome) in &report.items {
        let line = format!("  {}  {}: {outcome}", finding.id, finding.label);
        println!("{}", display_safe(&line));
    }
    space_lines(report)
        .iter()
        .for_each(|line| println!("{line}"));
    if report.space_may_be_stuck() {
        println!("{SNAPSHOT_HINT}");
    }
    report
        .warnings
        .iter()
        .for_each(|warning| eprintln!("aviso: {warning}"));
    exit_code_for(report)
}

fn exit_code_for(report: &CleanReport) -> ExitCode {
    let all_done = report
        .items
        .iter()
        .all(|(_, outcome)| matches!(outcome, ItemOutcome::Done { .. }));
    if all_done && report.warnings.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(EXIT_PARTIAL)
    }
}
