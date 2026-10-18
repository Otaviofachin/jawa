use super::*;
use crate::tui::model::{Msg, Screen, update};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use jawa_core::clean::{CleanReport, ItemOutcome, SkipReason};
use jawa_core::model::finding::{
    Action, BlockReason, CategoryId, CommandSpec, Finding, FindingId, GuideSpec, Reversibility,
    Risk,
};
use jawa_core::scan::CategoryScan;
use jawa_core::scan::categories::ScanOutput;
use jawa_core::system::processes::Holder;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::path::PathBuf;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 30;
const GB: u64 = 1_000_000_000;

fn render(model: &Model) -> String {
    render_sized(model, HEIGHT)
}

fn render_sized(model: &Model, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, height)).unwrap();
    terminal.draw(|frame| draw(frame, model)).unwrap();
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn finding(category: CategoryId, path: &str, risk: Risk) -> Finding {
    let action = Action::Delete(PathBuf::from(path));
    Finding {
        id: FindingId::of(category, &action),
        category,
        label: format!("item {path}"),
        location: path.to_string(),
        bytes: 2 * GB,
        risk,
        reversibility: Reversibility::Regenerable,
        note: None,
        action,
        fingerprint: None,
        blocked: None,
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

fn model_with(id: CategoryId, findings: Vec<Finding>, footnote: Option<&str>) -> Model {
    let mut model = Model::new();
    let scan = CategoryScan {
        id,
        result: Ok(ScanOutput {
            findings,
            footnote: footnote.map(str::to_string),
        }),
    };
    update(&mut model, Msg::CategoryScanned(scan));
    model
}

fn report(estimated: u64, measured: u64, items: Vec<(Finding, ItemOutcome)>) -> CleanReport {
    CleanReport {
        items,
        estimated_bytes: estimated,
        free_before: 100 * GB,
        free_after: 100 * GB + measured,
        warnings: Vec::new(),
    }
}

#[test]
fn overview_shows_categories_and_disk() {
    let cache = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut model = model_with(CategoryId::NpmCache, vec![cache], None);
    model.disk = Some((50 * GB, 200 * GB));
    let screen = render(&model);
    assert!(screen.contains("50,0 GB livre de 200,0 GB"), "{screen}");
    assert!(screen.contains("Cache do npm"), "{screen}");
    assert!(screen.contains("2,0 GB"), "{screen}");
    assert!(screen.contains("Sistema (modo guiado)"), "{screen}");
}

#[test]
fn category_shows_blocked_reason() {
    let holder = Holder {
        pid: 4242,
        name: "node".to_string(),
    };
    let blocked = Finding {
        blocked: Some(BlockReason::InUse(vec![holder])),
        ..finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe)
    };
    let mut model = model_with(CategoryId::NpmCache, vec![blocked], None);
    model.screen = Screen::Category(CategoryId::NpmCache);
    let screen = render(&model);
    assert!(screen.contains("[-]"), "{screen}");
    assert!(screen.contains("em uso por node (PID 4242)"), "{screen}");
}

#[test]
fn category_shows_checkboxes_notes_and_footnote() {
    let safe = Finding {
        note: Some("reinstale com npm ci".to_string()),
        ..finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe)
    };
    let review = finding(CategoryId::NpmCache, "/h/.npm/_npx", Risk::Review);
    let footnote = Some("2 itens pequenos ficaram de fora.");
    let mut model = model_with(CategoryId::NpmCache, vec![safe, review], footnote);
    model.screen = Screen::Category(CategoryId::NpmCache);
    let screen = render(&model);
    assert!(screen.contains("[x]"), "{screen}");
    assert!(screen.contains("[ ]"), "{screen}");
    assert!(screen.contains("reinstale com npm ci"), "{screen}");
    assert!(
        screen.contains("2 itens pequenos ficaram de fora."),
        "{screen}"
    );
}

#[test]
fn names_with_control_characters_are_sanitized() {
    let hostile = Finding {
        label: "filme\u{1b}[2J.iso".to_string(),
        note: Some("nota\u{1b}]0;titulo\u{7}".to_string()),
        ..finding(CategoryId::Downloads, "/h/Downloads/filme", Risk::Review)
    };
    let footnote = Some("rodapé\u{1b}[31m");
    let mut model = model_with(CategoryId::Downloads, vec![hostile.clone()], footnote);
    model.screen = Screen::Category(CategoryId::Downloads);
    let screen = render(&model);
    assert!(screen.contains("filme\u{fffd}[2J.iso"), "{screen}");
    assert!(screen.contains("nota\u{fffd}]0;titulo\u{fffd}"), "{screen}");
    assert!(screen.contains("rodapé\u{fffd}[31m"), "{screen}");
}

