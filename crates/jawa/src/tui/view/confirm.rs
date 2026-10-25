use jawa_core::model::finding::{Finding, Reversibility};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::super::model::{CONFIRM_WORD, Model};
use crate::format::{display_safe, human_bytes, reversibility_label};

use super::*;

pub(super) const REVERSIBILITY_ORDER: [Reversibility; 3] = [
    Reversibility::Regenerable,
    Reversibility::Trash,
    Reversibility::Irreversible,
];

pub(super) fn draw_confirm(frame: &mut Frame, area: Rect, model: &Model) {
    let notes = confirm_note_lines(model);
    let notes_height = wrapped_rows(&notes, area.width);
    let input_height = u16::from(model.has_irreversible_selection());
    let [items, total, notes_area, input] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(notes_height),
        Constraint::Length(input_height),
    ])
    .areas(area);
    let block = Block::bordered().title("Itens marcados (↑/↓ rola)");
    draw_scrolled(frame, items, model.scroll, confirm_item_lines(model), block);
    frame.render_widget(confirm_total_line(model), total);
    frame.render_widget(Paragraph::new(notes).wrap(Wrap { trim: false }), notes_area);
    let field = Line::from(format!("> {}_", model.confirm_input)).bold();
    frame.render_widget(field, input);
}

pub(super) fn confirm_item_lines(model: &Model) -> Vec<Line<'static>> {
    let selected = model.selected_findings();
    let mut lines = Vec::new();
    for reversibility in REVERSIBILITY_ORDER {
        let group: Vec<&&Finding> = selected
            .iter()
            .filter(|finding| finding.reversibility == reversibility)
            .collect();
        if group.is_empty() {
            continue;
        }
        lines.push(Line::from(reversibility_label(reversibility)).bold());
        lines.extend(group.iter().map(|finding| {
            let size = human_bytes(finding.bytes);
            let line = format!("  {size:>10}  {} ({})", finding.label, finding.location);
            Line::from(display_safe(&line).into_owned())
        }));
    }
    lines
}

pub(super) fn confirm_total_line(model: &Model) -> Line<'static> {
    let selected = model.selected_findings();
    let total: u64 = selected.iter().map(|finding| finding.bytes).sum();
    Line::from(format!(
        "Total estimado: {} em {} item(ns)",
        human_bytes(total),
        selected.len()
    ))
    .bold()
}

pub(super) fn confirm_note_lines(model: &Model) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = model
        .confirm_warnings()
        .into_iter()
        .map(|warning| Line::from(format!("Aviso: {warning}")).yellow())
        .collect();
    if model.has_irreversible_selection() {
        lines.push(Line::from(format!(
            "Há itens irreversíveis: digite {CONFIRM_WORD} e tecle Enter para confirmar."
        )));
    } else {
        lines.push(Line::from("Enter limpa os itens marcados · Esc volta"));
    }
    lines
}
