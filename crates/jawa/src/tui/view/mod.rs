use jawa_core::model::finding::Risk;
use jawa_core::scan::categories::{category, registry};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};

use super::model::{Model, Screen};
use crate::format::human_bytes;

mod category;
mod cleaning;
mod confirm;
mod explain;
mod guide;
mod help;
mod overview;
mod results;

use category::draw_category;
use cleaning::draw_cleaning;
use confirm::draw_confirm;
use explain::draw_explain;
use guide::draw_guide;
use help::draw_help;
use overview::draw_overview;
use results::draw_results;

const TITLE_WIDTH: usize = 38;

pub fn draw(frame: &mut Frame, model: &Model) {
    let [header, body, status, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let title = format!("Jawa · {}", screen_title(model.screen));
    frame.render_widget(Line::from(title).bold(), header);
    match model.screen {
        Screen::Overview => draw_overview(frame, body, model),
        Screen::Category(id) => draw_category(frame, body, model, id),
        Screen::Explain(id) => draw_explain(frame, body, model, id),
        Screen::Confirm => draw_confirm(frame, body, model),
        Screen::Cleaning => draw_cleaning(frame, body, model),
        Screen::Results => draw_results(frame, body, model),
        Screen::Guide => draw_guide(frame, body, model),
        Screen::Help => draw_help(frame, body, model),
    }
    frame.render_widget(status_line(model), status);
    frame.render_widget(Line::from(footer_keys(model.screen)).dark_gray(), footer);
}

fn screen_title(screen: Screen) -> &'static str {
    match screen {
        Screen::Overview => "visão geral",
        Screen::Category(id) | Screen::Explain(id) => category(id).descriptor().title,
        Screen::Confirm => "confirmar limpeza",
        Screen::Cleaning => "limpando",
        Screen::Results => "resultado",
        Screen::Guide => "comandos guiados",
        Screen::Help => "ajuda",
    }
}

fn footer_keys(screen: Screen) -> &'static str {
    match screen {
        Screen::Overview => {
            "↑/↓ move · Enter abre · e explica · l limpa · g guia · m mede · ? ajuda · q sai"
        }
        Screen::Category(_) => "↑/↓ move · Espaço marca · e explica · l limpa · Esc volta · q sai",
        Screen::Explain(_) | Screen::Guide | Screen::Help => "↑/↓ rola · Esc volta · q sai",
        Screen::Confirm => "↑/↓ rola · Enter confirma · Backspace apaga · Esc volta",
        Screen::Cleaning => "q ou Ctrl+C cancela",
        Screen::Results => "↑/↓ rola · Esc volta · m mede de novo · g guia · q sai",
    }
}

fn status_line(model: &Model) -> Line<'static> {
    if let Some(status) = &model.status {
        return Line::from(status.clone()).yellow();
    }
    if model.scanning {
        let total = registry().len();
        return Line::from(format!(
            "Medindo… {} de {total} categorias",
            model.scans.len()
        ));
    }
    let selected = model.selected_findings();
    let bytes: u64 = selected.iter().map(|finding| finding.bytes).sum();
    Line::from(format!(
        "Marcados: {} item(ns), {}",
        selected.len(),
        human_bytes(bytes)
    ))
}

fn risk_color(risk: Risk) -> Color {
    match risk {
        Risk::Safe => Color::Green,
        Risk::Review => Color::Yellow,
        Risk::Guided => Color::Blue,
    }
}

fn highlighted_list<'a>(items: Vec<ListItem<'a>>, title: &'a str) -> List<'a> {
    List::new(items)
        .block(Block::bordered().title(title))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ")
}

pub(super) fn draw_text(frame: &mut Frame, area: Rect, model: &Model, lines: Vec<Line<'static>>) {
    draw_scrolled(frame, area, model.scroll, lines, Block::bordered());
}

pub(super) fn draw_scrolled(
    frame: &mut Frame,
    area: Rect,
    scroll: u16,
    lines: Vec<Line<'static>>,
    block: Block<'static>,
) {
    let inner = block.inner(area);
    let rows = wrapped_rows(&lines, inner.width);
    let max_scroll = rows.saturating_sub(inner.height);
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll.min(max_scroll), 0));
    frame.render_widget(paragraph, area);
}

fn wrapped_rows(lines: &[Line], width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let rows: usize = lines
        .iter()
        .map(|line| line.width().div_ceil(width).max(1))
        .sum();
    u16::try_from(rows).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests;