#[test]
fn explain_shows_six_fields() {
    let mut model = Model::new();
    model.screen = Screen::Explain(CategoryId::NpmCache);
    let screen = render(&model);
    for heading in ["O que é", "Por que cresce", "Recomendação"] {
        assert!(screen.contains(heading), "{screen}");
    }
}

#[test]
fn confirm_shows_type_apagar_prompt() {
    let item = Finding {
        reversibility: Reversibility::Irreversible,
        ..finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe)
    };
    let mut model = model_with(CategoryId::NpmCache, vec![item], None);
    model.screen = Screen::Confirm;
    model.confirm_input = "apa".to_string();
    let screen = render(&model);
    assert!(screen.contains("Irreversível"), "{screen}");
    assert!(screen.contains("digite apagar"), "{screen}");
    assert!(screen.contains("> apa"), "{screen}");
}

#[test]
fn confirm_without_irreversible_shows_warnings_and_enter() {
    let cache = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let trash = Finding {
        reversibility: Reversibility::Trash,
        ..finding(CategoryId::NpmCache, "/h/x.iso", Risk::Safe)
    };
    let mut model = model_with(CategoryId::NpmCache, vec![cache, trash], None);
    model.screen = Screen::Confirm;
    let screen = render(&model);
    assert!(screen.contains("Total estimado: 4,0 GB"), "{screen}");
    assert!(screen.contains("O espaço só volta quando você esvaziar a Lixeira."));
    assert!(screen.contains("Enter limpa"), "{screen}");
    assert!(!screen.contains("digite apagar"), "{screen}");
}

#[test]
fn cleaning_shows_gauge_and_current_item() {
    let mut model = Model::new();
    model.screen = Screen::Cleaning;
    model.progress = Some(jawa_core::clean::Progress {
        done: 1,
        total: 4,
        current: Some("Cache do npm".to_string()),
    });
    let screen = render(&model);
    assert!(screen.contains("1 de 4"), "{screen}");
    assert!(screen.contains("Cache do npm"), "{screen}");
}

#[test]
fn results_shows_snapshot_hint_when_measured_below_half() {
    let mut model = Model::new();
    model.screen = Screen::Results;
    model.report = Some(report(10 * GB, GB, Vec::new()));
    let screen = render(&model);
    assert!(screen.contains("10,0 GB"), "{screen}");
    assert!(screen.contains("1,0 GB"), "{screen}");
    assert!(screen.contains("snapshots"), "{screen}");
    model.report = Some(report(10 * GB, 6 * GB, Vec::new()));
    assert!(!render(&model).contains("snapshots"));
}

#[test]
fn results_shows_skipped_reasons_warnings_and_trash_hint() {
    let trashed = Finding {
        action: Action::Trash(PathBuf::from("/h/x.iso")),
        reversibility: Reversibility::Trash,
        ..finding(CategoryId::Downloads, "/h/x.iso", Risk::Review)
    };
    let skipped = finding(CategoryId::NpmCache, "/h/.npm/_cacache", Risk::Safe);
    let mut outcome = report(
        2 * GB,
        2 * GB,
        vec![
            (trashed, ItemOutcome::Done { output: None }),
            (skipped, ItemOutcome::Skipped(SkipReason::Changed)),
        ],
    );
    outcome
        .warnings
        .push("não foi possível gravar o histórico".to_string());
    let mut model = Model::new();
    model.screen = Screen::Results;
    model.report = Some(outcome);
    let screen = render(&model);
    assert!(screen.contains("mudou desde a medição"), "{screen}");
    assert!(
        screen.contains("não foi possível gravar o histórico"),
        "{screen}"
    );
    assert!(screen.contains("Na Lixeira: 2,0 GB"), "{screen}");
}

#[test]
fn guide_lists_commands() {
    let guide = guided(CategoryId::System, "sudo apt-get clean");
    let foreign = Finding {
        blocked: Some(BlockReason::ForeignOwned {
            count: 2,
            fix: "sudo chown -R \"$USER\": '/h/x'".to_string(),
        }),
        ..finding(CategoryId::System, "/h/x", Risk::Safe)
    };
    let mut model = model_with(CategoryId::System, vec![guide, foreign], None);
    model.screen = Screen::Guide;
    let screen = render(&model);
    assert!(screen.contains("1. sudo apt-get clean"), "{screen}");
    assert!(
        screen.contains("2. sudo chown -R \"$USER\": '/h/x'"),
        "{screen}"
    );
}

#[test]
fn help_lists_keys() {
    let mut model = Model::new();
    model.screen = Screen::Help;
    let screen = render(&model);
    assert!(screen.contains("Espaço"), "{screen}");
    assert!(screen.contains("apagar"), "{screen}");
}

