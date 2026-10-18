//! Leitura do git: caminhos ignorados (S5) e ultima atividade de um repositorio (S16).

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::model::finding::CommandSpec;
use crate::scan::READ_TIMEOUT;
use crate::system::runner::{CommandOutput, CommandRunner, RunError};

pub struct Git<'a> {
    exe: &'a Path,
    runner: &'a dyn CommandRunner,
    env: Vec<(String, String)>,
}

impl<'a> Git<'a> {
    pub fn new(exe: &'a Path, runner: &'a dyn CommandRunner) -> Git<'a> {
        Git {
            exe,
            runner,
            env: Vec::new(),
        }
    }

    /// Variaveis extras para cada chamada (usadas para isolar os testes da
    /// configuracao global do git).
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Git<'a> {
        self.env = env;
        self
    }

    /// Quais `candidates` o git ignora, numa unica chamada (`check-ignore
    /// --stdin -z`). Nenhum ignorado (codigo 1) e um conjunto vazio, nao erro.
    pub fn ignored_among(
        &self,
        repo: &Path,
        candidates: &[PathBuf],
    ) -> Result<HashSet<PathBuf>, RunError> {
        if candidates.is_empty() {
            return Ok(HashSet::new());
        }
        let mut input = Vec::new();
        for candidate in candidates {
            input.extend_from_slice(candidate.as_os_str().as_bytes());
            input.push(0);
        }
        let output = self.run(repo, &["check-ignore", "--stdin", "-z"], Some(input))?;
        let nothing_ignored = output.code == Some(1) && output.stdout.is_empty();
        if !output.success && !nothing_ignored {
            return Err(RunError::Io(failure_text(&output)));
        }
        Ok(output
            .stdout
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(|name| PathBuf::from(OsString::from_vec(name.as_bytes().to_vec())))
            .collect())
    }

    /// O mais recente entre o ultimo commit do `HEAD`, o `logs/HEAD` (checkout,
    /// pull) e o `index` (add, status). `None` se nada pode ser lido.
    pub fn last_activity(&self, repo: &Path) -> Option<SystemTime> {
        let commit = self.last_commit_time(repo);
        let git_dir = self.git_dir(repo);
        let touched = git_dir.iter().flat_map(|dir| {
            ["logs/HEAD", "index"]
                .into_iter()
                .filter_map(|name| modified_time(&dir.join(name)))
        });
        commit.into_iter().chain(touched).max()
    }

    fn last_commit_time(&self, repo: &Path) -> Option<SystemTime> {
        let output = self
            .run(repo, &["log", "-1", "--format=%ct"], None)
            .ok()
            .filter(|output| output.success)?;
        let seconds = output.stdout.trim().parse::<u64>().ok()?;
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds))
    }

    fn git_dir(&self, repo: &Path) -> Option<PathBuf> {
        let output = self
            .run(repo, &["rev-parse", "--absolute-git-dir"], None)
            .ok()
            .filter(|output| output.success)?;
        let path = output.stdout.trim_end_matches('\n');
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    fn run(
        &self,
        repo: &Path,
        args: &[&str],
        stdin: Option<Vec<u8>>,
    ) -> Result<CommandOutput, RunError> {
        let mut full_args = vec!["-C".to_string(), repo.display().to_string()];
        full_args.extend(args.iter().map(|arg| arg.to_string()));
        self.runner.run(&CommandSpec {
            program: self.exe.to_path_buf(),
            args: full_args,
            stdin,
            env: self.env.clone(),
            timeout: READ_TIMEOUT,
        })
    }
}

fn failure_text(output: &CommandOutput) -> String {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        format!("git terminou com código {:?}", output.code)
    } else {
        stderr.to_string()
    }
}

