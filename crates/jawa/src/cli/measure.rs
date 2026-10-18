use std::process::ExitCode;

use jawa_core::model::finding::{CategoryId, Finding};
use jawa_core::safety::ledger::InstanceLock;
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::registry;
use serde_json::json;

use super::{CliError, CliResult};
use crate::format::{display_safe, human_bytes, render_table, reversibility_label, risk_label};
use jawa_core::api::Session;

const TABLE_HEADERS: [&str; 6] = [
    "Categoria",
    "Id",
    "Tamanho",
    "Risco",
    "Reversibilidade",
    "Item",
];
const SIZE_COLUMN: usize = 2;

pub fn run(session: &Session, only: Option<CategoryId>, as_json: bool) -> CliResult {
    ensure_available(only)?;
    let _lock = InstanceLock::acquire(&session.env)?;
    let (ledger, scans) = session.measure(only, &|_| {});
    ledger.save(&session.env)?;
    if as_json {
        print_json(&ledger.findings, &scans)?;
    } else {
        print_report(&scans);
    }
    Ok(ExitCode::SUCCESS)
}

fn ensure_available(only: Option<CategoryId>) -> Result<(), CliError> {
    match only {
        Some(id)
            if !registry()
                .iter()
                .any(|category| category.descriptor().id == id) =>
        {
            Err(CliError::Message(format!(
                "a categoria {id} ainda não está disponível nesta versão"
            )))
        }
        _ => Ok(()),
    }
}

fn print_json(findings: &[Finding], scans: &[CategoryScan]) -> Result<(), CliError> {
    let problems: Vec<_> = scans
        .iter()
        .filter_map(|scan| {
            let problem = scan.result.as_ref().err()?;
            Some(json!({ "categoria": scan.id, "mensagem": problem.to_string() }))
        })
        .collect();
    let report = json!({ "itens": findings, "problemas": problems });
    let text = serde_json::to_string_pretty(&report)
        .map_err(|error| CliError::Message(error.to_string()))?;
    println!("{text}");
    Ok(())
}

fn print_report(scans: &[CategoryScan]) {
    let mut rows = Vec::new();
    let mut details = Vec::new();
    let mut total = 0u64;
    for scan in scans {
        let Ok(output) = &scan.result else { continue };
        let mut findings: Vec<&Finding> = output.findings.iter().collect();
        findings.sort_by_key(|finding| std::cmp::Reverse(finding.bytes));
        for finding in findings {
            total += finding.bytes;
            rows.push(table_row(finding));
            details.push(detail_lines(finding));
        }
    }
    if rows.is_empty() {
        println!("Nada encontrado para limpar.");
    } else {
        print_rows(&rows, &details);
        println!("\nTotal encontrado: {}", human_bytes(total));
    }
    print_notices(scans);
}

fn table_row(finding: &Finding) -> Vec<String> {
    vec![
        finding.category.to_string(),
        finding.id.to_string(),
        human_bytes(finding.bytes),
        risk_label(finding.risk).to_string(),
        reversibility_label(finding.reversibility).to_string(),
        display_safe(&format!("{} ({})", finding.label, finding.location)).into_owned(),
    ]
}

fn detail_lines(finding: &Finding) -> Vec<String> {
    let note = finding.note.iter().map(|note| format!("nota: {note}"));
    let blocked = finding
        .blocked
        .iter()
        .map(|reason| format!("BLOQUEADO: {reason}"));
    note.chain(blocked)
        .map(|line| display_safe(&line).into_owned())
        .collect()
}

fn print_rows(rows: &[Vec<String>], details: &[Vec<String>]) {
    let table = render_table(&TABLE_HEADERS, rows, &[SIZE_COLUMN]);
    for (index, line) in table.lines().enumerate() {
        println!("{line}");
        if let Some(lines) = index.checked_sub(1).and_then(|row| details.get(row)) {
            lines.iter().for_each(|detail| println!("    {detail}"));
        }
    }
}

fn print_notices(scans: &[CategoryScan]) {
    for scan in scans {
        match &scan.result {
            Ok(output) => {
                if let Some(footnote) = &output.footnote {
                    println!("{}: {}", scan.id, display_safe(footnote));
                }
            }
            Err(problem) => println!("{}: {}", scan.id, display_safe(&problem.to_string())),
        }
    }
}
