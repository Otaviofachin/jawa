use crate::scan::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;
use crate::system::config::Config;
use crate::system::env::Environment;
use crate::system::runner::{CommandRunner, FakeRunner};

pub(crate) struct Fixture {
    _dir: tempfile::TempDir,
    pub home: PathBuf,
    pub env: Environment,
    pub config: Config,
    pub runner: FakeRunner,
}

pub(crate) fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    let env = Environment::for_tests(&home);
    fs::create_dir_all(&env.proc_root).unwrap();
    Fixture {
        _dir: dir,
        home,
        env,
        config: Config::default(),
        runner: FakeRunner::new(),
    }
}

impl Fixture {
    pub(crate) fn ctx(&self) -> ScanContext<'_> {
        self.ctx_with(&self.runner, Vec::new())
    }

    pub(crate) fn ctx_with<'a>(
        &'a self,
        runner: &'a dyn CommandRunner,
        project_roots: Vec<PathBuf>,
    ) -> ScanContext<'a> {
        ScanContext {
            env: &self.env,
            config: &self.config,
            project_roots,
            runner,
            read_timeout: Duration::from_millis(200),
            system: SystemPaths {
                docker_desktop_install: self.home.join("opt/docker-desktop"),
                snaps_dir: self.home.join("var/snaps"),
                apt_archives: self.home.join("var/apt"),
                crash_dir: self.home.join("var/crash"),
            },
            git_env: crate::system::git::test_repo::isolated_env(),
        }
    }
}

pub(crate) fn write_bytes(path: &Path, size: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![7u8; size]).unwrap();
}
