use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs4::TryLockError;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::CoreError;
use crate::model::finding::{Finding, FindingId};
use crate::system::env::Environment;

pub const LEDGER_FORMAT: u32 = 1;
pub const LEDGER_MAX_AGE: Duration = Duration::from_secs(30 * 60);

/// Registro da ultima medicao: a limpeza so aceita ids daqui (S2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub format: u32,
    pub created_secs: u64,
    #[serde(with = "crate::model::stored_path")]
    pub home: PathBuf,
    pub uid: u32,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LedgerError {
    #[error("não há medição anterior; rode `jawa medir` de novo.")]
    Missing,
    #[error("a última medição tem {minutes} minutos (o limite é 30); rode `jawa medir` de novo.")]
    Stale { minutes: u64 },
    #[error(
        "a última medição é de outro usuário ou de outra pasta pessoal; rode `jawa medir` de novo."
    )]
    OtherUser,
    #[error("a última medição foi gravada por outra versão do jawa; rode `jawa medir` de novo.")]
    FormatChanged,
    #[error("a última medição está ilegível ({0}); rode `jawa medir` de novo.")]
    Corrupt(String),
}

/// So o campo `format`, lido antes do resto para separar versao antiga de
/// arquivo corrompido.
#[derive(Deserialize)]
struct FormatProbe {
    format: u32,
}

impl Ledger {
    pub fn new(env: &Environment, findings: Vec<Finding>) -> Ledger {
        Ledger {
            format: LEDGER_FORMAT,
            created_secs: unix_secs(env.now),
            home: env.home.clone(),
            uid: env.uid,
            findings,
        }
    }

    /// Grava em arquivo temporario ao lado do destino, sincroniza e renomeia.
    pub fn save(&self, env: &Environment) -> Result<(), CoreError> {
        let path = ledger_file(env);
        fs::create_dir_all(state_dir(env))?;
        let temporary_path = path.with_extension("json.tmp");
        let mut file = File::create(&temporary_path)?;
        file.write_all(&serde_json::to_vec(self).map_err(io::Error::from)?)?;
        file.sync_all()?;
        fs::rename(&temporary_path, &path)?;
        Ok(())
    }

    /// Le o ledger e recusa um que seja de outra versao, de outro usuario,
    /// de outra pasta pessoal ou mais velho que `LEDGER_MAX_AGE` (S2).
    pub fn load_fresh(env: &Environment) -> Result<Ledger, LedgerError> {
        let text = match fs::read_to_string(ledger_file(env)) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(LedgerError::Missing);
            }
            Err(error) => return Err(LedgerError::Corrupt(error.to_string())),
        };
        let probe: FormatProbe = parse(&text)?;
        if probe.format != LEDGER_FORMAT {
            return Err(LedgerError::FormatChanged);
        }
        let ledger: Ledger = parse(&text)?;
        ledger.check_owner(env)?;
        ledger.check_age(env)?;
        Ok(ledger)
    }

    pub fn find(&self, id: &FindingId) -> Option<&Finding> {
        self.findings.iter().find(|finding| &finding.id == id)
    }

    pub fn forget(&mut self, ids: &[FindingId]) {
        self.findings.retain(|finding| !ids.contains(&finding.id));
    }

    fn check_owner(&self, env: &Environment) -> Result<(), LedgerError> {
        if self.uid != env.uid || self.home != env.home {
            return Err(LedgerError::OtherUser);
        }
        Ok(())
    }

    fn check_age(&self, env: &Environment) -> Result<(), LedgerError> {
        let now_secs = unix_secs(env.now);
        let Some(age_secs) = now_secs.checked_sub(self.created_secs) else {
            return Err(LedgerError::Corrupt(
                "a data da medição está no futuro".to_string(),
            ));
        };
        if age_secs > LEDGER_MAX_AGE.as_secs() {
            return Err(LedgerError::Stale {
                minutes: age_secs / 60,
            });
        }
        Ok(())
    }
}

