use super::*;
use crossterm::event::{KeyCode, KeyModifiers};
use jawa_core::clean::ItemOutcome;
use jawa_core::model::finding::{Action, BlockReason, Finding, GuideSpec, Reversibility, Risk};
use jawa_core::scan::categories::ScanOutput;
use std::path::PathBuf;

const REINSTALL_WARNING: &str = "As reinstalações vão baixar tudo da internet.";

fn finding(category: CategoryId, path: &str, risk: Risk) -> Finding {
    let action = Action::Delete(PathBuf::from(path));
    Finding {
        id: FindingId::of(category, &action),
        category,
        label: path.to_string(),
        location: path.to_string(),
        bytes: 2_000_000,
        risk,
        reversibility: Reversibility::Regenerable,
        note: None,
        action,
        fingerprint: None,
        blocked: None,
    }
}

fn irreversible(category: CategoryId, path: &str) -> Finding {
    Finding {
        reversibility: Reversibility::Irreversible,
        ..finding(category, path, Risk::Review)
    }
}

fn guided(category: CategoryId, command_line: &str) -> Finding {
    let action = Action::Guide(GuideSpec {
        command_line: command_line.to_string(),
    });
    Finding {
        id: FindingId::of(category, &action),
        action,
        ..finding(category, command_line, Risk::Guided)
    }
}

fn scanned(id: CategoryId, findings: Vec<Finding>) -> Msg {
    Msg::CategoryScanned(CategoryScan {
        id,
        result: Ok(ScanOutput {
            findings,
            footnote: None,
        }),
    })
}

fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl_c() -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
}

fn press(model: &mut Model, code: KeyCode) -> Vec<Effect> {
    update(model, key(code))
}

fn type_text(model: &mut Model, text: &str) {
    for character in text.chars() {
        press(model, KeyCode::Char(character));
    }
}

fn measured(scans: Vec<Msg>) -> Model {
    let mut model = Model::new();
    update(&mut model, Msg::ScanStarted);
    for msg in scans {
        update(&mut model, msg);
    }
    update(&mut model, Msg::ScanFinished);
    model
}

/// O npm-cache e a primeira linha da visao geral.
fn open_npm_cache(model: &mut Model) {
    press(model, KeyCode::Enter);
    assert_eq!(model.screen, Screen::Category(CategoryId::NpmCache));
}

fn start_clean(model: &mut Model) -> Vec<Effect> {
    press(model, KeyCode::Char('l'));
    assert_eq!(model.screen, Screen::Confirm);
    press(model, KeyCode::Enter)
}

fn sample_report(items: Vec<(Finding, ItemOutcome)>) -> CleanReport {
    CleanReport {
        items,
        estimated_bytes: 2_000_000,
        free_before: 10_000_000,
        free_after: 12_000_000,
        warnings: Vec::new(),
    }
}

#[test]
fn safe_items_start_selected() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let model = measured(vec![scanned(CategoryId::NpmCache, vec![safe.clone()])]);
    assert!(model.selected.contains(&safe.id));
}

#[test]
fn review_items_start_unselected() {
    let review = finding(CategoryId::IdleNodeModules, "/p/node_modules", Risk::Review);
    let model = measured(vec![scanned(CategoryId::IdleNodeModules, vec![review])]);
    assert!(model.selected.is_empty());
}

#[test]
fn blocked_and_guided_cannot_be_selected() {
    let blocked = Finding {
        bytes: 9_000_000,
        blocked: Some(BlockReason::CrossesMount),
        ..finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe)
    };
    let guide = guided(CategoryId::NpmCache, "sudo apt-get clean");
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![blocked, guide])]);
    assert!(model.selected.is_empty());
    open_npm_cache(&mut model);
    press(&mut model, KeyCode::Char(' '));
    assert!(model.selected.is_empty());
    let status = model.status.clone().unwrap_or_default();
    assert!(
        status.contains("atravessa um ponto de montagem"),
        "{status}"
    );
    press(&mut model, KeyCode::Down);
    press(&mut model, KeyCode::Char(' '));
    assert!(model.selected.is_empty());
    let status = model.status.clone().unwrap_or_default();
    assert!(status.contains("guiado"), "{status}");
}

#[test]
fn space_toggles_a_review_item() {
    let review = finding(CategoryId::NpmCache, "/h/.npm/_npx", Risk::Review);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![review.clone()])]);
    open_npm_cache(&mut model);
    press(&mut model, KeyCode::Char(' '));
    assert!(model.selected.contains(&review.id));
    press(&mut model, KeyCode::Char(' '));
    assert!(model.selected.is_empty());
}

#[test]
fn confirm_requires_typing_apagar_for_irreversible() {
    let item = irreversible(CategoryId::NpmCache, "/h/.npm/_cacache");
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![item.clone()])]);
    open_npm_cache(&mut model);
    press(&mut model, KeyCode::Char(' '));
    assert!(start_clean(&mut model).is_empty());
    assert_eq!(model.screen, Screen::Confirm);
    type_text(&mut model, "apaga");
    assert!(press(&mut model, KeyCode::Enter).is_empty());
    type_text(&mut model, "r");
    let effects = press(&mut model, KeyCode::Enter);
    let expected = CleanRequest {
        ids: vec![item.id],
        allow_irreversible: true,
    };
    assert_eq!(effects, vec![Effect::StartClean(expected)]);
    assert_eq!(model.screen, Screen::Cleaning);
}

