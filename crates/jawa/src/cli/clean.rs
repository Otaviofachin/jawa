use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use clap::Args;
use jawa_core::clean::{CleanReport, CleanRequest, ItemOutcome, Progress};
use jawa_core::model::finding::{Action, Finding, FindingId, Reversibility, Risk};
use jawa_core::safety::ledger::{InstanceLock, Ledger};
use jawa_core::scan::warnings::selection_warnings;

use super::json::{plan_line, progress_line, result_line};
use super::{CliError, CliResult, EXIT_PARTIAL};
use crate::format::{
    SNAPSHOT_HINT, display_safe, human_bytes, reversibility_label, risk_label, space_lines,
};
use jawa_core::api::Session;

const YES_ANSWERS: [&str; 2] = ["s", "sim"];

#[derive(Args)]
#[command(next_help_heading = super::OPTIONS_HEADING)]
pub struct CleanArgs {
    #[arg(value_name = "IDS", help_heading = super::ARGUMENTS_HEADING, help = "Ids dos itens da última medição")]
    ids: Vec<String>,
    #[arg(
        long = "seguros",
        help = "Soma todos os itens seguros e livres de bloqueio"
    )]
    safe: bool,
    #[arg(long = "simular", help = "Mostra o plano e sai sem apagar nada")]
    simulate: bool,
    #[arg(
        long = "sim",
        help = "Confirma sem perguntar (obrigatório fora de um terminal)"
    )]
    yes: bool,
    #[arg(
        long = "confirmar-irreversivel",
        help = "Permite apagar itens irreversíveis"
    )]
    allow_irreversible: bool,
    #[arg(
        long = "json",
        hide = true,
        help = "Progresso e resultado em JSON Lines (usado pela interface jawa-tui)"
    )]
    json: bool,
}

pub fn run(session: &Session, args: &CleanArgs) -> CliResult {
    let _lock = InstanceLock::acquire(&session.env)?;
    let requested = parse_ids(&args.ids)?;
    let mut ledger = Ledger::load_fresh(&session.env)?;
    let selected = select_findings(&ledger, requested, args.safe)?;
    if args.json {
        return run_json(session, &mut ledger, &selected, args);
    }
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
    let request = request_for(&selected, args.allow_irreversible);
    let report = execute(session, &mut ledger, &request)?;
    Ok(print_report(&report))
}

fn request_for(selected: &[Finding], allow_irreversible: bool) -> CleanRequest {
    CleanRequest {
        ids: selected.iter().map(|finding| finding.id.clone()).collect(),
        allow_irreversible,
    }
}

fn run_json(
    session: &Session,
    ledger: &mut Ledger,
    selected: &[Finding],
    args: &CleanArgs,
) -> CliResult {
    if args.simulate {
        println!("{}", plan_line(selected, &plan_warnings(selected)));
        return Ok(ExitCode::SUCCESS);
    }
    if !args.yes {
        return Err(CliError::Message(
            "--json exige --sim: nesse modo não há como perguntar".to_string(),
        ));
    }
    ensure_irreversible_allowed(selected, args.allow_irreversible)?;
    let request = request_for(selected, args.allow_irreversible);
    let cancel = AtomicBool::new(false);
    let report = session.clean(ledger, &request, &print_progress_json, &cancel)?;
    println!("{}", result_line(&report));
    Ok(exit_code_for(&report))
}

fn print_progress_json(progress: &Progress) {
    println!("{}", progress_line(progress));
    let _ = io::stdout().flush();
}

fn parse_ids(texts: &[String]) -> Result<Vec<FindingId>, CliError> {
    texts
        .iter()
        .map(|text| text.parse::<FindingId>().map_err(CliError::from))
        .collect()
}

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
    for warning in plan_warnings(selected) {
        println!("Aviso: {warning}");
    }
}

fn plan_warnings(selected: &[Finding]) -> Vec<&'static str> {
    let refs: Vec<&Finding> = selected
        .iter()
        .filter(|finding| finding.blocked.is_none())
        .collect();
    selection_warnings(&refs)
}

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