fn modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
pub(crate) mod test_repo {
    use std::fs;
    use std::fs::OpenOptions;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, SystemTime};

    const DAY: u64 = 86_400;

    pub(crate) fn git_exe() -> PathBuf {
        PathBuf::from("git")
    }

    /// Isola o git dos testes da configuracao global e do sistema do dev.
    pub(crate) fn isolated_env() -> Vec<(String, String)> {
        vec![
            ("GIT_CONFIG_GLOBAL".to_string(), "/dev/null".to_string()),
            ("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string()),
        ]
    }

    pub(crate) fn git(dir: &Path, args: &[&str], days_ago: u64) -> String {
        let seconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - days_ago * DAY;
        let date = format!("{seconds} +0000");
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Teste")
            .env("GIT_AUTHOR_EMAIL", "teste@example.com")
            .env("GIT_COMMITTER_NAME", "Teste")
            .env("GIT_COMMITTER_EMAIL", "teste@example.com")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Repositorio novo com um commit de `days_ago` dias atras, e o `index` e o
    /// `logs/HEAD` envelhecidos para a mesma data.
    pub(crate) fn init_repo(repo: &Path, days_ago: u64) {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "-q", "-b", "main"], 0);
        fs::write(repo.join("README.md"), "oi\n").unwrap();
        git(repo, &["add", "-A"], days_ago);
        git(repo, &["commit", "-q", "-m", "inicio"], days_ago);
        age_activity(repo, days_ago);
    }

    pub(crate) fn commit_all(repo: &Path, days_ago: u64) {
        git(repo, &["add", "-A"], days_ago);
        git(repo, &["commit", "-q", "-m", "mais"], days_ago);
        age_activity(repo, days_ago);
    }

    pub(crate) fn age_activity(repo: &Path, days_ago: u64) {
        let git_dir = PathBuf::from(git(repo, &["rev-parse", "--absolute-git-dir"], 0).trim());
        for name in ["logs/HEAD", "index"] {
            set_age(&git_dir.join(name), days_ago);
        }
    }

    pub(crate) fn set_age(path: &Path, days_ago: u64) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(days_ago * DAY))
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_repo::{git, git_exe, init_repo, isolated_env, set_age};
    use super::*;
    use crate::system::runner::SystemRunner;

    const DAY: u64 = 86_400;

    fn age_of(activity: SystemTime) -> Duration {
        SystemTime::now().duration_since(activity).unwrap()
    }

    fn repo_with_ignore(dir: &Path) -> PathBuf {
        let repo = dir.join("repo");
        init_repo(&repo, 0);
        fs::write(repo.join(".gitignore"), "dist/\nsaida*\n").unwrap();
        repo
    }

    #[test]
    fn ignored_among_batches() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_ignore(dir.path());
        let (dist, build, other) = (repo.join("dist"), repo.join("build"), repo.join("src"));
        for folder in [&dist, &build, &other] {
            fs::create_dir_all(folder).unwrap();
        }
        let exe = git_exe();
        let git = Git::new(&exe, &SystemRunner).with_env(isolated_env());
        let ignored = git
            .ignored_among(&repo, &[dist.clone(), build, other])
            .unwrap();
        assert_eq!(ignored, HashSet::from([dist]));
    }

    #[test]
    fn ignored_among_handles_newline_and_space_names() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_ignore(dir.path());
        let spaced = repo.join("saida com espaco");
        let broken = repo.join("saida\nquebrada");
        let versioned = repo.join("limpa\nversionada");
        for folder in [&spaced, &broken, &versioned] {
            fs::create_dir_all(folder).unwrap();
        }
        let exe = git_exe();
        let git = Git::new(&exe, &SystemRunner).with_env(isolated_env());
        let ignored = git
            .ignored_among(&repo, &[spaced.clone(), broken.clone(), versioned])
            .unwrap();
        assert_eq!(ignored, HashSet::from([spaced, broken]));
    }

    #[test]
    fn ignored_among_empty_when_none_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repo_with_ignore(dir.path());
        let visible = repo.join("src");
        fs::create_dir_all(&visible).unwrap();
        let exe = git_exe();
        let git = Git::new(&exe, &SystemRunner).with_env(isolated_env());
        assert!(git.ignored_among(&repo, &[visible]).unwrap().is_empty());
    }

    #[test]
    fn ignored_among_reports_a_broken_repo() {
        let dir = tempfile::tempdir().unwrap();
        let exe = git_exe();
        let git = Git::new(&exe, &SystemRunner).with_env(isolated_env());
        let result = git.ignored_among(dir.path(), &[dir.path().join("dist")]);
        assert!(matches!(result, Err(RunError::Io(_))));
    }

    #[test]
    fn activity_uses_commit_date() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        init_repo(&repo, 40);
        let exe = git_exe();
        let activity = Git::new(&exe, &SystemRunner)
            .with_env(isolated_env())
            .last_activity(&repo)
            .unwrap();
        let age = age_of(activity);
        assert!(age > Duration::from_secs(39 * DAY) && age < Duration::from_secs(41 * DAY));
    }

    #[test]
    fn activity_counts_recent_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        init_repo(&repo, 90);
        set_age(&repo.join(".git/logs/HEAD"), 0);
        let exe = git_exe();
        let activity = Git::new(&exe, &SystemRunner)
            .with_env(isolated_env())
            .last_activity(&repo)
            .unwrap();
        assert!(age_of(activity) < Duration::from_secs(DAY));
    }

    #[test]
    fn activity_works_in_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        init_repo(&repo, 90);
        let worktree = dir.path().join("worktree");
        git(
            &repo,
            &["worktree", "add", "-q", worktree.to_str().unwrap()],
            90,
        );
        assert!(worktree.join(".git").is_file());
        let git_dir = PathBuf::from(git(&worktree, &["rev-parse", "--absolute-git-dir"], 0).trim());
        for name in ["logs/HEAD", "index"] {
            set_age(&git_dir.join(name), 90);
        }
        let exe = git_exe();
        let git = Git::new(&exe, &SystemRunner).with_env(isolated_env());
        assert!(age_of(git.last_activity(&worktree).unwrap()) > Duration::from_secs(80 * DAY));
        set_age(&git_dir.join("index"), 0);
        assert!(age_of(git.last_activity(&worktree).unwrap()) < Duration::from_secs(DAY));
    }

    #[test]
    fn activity_is_none_outside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        let exe = git_exe();
        assert!(
            Git::new(&exe, &SystemRunner)
                .with_env(isolated_env())
                .last_activity(dir.path())
                .is_none()
        );
    }
}
