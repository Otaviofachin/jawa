//! Os seis campos de explicacao de uma categoria.

use jawa_core::model::finding::CategoryId;
use jawa_core::scan::categories::category;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;

use super::super::model::Model;

use super::*;

pub(super) fn draw_explain(frame: &mut Frame, area: Rect, model: &Model, id: CategoryId) {
    let descriptor = category(id).descriptor();
    let explanation = &descriptor.explanation;
    let sections = [
        ("O que é", explanation.what_it_is),
        ("Por que cresce", explanation.why_it_grows),
        ("O que acontece ao limpar", explanation.effect),
        ("Dá para desfazer?", explanation.reversibility_detail),
        ("Custo para recriar", explanation.recreate_cost),
        ("Recomendação", explanation.recommendation),
    ];
    let mut lines = vec![Line::from(descriptor.summary), Line::default()];
    for (heading, text) in sections {
        lines.push(Line::from(heading).bold());
        lines.push(Line::from(format!("  {text}")));
    }
    draw_text(frame, area, model, lines);
}
