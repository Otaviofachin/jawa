use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use crate::clean::{CleanReport, CleanRequest, ExecContext, Progress, SystemTrash, clean};
use crate::error::CoreError;
use crate::model::finding::CategoryId;
use crate::model::finding::Finding;
use crate::safety::audit::{AuditEntry, AuditLog};
use crate::safety::ledger::Ledger;
use crate::scan::categories::category;
use crate::scan::projects::effective_roots;
use crate::scan::{CategoryScan, READ_TIMEOUT, ScanContext, SystemPaths, scan_all};
use crate::system::config::{Config, config_file};
use crate::system::env::Environment;
use crate::system::runner::{CommandRunner, SystemRunner};

#[derive(Debug, Clone)]
pub struct Session {
    pub env: Environment,
    pub config: Config,
    pub system: SystemPaths,
}

impl Session {
    pub fn open() -> Result<Session, CoreError> {
        let env = Environment::detect()?;
        let config = Config::load(&config_file(&env))?.validate()?;
        Ok(Session {
            env,
            config,
            system: system_paths(),
        })
    }

    pub fn at_now(&self) -> Session {
        let mut session = self.clone();
        session.env.now = SystemTime::now();
        session
    }

    pub fn scan_context<'a>(&'a self, runner: &'a dyn CommandRunner) -> ScanContext<'a> {
        ScanContext {
            env: &self.env,
            config: &self.config,
            project_roots: effective_roots(&self.config, &self.env),
            runner,
            read_timeout: READ_TIMEOUT,
            system: self.system.clone(),
            git_env: Vec::new(),
        }
    }

    pub fn measure(
        &self,
        only: Option<CategoryId>,
        on_done: &dyn Fn(&CategoryScan),
    ) -> (Ledger, Vec<CategoryScan>) {
        let runner = SystemRunner;
        scan_all(&self.scan_context(&runner), only, on_done)
    }

    pub fn clean(
        &self,
        ledger: &mut Ledger,
        request: &CleanRequest,
        on_progress: &dyn Fn(&Progress),
        cancel: &AtomicBool,
    ) -> Result<CleanReport, CoreError> {
        let audit = AuditLog::open(&self.env)?;
        let runner = SystemRunner;
        let scan_context = self.scan_context(&runner);
        let roots_for = |finding: &Finding| category(finding.category).roots(&scan_context);
        let watched_names_for =
            |finding: &Finding| category(finding.category).watched_processes_for(finding);
        let in_use_scope_for = |finding: &Finding| category(finding.category).in_use_scope(finding);
        let exec_context = ExecContext {
            env: &self.env,
            runner: &runner,
            trash: &SystemTrash,
            audit: &audit,
            roots_for: &roots_for,
            watched_names_for: &watched_names_for,
            in_use_scope_for: &in_use_scope_for,
        };
        clean(ledger, request, &exec_context, on_progress, cancel)
    }

    pub fn history(&self, last: usize) -> Result<Vec<AuditEntry>, CoreError> {
        Ok(AuditLog::open(&self.env)?.recent(last))
    }
}

#[cfg(debug_assertions)]
fn system_paths() -> SystemPaths {
    match std::env::var_os("JAWA_SYSTEM_ROOT") {
        Some(root) if !root.is_empty() => SystemPaths::under(std::path::Path::new(&root)),
        _ => SystemPaths::default(),
    }
}

#[cfg(not(debug_assertions))]
fn system_paths() -> SystemPaths {
    SystemPaths::default()
}