#[test]
fn confirm_without_irreversible_is_enter() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe.clone()])]);
    let effects = start_clean(&mut model);
    let expected = CleanRequest {
        ids: vec![safe.id],
        allow_irreversible: false,
    };
    assert_eq!(effects, vec![Effect::StartClean(expected)]);
    assert_eq!(model.screen, Screen::Cleaning);
}

#[test]
fn confirm_shows_cross_warnings() {
    let cache = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let modules = finding(CategoryId::IdleNodeModules, "/p/node_modules", Risk::Review);
    let mut model = measured(vec![
        scanned(CategoryId::NpmCache, vec![cache]),
        scanned(CategoryId::IdleNodeModules, vec![modules.clone()]),
    ]);
    assert!(model.confirm_warnings().is_empty());
    model.selected.insert(modules.id);
    press(&mut model, KeyCode::Char('l'));
    assert_eq!(model.screen, Screen::Confirm);
    assert_eq!(model.confirm_warnings(), vec![REINSTALL_WARNING]);
}

#[test]
fn quit_during_clean_cancels() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe])]);
    start_clean(&mut model);
    assert_eq!(
        press(&mut model, KeyCode::Char('q')),
        vec![Effect::CancelClean]
    );
    assert_eq!(update(&mut model, ctrl_c()), vec![Effect::CancelClean]);
    assert_eq!(model.screen, Screen::Cleaning);
}

#[test]
fn rescan_clears_selection() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe])]);
    assert_eq!(
        press(&mut model, KeyCode::Char('m')),
        vec![Effect::StartScan]
    );
    assert!(model.selected.is_empty());
    assert!(model.scans.is_empty());
    assert_eq!(model.screen, Screen::Overview);
}

#[test]
fn l_without_selection_does_nothing() {
    let review = finding(CategoryId::NpmCache, "/h/.npm/_npx", Risk::Review);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![review])]);
    assert!(press(&mut model, KeyCode::Char('l')).is_empty());
    assert_eq!(model.screen, Screen::Overview);
}

#[test]
fn l_while_scanning_does_nothing() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = Model::new();
    update(&mut model, Msg::ScanStarted);
    update(&mut model, scanned(CategoryId::NpmCache, vec![safe]));
    assert!(press(&mut model, KeyCode::Char('l')).is_empty());
    assert_eq!(model.screen, Screen::Overview);
    assert!(press(&mut model, KeyCode::Char('m')).is_empty());
}

#[test]
fn quit_keys_outside_clean() {
    let mut model = measured(Vec::new());
    assert_eq!(press(&mut model, KeyCode::Char('q')), vec![Effect::Quit]);
    assert_eq!(update(&mut model, ctrl_c()), vec![Effect::Quit]);
}

#[test]
fn esc_returns_to_previous_screen() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe])]);
    open_npm_cache(&mut model);
    press(&mut model, KeyCode::Char('e'));
    assert_eq!(model.screen, Screen::Explain(CategoryId::NpmCache));
    press(&mut model, KeyCode::Esc);
    assert_eq!(model.screen, Screen::Category(CategoryId::NpmCache));
    press(&mut model, KeyCode::Esc);
    assert_eq!(model.screen, Screen::Overview);
    press(&mut model, KeyCode::Char('g'));
    assert_eq!(model.screen, Screen::Guide);
    press(&mut model, KeyCode::Esc);
    press(&mut model, KeyCode::Char('?'));
    assert_eq!(model.screen, Screen::Help);
}

#[test]
fn overview_cursor_moves_and_opens_category() {
    let mut model = measured(Vec::new());
    press(&mut model, KeyCode::Char('j'));
    press(&mut model, KeyCode::Down);
    press(&mut model, KeyCode::Char('k'));
    press(&mut model, KeyCode::Enter);
    assert_eq!(model.screen, Screen::Category(CategoryId::ToolCaches));
}

#[test]
fn clean_finished_shows_results_and_drops_processed_items() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe.clone()])]);
    start_clean(&mut model);
    let progress = Progress {
        done: 0,
        total: 1,
        current: Some(safe.label.clone()),
    };
    update(&mut model, Msg::CleanProgress(progress.clone()));
    assert_eq!(model.progress, Some(progress));
    let done = ItemOutcome::Done { output: None };
    update(
        &mut model,
        Msg::CleanFinished(sample_report(vec![(safe, done)])),
    );
    assert_eq!(model.screen, Screen::Results);
    assert!(model.report.is_some());
    assert!(model.selected.is_empty());
    assert!(model.findings_of(CategoryId::NpmCache).is_empty());
    press(&mut model, KeyCode::Esc);
    assert_eq!(model.screen, Screen::Overview);
}

