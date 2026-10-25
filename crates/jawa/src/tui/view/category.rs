use jawa_core::model::finding::{CategoryId, Finding};
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::category;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Text};
use ratatui::widgets::{ListItem, ListState, Paragraph, Wrap};

use super::super::model::{Model, is_selectable, unselectable_reason};
use crate::format::{display_safe, human_bytes};

use super::*;

pub(super) fn draw_category(frame: &mut Frame, area: Rect, model: &Model, id: CategoryId) {
    let notice = category_notice(model.scan_of(id));
    let notice_height = if notice.is_some() { 3 } else { 0 };
    let [summary, list, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(notice_height),
    ])
    .areas(area);
    frame.render_widget(Line::from(category(id).descriptor().summary), summary);
    let rows: Vec<ListItem> = model
        .findings_of(id)
        .into_iter()
        .map(|finding| finding_row(model, finding))
        .collect();
    let mut state = ListState::default().with_selected(Some(model.cursor));
    frame.render_stateful_widget(highlighted_list(rows, "Itens"), list, &mut state);
    if let Some(text) = notice {
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).dark_gray();
        frame.render_widget(paragraph, footer);
    }
}

pub(super) fn category_notice(scan: Option<&CategoryScan>) -> Option<String> {
    match &scan?.result {
        Ok(output) => output
            .footnote
            .as_deref()
            .map(|footnote| display_safe(footnote).into_owned()),
        Err(problem) => Some(display_safe(&problem.to_string()).into_owned()),
    }
}

pub(super) fn finding_row(model: &Model, finding: &Finding) -> ListItem<'static> {
    let checkbox = if !is_selectable(finding) {
        "[-]"
    } else if model.selected.contains(&finding.id) {
        "[x]"
    } else {
        "[ ]"
    };
    let first = format!(
        "{checkbox} {:>10}  {} ({})",
        human_bytes(finding.bytes),
        finding.label,
        finding.location
    );
    let mut lines =
        vec![Line::from(display_safe(&first).into_owned()).fg(risk_color(finding.risk))];
    if let Some(note) = &finding.note {
        lines.push(Line::from(format!("      nota: {}", display_safe(note))));
    }
    if let Some(reason) = unselectable_reason(finding) {
        lines.push(Line::from(format!("      {}", display_safe(&reason))).dark_gray());
    }
    ListItem::new(Text::from(lines))
}
