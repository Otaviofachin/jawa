//! Comandos do modo guiado para copiar.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;

use super::super::model::Model;
use crate::format::display_safe;

use super::*;

pub(super) fn draw_guide(frame: &mut Frame, area: Rect, model: &Model) {
    let guide = model.guide_lines();
    let mut lines = vec![
        Line::from("Rode estes comandos você mesmo, se quiser. O Jawa nunca os executa."),
        Line::default(),
    ];
    if guide.is_empty() {
        lines.push(Line::from("Nenhum comando guiado nesta medição."));
    }
    for (index, line) in guide.into_iter().enumerate() {
        lines.push(Line::from(format!("{}. {}", index + 1, display_safe(&line.command))).bold());
        lines.push(Line::from(format!("   {}", display_safe(&line.label))).dark_gray());
    }
    draw_text(frame, area, model, lines);
}
