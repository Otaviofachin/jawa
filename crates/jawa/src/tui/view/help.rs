//! Ajuda com as teclas.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use super::super::model::Model;

use super::*;

pub(super) const HELP_LINES: [&str; 13] = [
    "↑/↓ ou j/k   move o cursor (ou rola o texto)",
    "Enter        abre a categoria",
    "Espaço       marca ou desmarca o item",
    "e            explica a categoria",
    "l            confirma a limpeza dos itens marcados",
    "g            comandos guiados para copiar",
    "m            mede de novo (desmarca tudo)",
    "?            esta ajuda",
    "Esc          volta",
    "q            sai (durante a limpeza, cancela)",
    "",
    "Seguro vem marcado; Revisar, desmarcado; Guiado e bloqueado não podem ser marcados.",
    "Itens irreversíveis pedem que você digite apagar na confirmação.",
];

pub(super) fn draw_help(frame: &mut Frame, area: Rect, model: &Model) {
    let lines = HELP_LINES.iter().map(|text| Line::from(*text)).collect();
    draw_text(frame, area, model, lines);
}
