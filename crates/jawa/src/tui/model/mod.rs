use std::cmp::Reverse;
use std::collections::BTreeSet;

use crossterm::event::KeyEvent;
use jawa_core::clean::{CleanReport, CleanRequest, Progress};
use jawa_core::model::finding::{
    Action, BlockReason, CategoryId, Finding, FindingId, Reversibility, Risk,
};
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::registry;
use jawa_core::scan::warnings::selection_warnings;

mod keys;

use keys::on_key;

pub const CONFIRM_WORD: &str = "apagar";
const MAX_CONFIRM_INPUT: usize = 32;
const WAIT_SCAN: &str = "Aguarde a medição terminar.";
const NOTHING_SELECTED: &str = "Marque pelo menos um item com Espaço antes de limpar.";
const GUIDED_ONLY: &str = "Item guiado: o Jawa só mostra o comando (tecla g).";
const CANCELLING: &str = "Cancelando: o item atual termina e o resto é pulado.";
const CLEAN_FAILED: &str = "Algo deu errado na limpeza; tecle m para medir de novo.";
const PAGE_LINES: u16 = 10;
const SCROLL_MARGIN: usize = 12;
const TEXT_SCREEN_ROWS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Overview,
    Category(CategoryId),
    Explain(CategoryId),
    Confirm,
    Cleaning,
    Results,
    Guide,
    Help,
}

#[derive(Debug)]
pub enum Msg {
    Key(KeyEvent),
    ScanStarted,
    CategoryScanned(CategoryScan),
    ScanFinished,
    CleanProgress(Progress),
    CleanFinished(CleanReport),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    StartScan,
    StartClean(CleanRequest),
    CancelClean,
    Quit,
}

#[derive(Debug, Default)]
pub struct Model {
    pub screen: Screen,
    pub scans: Vec<CategoryScan>,
    pub selected: BTreeSet<FindingId>,
    pub cursor: usize,
    pub confirm_input: String,
    pub progress: Option<Progress>,
    pub report: Option<CleanReport>,
    pub disk: Option<(u64, u64)>,
    pub scanning: bool,
    pub cancelling: bool,
    pub status: Option<String>,
    pub scroll: u16,
    history: Vec<(Screen, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideLine {
    pub label: String,
    pub command: String,
}

impl Model {
    pub fn new() -> Model {
        Model::default()
    }

    pub fn scan_of(&self, id: CategoryId) -> Option<&CategoryScan> {
        self.scans.iter().find(|scan| scan.id == id)
    }

    pub fn findings_of(&self, id: CategoryId) -> Vec<&Finding> {
        let mut findings: Vec<&Finding> = self
            .scan_of(id)
            .and_then(|scan| scan.result.as_ref().ok())
            .map(|output| output.findings.iter().collect())
            .unwrap_or_default();
        findings.sort_by_key(|finding| Reverse(finding.bytes));
        findings
    }

    pub fn selected_findings(&self) -> Vec<&Finding> {
        category_ids()
            .into_iter()
            .flat_map(|id| self.findings_of(id))
            .filter(|finding| self.selected.contains(&finding.id) && is_selectable(finding))
            .collect()
    }

    pub fn has_irreversible_selection(&self) -> bool {
        self.selected_findings()
            .iter()
            .any(|finding| finding.reversibility == Reversibility::Irreversible)
    }

    pub fn confirm_warnings(&self) -> Vec<&'static str> {
        selection_warnings(&self.selected_findings())
    }

    pub fn guide_lines(&self) -> Vec<GuideLine> {
        category_ids()
            .into_iter()
            .flat_map(|id| self.findings_of(id))
            .filter_map(guide_line)
            .collect()
    }

    fn open(&mut self, screen: Screen) {
        self.history.push((self.screen, self.cursor));
        self.screen = screen;
        self.cursor = 0;
        self.scroll = 0;
    }

    fn back(&mut self) {
        let (screen, cursor) = self.history.pop().unwrap_or_default();
        self.screen = screen;
        self.cursor = cursor;
        self.scroll = 0;
    }

    fn reset_to(&mut self, screen: Screen) {
        self.history.clear();
        self.screen = screen;
        self.cursor = 0;
        self.scroll = 0;
    }
}

pub fn category_ids() -> Vec<CategoryId> {
    registry()
        .iter()
        .map(|category| category.descriptor().id)
        .collect()
}

pub fn is_selectable(finding: &Finding) -> bool {
    unselectable_reason(finding).is_none()
}

pub fn unselectable_reason(finding: &Finding) -> Option<String> {
    if let Some(reason) = &finding.blocked {
        return Some(format!("Bloqueado: {reason}."));
    }
    let guided = finding.risk == Risk::Guided || matches!(finding.action, Action::Guide(_));
    guided.then(|| GUIDED_ONLY.to_string())
}

fn starts_selected(finding: &Finding) -> bool {
    finding.risk == Risk::Safe && is_selectable(finding)
}

fn guide_line(finding: &Finding) -> Option<GuideLine> {
    let command = match (&finding.action, &finding.blocked) {
        (_, Some(BlockReason::ForeignOwned { fix, .. })) => fix.clone(),
        (Action::Guide(guide), _) => guide.command_line.clone(),
        _ => return None,
    };
    Some(GuideLine {
        label: format!("{} ({})", finding.label, finding.location),
        command,
    })
}

pub fn update(model: &mut Model, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Key(key) => return on_key(model, key),
        Msg::ScanStarted => model.scanning = true,
        Msg::CategoryScanned(scan) => on_category_scanned(model, scan),
        Msg::ScanFinished => model.scanning = false,
        Msg::CleanProgress(progress) => model.progress = Some(progress),
        Msg::CleanFinished(report) => on_clean_finished(model, report),
        Msg::Error(text) => on_error(model, text),
    }
    Vec::new()
}

pub fn begin_scan(model: &mut Model) -> Vec<Effect> {
    if model.scanning {
        model.status = Some(WAIT_SCAN.to_string());
        return Vec::new();
    }
    model.scans.clear();
    model.selected.clear();
    model.report = None;
    model.reset_to(Screen::Overview);
    model.scanning = true;
    vec![Effect::StartScan]
}

fn on_category_scanned(model: &mut Model, scan: CategoryScan) {
    if let Ok(output) = &scan.result {
        let preselected = output
            .findings
            .iter()
            .filter(|finding| starts_selected(finding));
        model
            .selected
            .extend(preselected.map(|finding| finding.id.clone()));
    }
    model.scans.retain(|known| known.id != scan.id);
    model.scans.push(scan);
}

fn on_clean_finished(model: &mut Model, report: CleanReport) {
    let processed: BTreeSet<FindingId> = report
        .items
        .iter()
        .map(|(finding, _)| finding.id.clone())
        .collect();
    for scan in &mut model.scans {
        if let Ok(output) = &mut scan.result {
            output
                .findings
                .retain(|finding| !processed.contains(&finding.id));
        }
    }
    model.selected.retain(|id| !processed.contains(id));
    model.progress = None;
    model.cancelling = false;
    model.report = Some(report);
    model.reset_to(Screen::Results);
}

fn on_error(model: &mut Model, text: String) {
    if model.screen != Screen::Cleaning {
        model.status = Some(text);
        return;
    }
    model.progress = None;
    model.cancelling = false;
    model.selected.clear();
    model.reset_to(Screen::Overview);
    model.status = Some(format!("{CLEAN_FAILED} Detalhe: {text}"));
}

#[cfg(test)]
mod tests;
