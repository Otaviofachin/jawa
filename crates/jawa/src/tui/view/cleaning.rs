use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Gauge, Paragraph, Wrap};

use super::super::model::Model;
use crate::format::display_safe;

pub(super) fn draw_cleaning(frame: &mut Frame, area: Rect, model: &Model) {
    let [gauge, current] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
    let (done, total, label) = model.progress.as_ref().map_or((0, 0, None), |progress| {
        (progress.done, progress.total, progress.current.clone())
    });
    let ratio = if total == 0 {
        0.0
    } else {
        done as f64 / total as f64
    };
    let widget = Gauge::default()
        .block(Block::bordered().title("Limpando"))
        .label(format!("{done} de {total}"))
        .ratio(ratio.clamp(0.0, 1.0))
        .gauge_style(Style::new().fg(Color::Green));
    frame.render_widget(widget, gauge);
    let text = label.map_or_else(String::new, |item| {
        format!("Agora: {}", display_safe(&item))
    });
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), current);
}
