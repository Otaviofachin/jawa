use jawa_core::model::finding::{CategoryId, Finding, Risk};
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::{category, registry};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Gauge, ListItem, ListState};

use super::super::model::Model;
use crate::format::human_bytes;

use super::*;

pub(super) fn draw_overview(frame: &mut Frame, area: Rect, model: &Model) {
    let [disk, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(disk_gauge(model.disk), disk);
    let rows: Vec<ListItem> = registry()
        .iter()
        .map(|category| overview_row(model, category.descriptor().id))
        .collect();
    let mut state = ListState::default().with_selected(Some(model.cursor));
    frame.render_stateful_widget(highlighted_list(rows, "Categorias"), list, &mut state);
}

pub(super) fn disk_gauge(disk: Option<(u64, u64)>) -> Gauge<'static> {
    let Some((free, total)) = disk.filter(|(_, total)| *total > 0) else {
        return Gauge::default().label("Disco: lendo…").ratio(0.0);
    };
    let used = total.saturating_sub(free) as f64 / total as f64;
    let label = format!(
        "Disco: {} livre de {}",
        human_bytes(free),
        human_bytes(total)
    );
    Gauge::default()
        .label(label)
        .ratio(used.clamp(0.0, 1.0))
        .gauge_style(Style::new().fg(Color::Cyan).bg(Color::DarkGray))
}

pub(super) fn overview_row(model: &Model, id: CategoryId) -> ListItem<'static> {
    let title = category(id).descriptor().title;
    let (detail, style) = match model.scan_of(id) {
        None if model.scanning => ("medindo…".to_string(), Style::new()),
        None => ("não medida".to_string(), Style::new().dark_gray()),
        Some(scan) => scanned_detail(model, scan),
    };
    ListItem::new(format!("{title:<TITLE_WIDTH$} {detail}")).style(style)
}

pub(super) fn scanned_detail(model: &Model, scan: &CategoryScan) -> (String, Style) {
    if let Err(problem) = &scan.result {
        return (problem.to_string(), Style::new().dark_gray());
    }
    let findings = model.findings_of(scan.id);
    let Some(risk) = category_risk(&findings) else {
        return ("nada a limpar".to_string(), Style::new().dark_gray());
    };
    let total: u64 = findings.iter().map(|finding| finding.bytes).sum();
    let marked: u64 = findings
        .iter()
        .filter(|finding| model.selected.contains(&finding.id))
        .map(|finding| finding.bytes)
        .sum();
    let detail = format!(
        "{:>10}  {} item(ns), {} marcado(s)",
        human_bytes(total),
        findings.len(),
        human_bytes(marked)
    );
    (detail, Style::new().fg(risk_color(risk)))
}

pub(super) fn category_risk(findings: &[&Finding]) -> Option<Risk> {
    let has = |risk: Risk| findings.iter().any(|finding| finding.risk == risk);
    [Risk::Review, Risk::Safe, Risk::Guided]
        .into_iter()
        .find(|risk| has(*risk))
}
