//! Threads de medicao e limpeza. Falam com a interface so por mensagens, e a
//! interface nunca espera por elas.

use std::io;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use jawa_core::clean::{CleanRequest, Progress};
use jawa_core::safety::ledger::Ledger;
use jawa_core::scan::CategoryScan;

use super::model::Msg;
use jawa_core::api::Session;

/// Prefixo do nome das threads de trabalho (o gancho de panico o usa).
pub const WORKER_THREAD_PREFIX: &str = "jawa-worker";
const UNEXPECTED_END: &str = "a tarefa em segundo plano parou de forma inesperada; veja o jawa.log";

/// Mede tudo em segundo plano, manda cada categoria assim que ela termina e
/// grava o ledger no fim (S1: so leitura).
pub fn spawn_scan(session: Arc<Session>, tx: Sender<Msg>) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("{WORKER_THREAD_PREFIX}-medicao"))
        .spawn(move || {
            let mut notice = EndNotice::new(tx.clone(), WorkerKind::Scan);
            send(&tx, Msg::ScanStarted);
            let on_done = |scan: &CategoryScan| send(&tx, Msg::CategoryScanned(scan.clone()));
            let (ledger, _) = session.measure(None, &on_done);
            if let Err(error) = ledger.save(&session.env) {
                send(
                    &tx,
                    Msg::Error(format!("não foi possível gravar a medição: {error}")),
                );
            }
            send(&tx, Msg::ScanFinished);
            notice.finished = true;
        })
}

/// Limpa em segundo plano com todas as travas do executor. `cancel` pula os
/// itens que ainda nao comecaram.
pub fn spawn_clean(
    ledger: Ledger,
    request: CleanRequest,
    session: Arc<Session>,
    tx: Sender<Msg>,
    cancel: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("{WORKER_THREAD_PREFIX}-limpeza"))
        .spawn(move || {
            let mut notice = EndNotice::new(tx.clone(), WorkerKind::Clean);
            let mut ledger = ledger;
            let on_progress = |progress: &Progress| send(&tx, Msg::CleanProgress(progress.clone()));
            let outcome = session.clean(&mut ledger, &request, &on_progress, &cancel);
            let msg = match outcome {
                Ok(report) => Msg::CleanFinished(report),
                Err(error) => Msg::Error(format!("a limpeza não começou: {error}")),
            };
            send(&tx, msg);
            notice.finished = true;
        })
}

