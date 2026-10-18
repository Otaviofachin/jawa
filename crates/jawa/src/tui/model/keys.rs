//! Teclas de cada tela: navegacao, marcacao, confirmacao com `apagar` e rolagem.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use jawa_core::clean::{CleanRequest, Progress};
use jawa_core::model::finding::{CategoryId, FindingId};

use super::*;

pub(super) fn on_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    if key.kind == KeyEventKind::Release {
        return Vec::new();
    }
    if model.screen == Screen::Cleaning {
        return on_cleaning_key(model, key);
    }
    if is_ctrl_c(&key) {
        return vec![Effect::Quit];
    }
    model.status = None;
    match model.screen {
        Screen::Overview => on_overview_key(model, key),
        Screen::Category(id) => on_category_key(model, id, key),
        Screen::Confirm => on_confirm_key(model, key),
        Screen::Results => on_results_key(model, key),
        Screen::Explain(_) | Screen::Guide | Screen::Help => on_text_key(model, key),
        Screen::Cleaning => Vec::new(),
    }
}

pub(super) fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// `↑/↓` e `j/k`; devolve se a tecla era de movimento.
pub(super) fn move_cursor(model: &mut Model, code: KeyCode, len: usize) -> bool {
    match code {
        KeyCode::Up | KeyCode::Char('k') => model.cursor = model.cursor.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            if model.cursor + 1 < len {
                model.cursor += 1;
            }
        }
        _ => return false,
    }
    true
}

/// Teclas que valem na visao geral, na categoria e no resultado.
pub(super) fn on_global_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    match key.code {
        KeyCode::Char('q') => return vec![Effect::Quit],
        KeyCode::Char('m') => return begin_scan(model),
        KeyCode::Char('l') => open_confirm(model),
        KeyCode::Char('g') => model.open(Screen::Guide),
        KeyCode::Char('?') => model.open(Screen::Help),
        _ => {}
    }
    Vec::new()
}

pub(super) fn on_overview_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    let ids = category_ids();
    if move_cursor(model, key.code, ids.len()) {
        return Vec::new();
    }
    let Some(&id) = ids.get(model.cursor) else {
        return on_global_key(model, key);
    };
    match key.code {
        KeyCode::Enter => model.open(Screen::Category(id)),
        KeyCode::Char('e') => model.open(Screen::Explain(id)),
        _ => return on_global_key(model, key),
    }
    Vec::new()
}

pub(super) fn on_category_key(model: &mut Model, id: CategoryId, key: KeyEvent) -> Vec<Effect> {
    let count = model.findings_of(id).len();
    if move_cursor(model, key.code, count) {
        return Vec::new();
    }
    match key.code {
        KeyCode::Char(' ') => toggle_at_cursor(model, id),
        KeyCode::Char('e') => model.open(Screen::Explain(id)),
        KeyCode::Esc => model.back(),
        _ => return on_global_key(model, key),
    }
    Vec::new()
}

pub(super) fn toggle_at_cursor(model: &mut Model, id: CategoryId) {
    let Some((finding_id, refusal)) = model
        .findings_of(id)
        .get(model.cursor)
        .map(|finding| (finding.id.clone(), unselectable_reason(finding)))
    else {
        return;
    };
    if let Some(reason) = refusal {
        model.status = Some(reason);
    } else if !model.selected.remove(&finding_id) {
        model.selected.insert(finding_id);
    }
}

pub(super) fn open_confirm(model: &mut Model) {
    if model.scanning {
        model.status = Some(WAIT_SCAN.to_string());
    } else if model.selected_findings().is_empty() {
        model.status = Some(NOTHING_SELECTED.to_string());
    } else {
        model.confirm_input.clear();
        model.open(Screen::Confirm);
    }
}

