use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::system::env::Environment;

const MIN_IDLE_DAYS: u32 = 1;
const MAX_IDLE_DAYS: u32 = 365;
const DEFAULT_IDLE_DAYS: u32 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub project_roots: Vec<PathBuf>,
    pub idle_days: u32,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            project_roots: Vec::new(),
            idle_days: DEFAULT_IDLE_DAYS,
        }
    }
}

pub fn config_file(env: &Environment) -> PathBuf {
    env.config_dir.join("jawa").join("config.toml")
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, CoreError> {
        let invalid =
            |detail: String| CoreError::InvalidConfig(format!("{}: {detail}", path.display()));
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(error) => return Err(invalid(format!("não foi possível ler ({error})"))),
        };
        toml::from_str(&text).map_err(|error| invalid(error.message().to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        let text = toml::to_string_pretty(self)
            .map_err(|error| CoreError::InvalidConfig(error.to_string()))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary_path = path.with_extension("toml.tmp");
        fs::write(&temporary_path, text)?;
        fs::rename(&temporary_path, path)?;
        Ok(())
    }

    pub fn validate(self) -> Result<Config, CoreError> {
        if !(MIN_IDLE_DAYS..=MAX_IDLE_DAYS).contains(&self.idle_days) {
            return Err(CoreError::InvalidConfig(format!(
                "idle_days deve ficar entre {MIN_IDLE_DAYS} e {MAX_IDLE_DAYS}"
            )));
        }
        let mut project_roots: Vec<PathBuf> = Vec::new();
        for root in self.project_roots {
            check_root(&root)?;
            if !project_roots.contains(&root) {
                project_roots.push(root);
            }
        }
        Ok(Config {
            project_roots,
            idle_days: self.idle_days,
        })
    }
}

fn check_root(root: &Path) -> Result<(), CoreError> {
    if !root.is_absolute() {
        return Err(CoreError::InvalidConfig(format!(
            "a raiz {} precisa ser um caminho absoluto",
            root.display()
        )));
    }
    if !root.is_dir() {
        return Err(CoreError::InvalidConfig(format!(
            "a raiz {} não existe",
            root.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(roots: Vec<PathBuf>, idle_days: u32) -> Config {
        Config {
            project_roots: roots,
            idle_days,
        }
    }

    #[test]
    fn missing_config_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::load(&dir.path().join("nope.toml")).unwrap(),
            Config::default()
        );
    }

    fn load_error(text: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, text).unwrap();
        let error = Config::load(&path).unwrap_err();
        assert!(matches!(error, CoreError::InvalidConfig(_)), "{error:?}");
        let message = error.to_string();
        assert!(message.contains(&path.display().to_string()), "{message}");
        message
    }

    #[test]
    fn broken_config_is_an_error() {
        load_error("idle_days = [[[");
        load_error("idle_days = \"trinta\"");
    }

    #[test]
    fn unknown_key_is_an_error() {
        let message = load_error("idle_day = 10\n");
        assert!(message.contains("idle_day"), "{message}");
    }

    #[test]
    fn unreadable_config_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            Config::load(&path),
            Err(CoreError::InvalidConfig(_))
        ));
    }

    #[test]
    fn config_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jawa/config.toml");
        let config = config_with(vec![PathBuf::from("/home/eu/field")], 45);
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        assert!(!path.with_extension("toml.tmp").exists());
    }

    #[test]
    fn validate_rejects_zero_and_366_days() {
        assert!(config_with(vec![], 0).validate().is_err());
        assert!(config_with(vec![], 366).validate().is_err());
        assert!(config_with(vec![], 1).validate().is_ok());
        assert!(config_with(vec![], 365).validate().is_ok());
    }

    #[test]
    fn validate_rejects_relative_and_missing_roots() {
        let relative = config_with(vec![PathBuf::from("relative/dir")], 30);
        assert!(relative.validate().is_err());
        let missing = config_with(vec![PathBuf::from("/definitely/not/here")], 30);
        assert!(missing.validate().is_err());
    }

    #[test]
    fn validate_drops_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let validated = config_with(vec![root.clone(), root.clone()], 30)
            .validate()
            .unwrap();
        assert_eq!(validated.project_roots, vec![root]);
    }
}