/// Com a interface fechada, o canal some e a mensagem nao tem para onde ir.
fn send(tx: &Sender<Msg>, msg: Msg) {
    if tx.send(msg).is_err() {
        tracing::debug!("interface fechada; mensagem do worker descartada");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerKind {
    Scan,
    Clean,
}

/// Avisa a interface se a thread terminar sem a mensagem final (panico), para
/// ela nao ficar presa em "medindo" ou "limpando".
struct EndNotice {
    tx: Sender<Msg>,
    kind: WorkerKind,
    finished: bool,
}

impl EndNotice {
    fn new(tx: Sender<Msg>, kind: WorkerKind) -> EndNotice {
        EndNotice {
            tx,
            kind,
            finished: false,
        }
    }
}

impl Drop for EndNotice {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        tracing::error!(kind = ?self.kind, "worker terminou sem a mensagem final");
        send(&self.tx, Msg::Error(UNEXPECTED_END.to_string()));
        if self.kind == WorkerKind::Scan {
            send(&self.tx, Msg::ScanFinished);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jawa_core::clean::ItemOutcome;
    use jawa_core::model::finding::{CategoryId, FindingId};
    use jawa_core::scan::SystemPaths;
    use jawa_core::system::config::Config;
    use jawa_core::system::env::Environment;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    const TWO_MIB: usize = 2 * 1024 * 1024;
    const WAIT: Duration = Duration::from_secs(120);

    struct Sandbox {
        _dir: tempfile::TempDir,
        session: Arc<Session>,
    }

    impl Sandbox {
        fn new() -> Sandbox {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path().to_path_buf();
            let session = Session {
                env: Environment::for_tests(&home),
                config: Config::default(),
                system: SystemPaths::under(&home.join("sistema")),
            };
            Sandbox {
                _dir: dir,
                session: Arc::new(session),
            }
        }

        fn seed_npm_cache(&self) -> PathBuf {
            let cache = self.session.env.home.join(".npm/_cacache");
            fs::create_dir_all(&cache).unwrap();
            fs::write(cache.join("blob"), vec![7u8; TWO_MIB]).unwrap();
            cache
        }

        fn scan(&self) -> Vec<Msg> {
            let (tx, rx) = mpsc::channel();
            spawn_scan(Arc::clone(&self.session), tx).unwrap();
            collect_until(&rx, |msg| matches!(msg, Msg::ScanFinished))
        }
    }

    fn collect_until(rx: &Receiver<Msg>, last: impl Fn(&Msg) -> bool) -> Vec<Msg> {
        let mut received = Vec::new();
        loop {
            let msg = rx.recv_timeout(WAIT).expect("o worker parou sem terminar");
            let done = last(&msg);
            received.push(msg);
            if done {
                return received;
            }
        }
    }

    #[test]
    fn scan_worker_reports_each_category_and_saves_ledger() {
        let sandbox = Sandbox::new();
        sandbox.seed_npm_cache();
        let received = sandbox.scan();
        assert!(matches!(received.first(), Some(Msg::ScanStarted)));
        let scanned: Vec<CategoryId> = received
            .iter()
            .filter_map(|msg| match msg {
                Msg::CategoryScanned(scan) => Some(scan.id),
                _ => None,
            })
            .collect();
        assert_eq!(scanned.len(), CategoryId::ALL.len());
        let ledger = Ledger::load_fresh(&sandbox.session.env).unwrap();
        assert!(
            ledger
                .findings
                .iter()
                .any(|finding| finding.category == CategoryId::NpmCache)
        );
    }

    #[test]
    fn clean_worker_reports_progress_and_deletes() {
        let sandbox = Sandbox::new();
        let cache = sandbox.seed_npm_cache();
        sandbox.scan();
        let ledger = Ledger::load_fresh(&sandbox.session.env).unwrap();
        let ids: Vec<FindingId> = ledger
            .findings
            .iter()
            .filter(|finding| finding.category == CategoryId::NpmCache)
            .map(|finding| finding.id.clone())
            .collect();
        let request = CleanRequest {
            ids,
            allow_irreversible: false,
        };
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        spawn_clean(ledger, request, Arc::clone(&sandbox.session), tx, cancel).unwrap();
        let received = collect_until(&rx, |msg| matches!(msg, Msg::CleanFinished(_)));
        assert!(
            received
                .iter()
                .any(|msg| matches!(msg, Msg::CleanProgress(_)))
        );
        let Some(Msg::CleanFinished(report)) = received.last() else {
            panic!("sem relatório");
        };
        assert!(
            report
                .items
                .iter()
                .all(|(_, outcome)| matches!(outcome, ItemOutcome::Done { .. }))
        );
        assert!(!cache.exists());
    }

    #[test]
    fn clean_worker_turns_unknown_id_into_error() {
        let sandbox = Sandbox::new();
        let ledger = Ledger::new(&sandbox.session.env, Vec::new());
        let request = CleanRequest {
            ids: vec!["0123456789".parse().unwrap()],
            allow_irreversible: false,
        };
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        spawn_clean(ledger, request, Arc::clone(&sandbox.session), tx, cancel).unwrap();
        let received = collect_until(&rx, |msg| matches!(msg, Msg::Error(_)));
        let Some(Msg::Error(text)) = received.last() else {
            panic!("sem erro");
        };
        assert!(text.contains("0123456789"), "{text}");
    }
}
