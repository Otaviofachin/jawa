use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::CoreError;

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub git: Option<PathBuf>,
    pub docker: Option<PathBuf>,
    pub snap: Option<PathBuf>,
    pub journalctl: Option<PathBuf>,
    pub apt: Option<PathBuf>,
}

impl Tools {
    fn detect() -> Tools {
        Tools {
            git: which::which("git").ok(),
            docker: which::which("docker").ok(),
            snap: which::which("snap").ok(),
            journalctl: which::which("journalctl").ok(),
            apt: which::which("apt").ok(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Environment {
    pub home: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub downloads_dir: Option<PathBuf>,
    pub nvm_dir: PathBuf,
    pub uid: u32,
    pub tools: Tools,
    pub proc_root: PathBuf,
    pub now: SystemTime,
}

impl Environment {
    pub fn detect() -> Result<Environment, CoreError> {
        let euid = rustix::process::geteuid().as_raw();
        detect_with(euid, |name| std::env::var_os(name))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(home: &Path) -> Environment {
        Environment {
            home: home.to_path_buf(),
            cache_dir: home.join(".cache"),
            config_dir: home.join(".config"),
            data_dir: home.join(".local/share"),
            state_dir: home.join(".local/state"),
            downloads_dir: None,
            nvm_dir: home.join(".nvm"),
            uid: rustix::process::geteuid().as_raw(),
            tools: Tools::default(),
            proc_root: home.join("proc"),
            now: SystemTime::now(),
        }
    }
}

fn detect_with(
    euid: u32,
    lookup: impl Fn(&str) -> Option<OsString>,
) -> Result<Environment, CoreError> {
    if euid == 0 {
        return Err(CoreError::RunningAsRoot);
    }
    let home = lookup("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .ok_or(CoreError::MissingHome)?;
    check_home(&home, euid)?;
    Ok(Environment {
        cache_dir: dirs::cache_dir().unwrap_or_else(|| home.join(".cache")),
        config_dir: dirs::config_dir().unwrap_or_else(|| home.join(".config")),
        data_dir: dirs::data_dir().unwrap_or_else(|| home.join(".local/share")),
        state_dir: dirs::state_dir().unwrap_or_else(|| home.join(".local/state")),
        downloads_dir: dirs::download_dir(),
        nvm_dir: nvm_dir_from(&home, lookup),
        uid: euid,
        tools: Tools::detect(),
        proc_root: PathBuf::from("/proc"),
        now: SystemTime::now(),
        home,
    })
}

fn check_home(home: &Path, euid: u32) -> Result<(), CoreError> {
    let canonical = home.canonicalize().map_err(|_| CoreError::MissingHome)?;
    if canonical == Path::new("/") {
        return Err(CoreError::HomeIsRoot);
    }
    let owner = std::fs::metadata(&canonical)
        .map_err(|_| CoreError::MissingHome)?
        .uid();
    if owner != euid {
        return Err(CoreError::HomeNotOwned(home.to_path_buf()));
    }
    Ok(())
}

fn nvm_dir_from(home: &Path, lookup: impl Fn(&str) -> Option<OsString>) -> PathBuf {
    lookup("NVM_DIR")
        .filter(|value| !value.is_empty())
        .map_or_else(|| home.join(".nvm"), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_refuses_root() {
        let result = detect_with(0, |_| None);
        assert!(matches!(result, Err(CoreError::RunningAsRoot)));
    }

    fn home_lookup(home: &Path) -> impl Fn(&str) -> Option<OsString> {
        let home = home.as_os_str().to_os_string();
        move |name| (name == "HOME").then(|| home.clone())
    }

    #[test]
    fn detect_refuses_root_as_home() {
        let euid = rustix::process::geteuid().as_raw();
        let result = detect_with(euid, home_lookup(Path::new("/")));
        assert!(matches!(result, Err(CoreError::HomeIsRoot)), "{result:?}");
    }

    #[test]
    fn detect_refuses_home_of_another_user() {
        let dir = tempfile::tempdir().unwrap();
        let euid = rustix::process::geteuid().as_raw();
        let own = detect_with(euid, home_lookup(dir.path())).unwrap();
        assert_eq!(own.home, dir.path());
        let result = detect_with(euid + 1, home_lookup(dir.path()));
        assert!(
            matches!(result, Err(CoreError::HomeNotOwned(_))),
            "{result:?}"
        );
        assert!(result.unwrap_err().to_string().contains("não pertence"));
    }

    #[test]
    fn nvm_dir_honours_env() {
        let home = Path::new("/home/eu");
        let with_var = nvm_dir_from(home, |_| Some(OsString::from("/x")));
        assert_eq!(with_var, PathBuf::from("/x"));
        let without_var = nvm_dir_from(home, |_| None);
        assert_eq!(without_var, PathBuf::from("/home/eu/.nvm"));
    }

    #[test]
    fn for_tests_keeps_everything_inside_home() {
        let env = Environment::for_tests(Path::new("/h"));
        assert_eq!(env.config_dir, PathBuf::from("/h/.config"));
        assert_eq!(env.proc_root, PathBuf::from("/h/proc"));
        assert!(env.tools.git.is_none());
    }
}