/// Com irreversivel marcado, as letras vao para o campo `apagar`.
pub(super) fn on_confirm_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    let typing = model.has_irreversible_selection();
    if scroll_key(model, key.code, !typing) {
        return Vec::new();
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc => {
            model.confirm_input.clear();
            model.back();
        }
        KeyCode::Enter => return confirm_clean(model),
        KeyCode::Backspace => {
            model.confirm_input.pop();
        }
        KeyCode::Char(character) if typing => {
            if model.confirm_input.chars().count() < MAX_CONFIRM_INPUT {
                model.confirm_input.push(character);
            }
        }
        KeyCode::Char('q') => return vec![Effect::Quit],
        _ => {}
    }
    Vec::new()
}

pub(super) fn confirm_clean(model: &mut Model) -> Vec<Effect> {
    let allow_irreversible = model.has_irreversible_selection();
    if allow_irreversible && model.confirm_input != CONFIRM_WORD {
        model.status = Some(format!(
            "Há itens irreversíveis: digite {CONFIRM_WORD} e tecle Enter."
        ));
        return Vec::new();
    }
    let ids: Vec<FindingId> = model
        .selected_findings()
        .iter()
        .map(|finding| finding.id.clone())
        .collect();
    if model.scanning || ids.is_empty() {
        return Vec::new();
    }
    model.progress = Some(Progress {
        done: 0,
        total: ids.len(),
        current: None,
    });
    model.confirm_input.clear();
    model.cancelling = false;
    model.report = None;
    model.reset_to(Screen::Cleaning);
    vec![Effect::StartClean(CleanRequest {
        ids,
        allow_irreversible,
    })]
}

/// Durante a limpeza, sair vira cancelar: o item atual termina antes.
pub(super) fn on_cleaning_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    let wants_out = key.code == KeyCode::Char('q') || is_ctrl_c(&key);
    if !wants_out {
        return Vec::new();
    }
    model.cancelling = true;
    model.status = Some(CANCELLING.to_string());
    vec![Effect::CancelClean]
}

pub(super) fn on_results_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    if scroll_key(model, key.code, true) {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            model.reset_to(Screen::Overview);
            Vec::new()
        }
        _ => on_global_key(model, key),
    }
}

pub(super) fn on_text_key(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
    if scroll_key(model, key.code, true) {
        return Vec::new();
    }
    match key.code {
        KeyCode::Esc => model.back(),
        KeyCode::Char('q') => return vec![Effect::Quit],
        _ => {}
    }
    Vec::new()
}

/// Setas, PageUp/PageDown e Home/End rolam; `j/k` so quando `letters`
/// (no campo `apagar` as letras sao texto). Devolve se a tecla era de rolagem.
pub(super) fn scroll_key(model: &mut Model, code: KeyCode, letters: bool) -> bool {
    let limit = scroll_limit(model);
    let next = match code {
        KeyCode::Up => model.scroll.saturating_sub(1),
        KeyCode::Down => model.scroll.saturating_add(1),
        KeyCode::Char('k') if letters => model.scroll.saturating_sub(1),
        KeyCode::Char('j') if letters => model.scroll.saturating_add(1),
        KeyCode::PageUp => model.scroll.saturating_sub(PAGE_LINES),
        KeyCode::PageDown => model.scroll.saturating_add(PAGE_LINES),
        KeyCode::Home => 0,
        KeyCode::End => limit,
        _ => return false,
    };
    model.scroll = next.min(limit);
    true
}

/// Teto da rolagem pelo tamanho do conteudo, para a rolagem nao acumular
/// alem do fim (a view ainda ajusta pelo numero real de linhas).
pub(super) fn scroll_limit(model: &Model) -> u16 {
    let content_lines = match model.screen {
        Screen::Confirm => model.selected_findings().len(),
        Screen::Results => model
            .report
            .as_ref()
            .map_or(0, |report| report.items.len() + report.warnings.len()),
        Screen::Guide => model.guide_lines().len() * 2,
        _ => TEXT_SCREEN_ROWS,
    };
    // Cada linha pode quebrar em duas na tela.
    u16::try_from(content_lines * 2 + SCROLL_MARGIN).unwrap_or(u16::MAX)
}
