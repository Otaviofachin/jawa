use jawa_core::clean::{CleanReport, ItemOutcome};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;

use super::super::model::Model;
use crate::format::{SNAPSHOT_HINT, display_safe, space_lines};

use super::*;

pub(super) fn draw_results(frame: &mut Frame, area: Rect, model: &Model) {
    let lines = model.report.as_ref().map_or_else(Vec::new, result_lines);
    draw_text(frame, area, model, lines);
}

pub(super) fn result_lines(report: &CleanReport) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = space_lines(report).into_iter().map(Line::from).collect();
    if report.space_may_be_stuck() {
        lines.push(Line::from(SNAPSHOT_HINT).yellow());
    }
    lines.extend(outcome_lines(report));
    lines.extend(
        report
            .warnings
            .iter()
            .map(|warning| Line::from(format!("Aviso: {warning}")).yellow()),
    );
    lines.push(Line::from("Tecle m para medir de novo."));
    lines
}

pub(super) fn outcome_lines(report: &CleanReport) -> Vec<Line<'static>> {
    let is_done = |outcome: &ItemOutcome| matches!(outcome, ItemOutcome::Done { .. });
    let done = report
        .items
        .iter()
        .filter(|(_, outcome)| is_done(outcome))
        .count();
    let mut lines = vec![Line::from(format!(
        "Feitos: {done} de {}",
        report.items.len()
    ))];
    lines.extend(
        report
            .items
            .iter()
            .filter(|(_, outcome)| !is_done(outcome))
            .map(|(finding, outcome)| {
                let line = format!("  {}: {outcome}", finding.label);
                Line::from(display_safe(&line).into_owned())
            }),
    );
    lines
}
