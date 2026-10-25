use jawa_core::clean::{CleanReport, ItemOutcome, Progress};
use jawa_core::model::finding::{Action, Finding, Reversibility, Risk};
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::{Explanation, category};
use serde_json::{Value, json};

pub const CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy)]
pub struct DiskSpace {
    pub free: u64,
    pub total: u64,
}

pub fn measure_view(scans: &[CategoryScan], disk: DiskSpace) -> Value {
    json!({
        "version": CONTRACT_VERSION,
        "disk": { "free": disk.free, "total": disk.total },
        "categories": scans.iter().map(category_view).collect::<Vec<_>>(),
    })
}

fn category_view(scan: &CategoryScan) -> Value {
    let descriptor = category(scan.id).descriptor();
    let (items, footnote, problem) = match &scan.result {
        Ok(output) => (
            output.findings.iter().map(item_view).collect(),
            output.footnote.clone(),
            None,
        ),
        Err(problem) => (Vec::new(), None, Some(problem.to_string())),
    };
    json!({
        "id": scan.id,
        "title": descriptor.title,
        "summary": descriptor.summary,
        "explanation": explanation_view(&descriptor.explanation),
        "footnote": footnote,
        "problem": problem,
        "items": items,
    })
}

fn explanation_view(explanation: &Explanation) -> Value {
    json!({
        "what_it_is": explanation.what_it_is,
        "why_it_grows": explanation.why_it_grows,
        "effect": explanation.effect,
        "reversibility": explanation.reversibility_detail,
        "recreate_cost": explanation.recreate_cost,
        "recommendation": explanation.recommendation,
    })
}

fn item_view(finding: &Finding) -> Value {
    let command = match &finding.action {
        Action::Guide(guide) => Some(guide.command_line.clone()),
        _ => None,
    };
    json!({
        "id": finding.id,
        "label": finding.label,
        "location": finding.location,
        "bytes": finding.bytes,
        "risk": risk_name(finding.risk),
        "reversibility": reversibility_name(finding.reversibility),
        "note": finding.note,
        "block": finding.blocked.as_ref().map(ToString::to_string),
        "command": command,
    })
}

fn risk_name(risk: Risk) -> &'static str {
    match risk {
        Risk::Safe => "safe",
        Risk::Review => "review",
        Risk::Guided => "guided",
    }
}

fn reversibility_name(reversibility: Reversibility) -> &'static str {
    match reversibility {
        Reversibility::Regenerable => "regenerable",
        Reversibility::Trash => "trash",
        Reversibility::Irreversible => "irreversible",
    }
}

pub fn plan_line(selected: &[Finding], warnings: &[&str]) -> String {
    json!({
        "type": "plan",
        "count": selected.len(),
        "estimated": selected.iter().map(|finding| finding.bytes).sum::<u64>(),
        "warnings": warnings,
    })
    .to_string()
}

pub fn progress_line(progress: &Progress) -> String {
    json!({
        "type": "progress",
        "done": progress.done,
        "total": progress.total,
        "current": progress.current,
    })
    .to_string()
}

pub fn result_line(report: &CleanReport) -> String {
    json!({
        "type": "result",
        "estimated": report.estimated_bytes,
        "freed": report.measured_bytes(),
        "trash": report.trashed_bytes(),
        "docker_vm": report.docker_desktop_bytes(),
        "maybe_stuck": report.space_may_be_stuck(),
        "items": report.items.iter().map(outcome_view).collect::<Vec<_>>(),
        "warnings": report.warnings,
    })
    .to_string()
}

fn outcome_view((finding, outcome): &(Finding, ItemOutcome)) -> Value {
    let (name, detail) = match outcome {
        ItemOutcome::Done { output } => ("done", output.clone()),
        ItemOutcome::Skipped(reason) => ("skipped", Some(reason.to_string())),
        ItemOutcome::Failed(error) => ("failed", Some(error.clone())),
    };
    json!({
        "id": finding.id,
        "label": finding.label,
        "outcome": name,
        "detail": detail,
    })
}