pub fn ledger_file(env: &Environment) -> PathBuf {
    state_dir(env).join("ultima-medicao.json")
}

fn state_dir(env: &Environment) -> PathBuf {
    env.state_dir.join("jawa")
}

fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, LedgerError> {
    serde_json::from_str(text).map_err(|error| LedgerError::Corrupt(error.to_string()))
}

fn unix_secs(moment: SystemTime) -> u64 {
    moment
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Lock exclusivo de instancia (S13), liberado no `Drop`.
#[derive(Debug)]
pub struct InstanceLock {
    file: File,
}

impl InstanceLock {
    /// Tenta o lock sem esperar: se outra execucao o tem, devolve `Busy`.
    pub fn acquire(env: &Environment) -> Result<InstanceLock, CoreError> {
        fs::create_dir_all(state_dir(env))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(state_dir(env).join("jawa.lock"))?;
        match fs4::FileExt::try_lock(&file) {
            Ok(()) => Ok(InstanceLock { file }),
            Err(TryLockError::WouldBlock) => Err(CoreError::Busy),
            Err(TryLockError::Error(error)) => Err(CoreError::Io(error)),
        }
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Err(error) = fs4::FileExt::unlock(&self.file) {
            tracing::warn!(%error, "não foi possível liberar o lock de instância");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::finding::{Action, CategoryId, Reversibility, Risk};
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    const CATEGORY: CategoryId = CategoryId::NpmCache;

    struct Fixture {
        _dir: tempfile::TempDir,
        env: Environment,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let env = Environment::for_tests(dir.path());
        Fixture { _dir: dir, env }
    }

    fn finding_for(path: &Path) -> Finding {
        let action = Action::Delete(path.to_path_buf());
        Finding {
            id: FindingId::of(CATEGORY, &action),
            category: CATEGORY,
            label: "item".to_string(),
            location: path.display().to_string(),
            bytes: 2_000_000,
            risk: Risk::Safe,
            reversibility: Reversibility::Regenerable,
            note: None,
            action,
            fingerprint: None,
            blocked: None,
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn ledger_path(env: &Environment) -> PathBuf {
        env.state_dir.join("jawa").join("ultima-medicao.json")
    }

    #[test]
    fn new_stamps_current_time() {
        let fx = fixture();
        let ledger = Ledger::new(&fx.env, Vec::new());
        assert!(ledger.created_secs.abs_diff(now_secs()) < 5);
        assert_eq!(ledger.format, LEDGER_FORMAT);
    }

    #[test]
    fn round_trips_non_utf8_path() {
        let fx = fixture();
        let odd = Path::new(OsStr::from_bytes(b"/x/caf\xe9"));
        let spaced = Path::new("/x/com espaco\ncom quebra");
        let ledger = Ledger::new(&fx.env, vec![finding_for(odd), finding_for(spaced)]);
        ledger.save(&fx.env).unwrap();
        assert!(ledger_path(&fx.env).exists());
        let loaded = Ledger::load_fresh(&fx.env).unwrap();
        assert_eq!(loaded, ledger);
        assert_eq!(loaded.findings[0].action.path(), Some(odd));
        assert_eq!(loaded.findings[1].action.path(), Some(spaced));
    }

    #[test]
    fn save_leaves_no_temporary_file() {
        let fx = fixture();
        Ledger::new(&fx.env, Vec::new()).save(&fx.env).unwrap();
        let names: Vec<_> = fs::read_dir(fx.env.state_dir.join("jawa"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("ultima-medicao.json")]);
    }

    #[test]
    fn load_refuses_stale() {
        let fx = fixture();
        let mut ledger = Ledger::new(&fx.env, Vec::new());
        ledger.created_secs = now_secs() - 31 * 60;
        ledger.save(&fx.env).unwrap();
        assert_eq!(
            Ledger::load_fresh(&fx.env),
            Err(LedgerError::Stale { minutes: 31 })
        );
    }

    #[test]
    fn load_accepts_recent() {
        let fx = fixture();
        let mut ledger = Ledger::new(&fx.env, Vec::new());
        ledger.created_secs = now_secs() - 29 * 60;
        ledger.save(&fx.env).unwrap();
        assert!(Ledger::load_fresh(&fx.env).is_ok());
    }

    #[test]
    fn load_refuses_future_timestamp() {
        let fx = fixture();
        let mut ledger = Ledger::new(&fx.env, Vec::new());
        ledger.created_secs = now_secs() + 3600;
        ledger.save(&fx.env).unwrap();
        assert!(matches!(
            Ledger::load_fresh(&fx.env),
            Err(LedgerError::Corrupt(_))
        ));
    }

    #[test]
    fn load_refuses_other_uid() {
        let fx = fixture();
        let mut ledger = Ledger::new(&fx.env, Vec::new());
        ledger.uid += 1;
        ledger.save(&fx.env).unwrap();
        assert_eq!(Ledger::load_fresh(&fx.env), Err(LedgerError::OtherUser));
    }

    #[test]
    fn load_refuses_other_home() {
        let fx = fixture();
        let mut ledger = Ledger::new(&fx.env, Vec::new());
        ledger.home = PathBuf::from("/home/outra-pessoa");
        ledger.save(&fx.env).unwrap();
        assert_eq!(Ledger::load_fresh(&fx.env), Err(LedgerError::OtherUser));
    }

    #[test]
    fn load_refuses_other_format() {
        let fx = fixture();
        let path = ledger_path(&fx.env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"format": 2, "outro": "formato"}"#).unwrap();
        assert_eq!(Ledger::load_fresh(&fx.env), Err(LedgerError::FormatChanged));
    }

    #[test]
    fn load_reports_corrupt() {
        let fx = fixture();
        let path = ledger_path(&fx.env);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{nao e json").unwrap();
        assert!(matches!(
            Ledger::load_fresh(&fx.env),
            Err(LedgerError::Corrupt(_))
        ));
    }

    #[test]
    fn load_reports_missing() {
        let fx = fixture();
        assert_eq!(Ledger::load_fresh(&fx.env), Err(LedgerError::Missing));
    }

    #[test]
    fn errors_tell_to_measure_again() {
        let errors = [
            LedgerError::Missing,
            LedgerError::Stale { minutes: 31 },
            LedgerError::OtherUser,
            LedgerError::FormatChanged,
            LedgerError::Corrupt("x".to_string()),
        ];
        for error in errors {
            assert!(
                error.to_string().ends_with("rode `jawa medir` de novo."),
                "{error}"
            );
        }
    }

    #[test]
    fn find_and_forget_removes_ids() {
        let fx = fixture();
        let first = finding_for(Path::new("/x/a"));
        let second = finding_for(Path::new("/x/b"));
        let mut ledger = Ledger::new(&fx.env, vec![first.clone(), second.clone()]);
        assert_eq!(ledger.find(&first.id), Some(&first));
        ledger.forget(std::slice::from_ref(&first.id));
        assert_eq!(ledger.find(&first.id), None);
        assert_eq!(ledger.findings, vec![second]);
    }

    #[test]
    fn second_lock_is_busy() {
        let fx = fixture();
        let _held = InstanceLock::acquire(&fx.env).unwrap();
        assert!(matches!(
            InstanceLock::acquire(&fx.env),
            Err(CoreError::Busy)
        ));
    }

    #[test]
    fn lock_released_on_drop() {
        let fx = fixture();
        let held = InstanceLock::acquire(&fx.env).unwrap();
        drop(held);
        assert!(InstanceLock::acquire(&fx.env).is_ok());
        assert!(fx.env.state_dir.join("jawa").join("jawa.lock").exists());
    }

    #[test]
    fn busy_message_is_portuguese() {
        assert_eq!(
            CoreError::Busy.to_string(),
            "outra execução do jawa está em andamento"
        );
    }
}