#[test]
fn error_during_clean_returns_to_overview() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe])]);
    start_clean(&mut model);
    update(&mut model, Msg::Error("medição velha".to_string()));
    assert_eq!(model.screen, Screen::Overview);
    assert!(model.selected.is_empty());
    let status = model.status.clone().unwrap_or_default();
    assert!(
        status.starts_with("Algo deu errado na limpeza; tecle m para medir de novo."),
        "{status}"
    );
    assert!(status.contains("medição velha"), "{status}");
}

#[test]
fn error_outside_clean_keeps_selection() {
    let safe = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![safe])]);
    update(&mut model, Msg::Error("gravação falhou".to_string()));
    assert_eq!(model.status.as_deref(), Some("gravação falhou"));
    assert_eq!(model.selected.len(), 1);
}

fn at_irreversible_confirm() -> (Model, Finding) {
    let item = irreversible(CategoryId::NpmCache, "/h/.npm/_cacache");
    let mut model = measured(vec![scanned(CategoryId::NpmCache, vec![item.clone()])]);
    model.selected.insert(item.id.clone());
    press(&mut model, KeyCode::Char('l'));
    assert_eq!(model.screen, Screen::Confirm);
    (model, item)
}

#[test]
fn confirm_requires_exact_apagar() {
    let (mut model, _) = at_irreversible_confirm();
    type_text(&mut model, " apagar ");
    assert!(press(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.screen, Screen::Confirm);
}

#[test]
fn confirm_field_ignores_control_and_alt_keys() {
    let (mut model, item) = at_irreversible_confirm();
    update(
        &mut model,
        Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
    );
    update(
        &mut model,
        Msg::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)),
    );
    assert_eq!(model.confirm_input, "");
    type_text(&mut model, "apagar");
    let effects = press(&mut model, KeyCode::Enter);
    let expected = CleanRequest {
        ids: vec![item.id],
        allow_irreversible: true,
    };
    assert_eq!(effects, vec![Effect::StartClean(expected)]);
}

#[test]
fn q_typed_in_apagar_field_is_text() {
    let (mut model, _) = at_irreversible_confirm();
    assert!(press(&mut model, KeyCode::Char('q')).is_empty());
    assert_eq!(model.confirm_input, "q");
    assert_eq!(model.screen, Screen::Confirm);
    assert_eq!(update(&mut model, ctrl_c()), vec![Effect::Quit]);
}

#[test]
fn confirm_list_scrolls_with_arrows_only_while_typing() {
    let items: Vec<Finding> = (0..30)
        .map(|index| irreversible(CategoryId::NpmCache, &format!("/h/item{index}")))
        .collect();
    let mut model = measured(vec![scanned(CategoryId::NpmCache, items.clone())]);
    model
        .selected
        .extend(items.iter().map(|item| item.id.clone()));
    press(&mut model, KeyCode::Char('l'));
    press(&mut model, KeyCode::Down);
    press(&mut model, KeyCode::PageDown);
    assert!(model.scroll > 1);
    press(&mut model, KeyCode::Home);
    assert_eq!(model.scroll, 0);
    press(&mut model, KeyCode::Char('j'));
    assert_eq!(model.scroll, 0);
    assert_eq!(model.confirm_input, "j");
}

#[test]
fn results_scroll_keys_move_and_stay_bounded() {
    let items: Vec<(Finding, ItemOutcome)> = (0..30)
        .map(|index| {
            let item = finding(CategoryId::NpmCache, &format!("/h/item{index}"), Risk::Safe);
            (item, ItemOutcome::Done { output: None })
        })
        .collect();
    let mut model = measured(Vec::new());
    update(&mut model, Msg::CleanFinished(sample_report(items)));
    press(&mut model, KeyCode::Char('j'));
    press(&mut model, KeyCode::Down);
    assert_eq!(model.scroll, 2);
    press(&mut model, KeyCode::Char('k'));
    assert_eq!(model.scroll, 1);
    press(&mut model, KeyCode::End);
    let end = model.scroll;
    assert!(end >= 30);
    press(&mut model, KeyCode::PageDown);
    assert_eq!(model.scroll, end);
    press(&mut model, KeyCode::Home);
    assert_eq!(model.scroll, 0);
}

#[test]
fn guide_lines_list_guided_and_foreign_owned() {
    let guide = guided(CategoryId::System, "sudo apt-get clean");
    let foreign = Finding {
        blocked: Some(BlockReason::ForeignOwned {
            count: 3,
            fix: "sudo chown -R \"$USER\": '/h/x'".to_string(),
        }),
        ..finding(CategoryId::BuildOutputs, "/h/x", Risk::Safe)
    };
    let model = measured(vec![
        scanned(CategoryId::BuildOutputs, vec![foreign]),
        scanned(CategoryId::System, vec![guide]),
    ]);
    let commands: Vec<String> = model
        .guide_lines()
        .into_iter()
        .map(|line| line.command)
        .collect();
    assert_eq!(
        commands,
        vec!["sudo chown -R \"$USER\": '/h/x'", "sudo apt-get clean"]
    );
}