#[test]
fn status_line_shows_message() {
    let mut model = Model::new();
    model.status = Some("Bloqueado: em uso.".to_string());
    assert!(render(&model).contains("Bloqueado: em uso."));
}

const SHORT: u16 = 20;

#[test]
fn confirm_total_always_visible_with_many_items() {
    let items: Vec<Finding> = (0..40)
        .map(|index| Finding {
            reversibility: Reversibility::Irreversible,
            ..finding(
                CategoryId::NpmCache,
                &format!("/h/item{index:02}"),
                Risk::Safe,
            )
        })
        .collect();
    let mut model = model_with(CategoryId::NpmCache, items, None);
    model.screen = Screen::Confirm;
    model.confirm_input = "apa".to_string();
    let screen = render_sized(&model, SHORT);
    assert!(screen.contains("Total estimado: 80,0 GB"), "{screen}");
    assert!(screen.contains("digite apagar"), "{screen}");
    assert!(screen.contains("> apa_"), "{screen}");
    assert!(!screen.contains("item /h/item39"), "{screen}");
    model.scroll = 39;
    let scrolled = render_sized(&model, SHORT);
    assert!(scrolled.contains("item /h/item39"), "{scrolled}");
    assert!(scrolled.contains("> apa_"), "{scrolled}");
}

#[test]
fn results_scrolls_to_show_warnings() {
    let items: Vec<(Finding, ItemOutcome)> = (0..40)
        .map(|index| {
            let item = finding(
                CategoryId::NpmCache,
                &format!("/h/item{index:02}"),
                Risk::Safe,
            );
            (item, ItemOutcome::Skipped(SkipReason::Changed))
        })
        .collect();
    let mut outcome = report(2 * GB, 2 * GB, items);
    outcome.warnings.push("aviso no fim da lista".to_string());
    let mut model = Model::new();
    update(&mut model, Msg::CleanFinished(outcome));
    assert!(!render_sized(&model, SHORT).contains("aviso no fim da lista"));
    update(
        &mut model,
        Msg::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
    );
    let screen = render_sized(&model, SHORT);
    assert!(screen.contains("aviso no fim da lista"), "{screen}");
}

#[test]
fn long_confirm_notes_keep_the_input_line() {
    let item = Finding {
        reversibility: Reversibility::Trash,
        note: Some("registry privado".to_string()),
        ..finding(CategoryId::Downloads, "/h/x.iso", Risk::Review)
    };
    let irreversible_item = Finding {
        reversibility: Reversibility::Irreversible,
        ..finding(CategoryId::NpmCache, "/h/y", Risk::Safe)
    };
    let mut model = model_with(CategoryId::NpmCache, vec![irreversible_item], None);
    update(
        &mut model,
        Msg::CategoryScanned(CategoryScan {
            id: CategoryId::Downloads,
            result: Ok(ScanOutput {
                findings: vec![item.clone()],
                footnote: None,
            }),
        }),
    );
    model.selected.insert(item.id);
    model.screen = Screen::Confirm;
    let mut terminal = Terminal::new(TestBackend::new(40, SHORT)).unwrap();
    terminal.draw(|frame| draw(frame, &model)).unwrap();
    let buffer = terminal.backend().buffer();
    let screen: String = buffer
        .content()
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(screen.contains("> _"), "{screen}");
}

#[test]
fn results_split_trash_and_docker_desktop() {
    let trashed = Finding {
        action: Action::Trash(PathBuf::from("/h/x.iso")),
        reversibility: Reversibility::Trash,
        ..finding(CategoryId::Downloads, "/h/x.iso", Risk::Review)
    };
    let in_vm = Finding {
        action: Action::Run(CommandSpec {
            program: PathBuf::from("/usr/bin/docker"),
            args: ["image", "prune", "-a", "-f"].map(String::from).to_vec(),
            stdin: None,
            env: vec![("DOCKER_CONTEXT".to_string(), "desktop-linux".to_string())],
            timeout: std::time::Duration::from_secs(1),
        }),
        ..finding(CategoryId::DockerImages, "docker image prune", Risk::Safe)
    };
    let done = || ItemOutcome::Done { output: None };
    let mut model = Model::new();
    model.screen = Screen::Results;
    model.report = Some(report(
        6 * GB,
        2 * GB,
        vec![(trashed, done()), (in_vm, done())],
    ));
    let screen = render(&model);
    assert!(
        screen.contains("Liberado no disco (medido): 2,0 GB"),
        "{screen}"
    );
    assert!(screen.contains("Na Lixeira: 2,0 GB"), "{screen}");
    assert!(
        screen.contains("Dentro da VM do Docker Desktop: 2,0 GB"),
        "{screen}"
    );
    assert!(screen.contains("desktop-reclaim-space"), "{screen}");
    assert!(!screen.contains("snapshots"), "{screen}");
}
