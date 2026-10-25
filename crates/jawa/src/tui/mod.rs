pub mod model;
pub mod view;
pub mod worker;

use std::io::{self, IsTerminal, Stdout};
use std::panic;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use jawa_core::clean::CleanRequest;
use jawa_core::error::CoreError;
use jawa_core::safety::ledger::{InstanceLock, Ledger};
use jawa_core::system::disk::{free_bytes, total_bytes};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use thiserror::Error;

use self::model::{Effect, Model, Msg, begin_scan, update};
use self::worker::WORKER_THREAD_PREFIX;
use jawa_core::api::Session;

const POLL_INTERVAL: Duration = Duration::from_millis(100);

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

#[derive(Debug, Error)]
pub enum TuiError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("erro de entrada e saída no terminal: {0}")]
    Io(#[from] io::Error),
    #[error(
        "a interface precisa de um terminal interativo; fora dele, use `jawa medir` e `jawa limpar`"
    )]
    NoTerminal,
}

pub fn run(session: Session) -> Result<(), TuiError> {
    let _lock = InstanceLock::acquire(&session.env)?;
    if !(io::stdin().is_terminal() && io::stdout().is_terminal()) {
        return Err(TuiError::NoTerminal);
    }
    install_panic_hook();
    let _screen = ScreenGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let (tx, rx) = mpsc::channel();
    let mut runtime = Runtime {
        session: Arc::new(session),
        tx,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    event_loop(&mut terminal, &mut runtime, &rx)
}

struct ScreenGuard;

impl ScreenGuard {
    fn enter() -> io::Result<ScreenGuard> {
        enable_raw_mode()?;
        let guard = ScreenGuard;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(guard)
    }
}

impl Drop for ScreenGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    if let Err(error) = disable_raw_mode() {
        tracing::warn!(%error, "não foi possível sair do modo raw");
    }
    if let Err(error) = execute!(io::stdout(), LeaveAlternateScreen, Show) {
        tracing::warn!(%error, "não foi possível sair da tela alternativa");
    }
}

fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let in_worker = thread::current()
            .name()
            .is_some_and(|name| name.starts_with(WORKER_THREAD_PREFIX));
        if in_worker {
            tracing::error!(panic = %info, "pânico numa thread de trabalho");
            return;
        }
        restore_terminal();
        previous(info);
    }));
}

fn event_loop(
    terminal: &mut TuiTerminal,
    runtime: &mut Runtime,
    rx: &Receiver<Msg>,
) -> Result<(), TuiError> {
    let mut model = Model::new();
    model.disk = disk_usage(&runtime.session);
    let mut effects = begin_scan(&mut model);
    loop {
        for effect in effects.drain(..) {
            if !runtime.apply(effect) {
                return Ok(());
            }
        }
        terminal.draw(|frame| view::draw(frame, &model))?;
        for msg in next_messages(rx)? {
            let disk_changed = matches!(msg, Msg::ScanFinished | Msg::CleanFinished(_));
            let is_error = matches!(msg, Msg::Error(_));
            effects.extend(update(&mut model, msg));
            if disk_changed {
                model.disk = disk_usage(&runtime.session);
            }
            if is_error {
                terminal.clear()?;
            }
        }
    }
}

fn next_messages(rx: &Receiver<Msg>) -> io::Result<Vec<Msg>> {
    let mut messages = Vec::new();
    if event::poll(POLL_INTERVAL)?
        && let Event::Key(key) = event::read()?
        && key.kind != KeyEventKind::Release
    {
        messages.push(Msg::Key(key));
    }
    messages.extend(rx.try_iter());
    Ok(messages)
}

fn disk_usage(session: &Session) -> Option<(u64, u64)> {
    let home = &session.env.home;
    Some((free_bytes(home).ok()?, total_bytes(home).ok()?))
}

struct Runtime {
    session: Arc<Session>,
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Runtime {
    fn apply(&mut self, effect: Effect) -> bool {
        match effect {
            Effect::StartScan => self.start_scan(),
            Effect::StartClean(request) => self.start_clean(request),
            Effect::CancelClean => self.cancel.store(true, Ordering::SeqCst),
            Effect::Quit => return false,
        }
        true
    }

    fn start_scan(&self) {
        let session = Arc::new(self.session.at_now());
        if let Err(error) = worker::spawn_scan(session, self.tx.clone()) {
            self.notify(Msg::Error(format!(
                "não foi possível começar a medição: {error}"
            )));
            self.notify(Msg::ScanFinished);
        }
    }

    fn start_clean(&mut self, request: CleanRequest) {
        let session = Arc::new(self.session.at_now());
        let ledger = match Ledger::load_fresh(&session.env) {
            Ok(ledger) => ledger,
            Err(error) => {
                self.notify(Msg::Error(format!(
                    "A limpeza não começou: {error} Na interface, tecle m."
                )));
                return;
            }
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        if let Err(error) = worker::spawn_clean(ledger, request, session, self.tx.clone(), cancel) {
            self.notify(Msg::Error(format!("a limpeza não começou: {error}")));
        }
    }

    fn notify(&self, msg: Msg) {
        if self.tx.send(msg).is_err() {
            tracing::warn!("canal da interface fechado");
        }
    }
}
