use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::model::finding::FindingId;
use crate::system::env::Environment;

/// Uma linha do historico (S12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub at_secs: u64,
    pub id: FindingId,
    pub category: String,
    pub location: String,
    pub bytes: u64,
    pub outcome: String,
}

/// Historico em JSONL em `state_dir/jawa/historico.jsonl`.
#[derive(Debug, Clone)]
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Abre o arquivo em modo append ja aqui: sem permissao de escrita, a
    /// falha aparece antes de qualquer limpeza (S12).
    pub fn open(env: &Environment) -> Result<AuditLog, CoreError> {
        let directory = env.state_dir.join("jawa");
        fs::create_dir_all(&directory)?;
        let path = directory.join("historico.jsonl");
        open_for_append(&path)?;
        Ok(AuditLog { path })
    }

    /// Acrescenta uma linha com uma unica escrita, para nao intercalar linhas.
    pub fn record(&self, entry: &AuditEntry) -> Result<(), CoreError> {
        let mut line = serde_json::to_vec(entry).map_err(io::Error::from)?;
        line.push(b'\n');
        open_for_append(&self.path)?.write_all(&line)?;
        Ok(())
    }

    /// As ultimas `limit` entradas, da mais antiga para a mais recente.
    /// Linhas ilegiveis sao puladas.
    pub fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(path = ?self.path, %error, "não foi possível ler o histórico");
                }
                return Vec::new();
            }
        };
        let entries: Vec<AuditEntry> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        let skip = entries.len().saturating_sub(limit);
        entries.into_iter().skip(skip).collect()
    }
}

fn open_for_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(outcome: &str) -> AuditEntry {
        AuditEntry {
            at_secs: 1_700_000_000,
            id: "abcdef0123".parse().unwrap(),
            category: "npm-cache".to_string(),
            location: "~/.npm/_cacache".to_string(),
            bytes: 2_000_000,
            outcome: outcome.to_string(),
        }
    }

    #[test]
    fn records_and_reads_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::for_tests(dir.path());
        let log = AuditLog::open(&env).unwrap();
        for outcome in ["um", "dois", "tres"] {
            log.record(&entry(outcome)).unwrap();
        }
        let text = fs::read_to_string(env.state_dir.join("jawa/historico.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 3);
        let outcomes: Vec<String> = log.recent(2).into_iter().map(|e| e.outcome).collect();
        assert_eq!(outcomes, vec!["dois", "tres"]);
        assert_eq!(log.recent(10).len(), 3);
    }

    #[test]
    fn audit_open_fails_when_file_not_writable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::for_tests(dir.path());
        let history = env.state_dir.join("jawa/historico.jsonl");
        fs::create_dir_all(history.parent().unwrap()).unwrap();
        fs::write(&history, "").unwrap();
        fs::set_permissions(&history, fs::Permissions::from_mode(0o444)).unwrap();
        let result = AuditLog::open(&env);
        fs::set_permissions(&history, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(result, Err(CoreError::Io(_))));
    }

    #[test]
    fn open_creates_history_file() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::for_tests(dir.path());
        AuditLog::open(&env).unwrap();
        assert!(env.state_dir.join("jawa/historico.jsonl").exists());
    }

    #[test]
    fn recent_is_empty_without_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(&Environment::for_tests(dir.path())).unwrap();
        assert!(log.recent(5).is_empty());
    }

    #[test]
    fn recent_skips_broken_lines() {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::for_tests(dir.path());
        let log = AuditLog::open(&env).unwrap();
        log.record(&entry("um")).unwrap();
        let path = env.state_dir.join("jawa/historico.jsonl");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("{quebrado\n");
        fs::write(&path, text).unwrap();
        log.record(&entry("dois")).unwrap();
        let outcomes: Vec<String> = log.recent(5).into_iter().map(|e| e.outcome).collect();
        assert_eq!(outcomes, vec!["um", "dois"]);
    }
}
