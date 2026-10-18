//! Descoberta de repositorios e de pastas conhecidas dentro deles (S5, S16).

use std::fs;
use std::path::{Path, PathBuf};

use super::ScanContext;
use super::categories::ScanProblem;
use crate::system::config::Config;
use crate::system::env::Environment;
use crate::system::git::Git;

/// Ate quantos niveis abaixo de um repositorio se procura outro aninhado
/// (a mesma profundidade da busca por pastas conhecidas).
const NESTED_REPOSITORY_DEPTH: usize = 3;

const MIN_REPOSITORIES_PER_ROOT: usize = 2;
const AUTODETECT_DEPTH: usize = 2;
const SKIPPED_HOME_DIRS: [&str; 1] = ["snap"];
/// Pastas de dependencias baixadas: um `dist` ou `build` la dentro faz parte
/// do pacote, nao e saida de build do projeto.
const DEPENDENCY_DIRS: [&str; 5] = ["node_modules", "bower_components", "vendor", "venv", "Pods"];

/// `.git` existe, como pasta ou como arquivo (worktree e submodulo).
pub fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// A propria raiz, se for repositorio; senao os filhos diretos que sao, em ordem.
pub fn repositories_in(root: &Path) -> Vec<PathBuf> {
    if is_repository(root) {
        return vec![root.to_path_buf()];
    }
    child_dirs(root)
        .into_iter()
        .filter(|child| is_repository(child))
        .collect()
}

/// `repo` e os repositorios aninhados nele (clones e submodulos) ate
/// `max_depth` niveis abaixo de cada um, em ordem. Cada aninhado e varrido
/// como repositorio proprio, com o `.gitignore` e a atividade dele (S5, S16).
pub fn with_nested_repositories(repo: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut all = vec![repo.to_path_buf()];
    for nested in nested_repositories(repo, max_depth) {
        all.extend(with_nested_repositories(&nested, max_depth));
    }
    all
}

/// Repositorios dentro de `repo`, sem descer neles nem em pastas muradas.
fn nested_repositories(repo: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![repo.to_path_buf()];
    for _ in 0..max_depth {
        let mut next_level = Vec::new();
        for child in level.iter().flat_map(|dir| child_dirs(dir)) {
            if is_walled_by_name(&child) {
                continue;
            }
            if is_repository(&child) {
                found.push(child);
            } else {
                next_level.push(child);
            }
        }
        level = next_level;
    }
    found.sort();
    found
}

/// Busca em largura por pastas de `names` (que podem ter barra, como
/// `.angular/cache`) ate `max_depth` niveis abaixo de `repo`. Nao desce em
/// pastas de dependencias (`node_modules`, `bower_components`...), `.git`, pastas ocultas, symlinks, pasta ja encontrada nem
/// em repositorio aninhado (que e varrido como repositorio proprio).
pub fn find_named_dirs(repo: &Path, names: &[&str], max_depth: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![repo.to_path_buf()];
    for depth in 0..max_depth {
        let mut next_level = Vec::new();
        for dir in &level {
            let matches = matches_in(dir, names, max_depth - depth);
            next_level.extend(descendable_children(dir, &matches));
            found.extend(matches);
        }
        level = next_level;
    }
    found
}

fn matches_in(dir: &Path, names: &[&str], remaining_depth: usize) -> Vec<PathBuf> {
    names
        .iter()
        .filter(|name| name.split('/').count() <= remaining_depth)
        .map(|name| dir.join(name))
        .filter(|candidate| is_real_dir(candidate) && !is_repository(candidate))
        .collect()
}

fn descendable_children(dir: &Path, matches: &[PathBuf]) -> Vec<PathBuf> {
    child_dirs(dir)
        .into_iter()
        .filter(|child| {
            !matches
                .iter()
                .any(|found| found.starts_with(child) || found == child)
        })
        .filter(|child| !is_walled(child))
        .collect()
}

fn is_walled(dir: &Path) -> bool {
    is_walled_by_name(dir) || is_repository(dir)
}

fn is_walled_by_name(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_none_or(|name| name.starts_with('.') || DEPENDENCY_DIRS.contains(&name))
}

/// Subpastas diretas (sem seguir symlink), em ordem.
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    children.sort();
    children
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// Pastas ate 2 niveis abaixo da pessoal, sem ocultas, `snap` nem downloads.
/// Pastas que sao repositorios entram, mas nao se desce nelas.
pub fn home_folders(env: &Environment) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let mut level = vec![env.home.clone()];
    for _ in 0..AUTODETECT_DEPTH {
        let mut next_level = Vec::new();
        for dir in &level {
            let children: Vec<PathBuf> = child_dirs(dir)
                .into_iter()
                .filter(|child| !is_skipped_home_dir(child, env))
                .collect();
            next_level.extend(
                children
                    .iter()
                    .filter(|child| !is_repository(child))
                    .cloned(),
            );
            folders.extend(children);
        }
        level = next_level;
    }
    folders
}

fn is_skipped_home_dir(dir: &Path, env: &Environment) -> bool {
    let Some(name) = dir.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    name.starts_with('.')
        || SKIPPED_HOME_DIRS.contains(&name)
        || name.eq_ignore_ascii_case("downloads")
        || env.downloads_dir.as_deref() == Some(dir)
}

/// Raizes de projetos sem configuracao: pastas com pelo menos 2 repositorios
/// como filhos diretos.
pub fn autodetect_roots(env: &Environment) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for folder in home_folders(env) {
        let is_inside_root = roots.iter().any(|root| folder.starts_with(root));
        if !is_inside_root && !is_repository(&folder) && has_enough_repositories(&folder) {
            roots.push(folder);
        }
    }
    roots
}

fn has_enough_repositories(folder: &Path) -> bool {
    child_dirs(folder)
        .iter()
        .filter(|child| is_repository(child))
        .count()
        >= MIN_REPOSITORIES_PER_ROOT
}

/// As raizes configuradas, ou as detectadas quando a lista esta vazia.
pub fn effective_roots(config: &Config, env: &Environment) -> Vec<PathBuf> {
    if config.project_roots.is_empty() {
        autodetect_roots(env)
    } else {
        config.project_roots.clone()
    }
}

/// O `git` da varredura, ou `ToolMissing` quando nao foi encontrado no `PATH`.
pub(crate) fn git_for<'a>(ctx: &'a ScanContext) -> Result<Git<'a>, ScanProblem> {
    let exe = ctx
        .env
        .tools
        .git
        .as_deref()
        .ok_or(ScanProblem::ToolMissing("git"))?;
    Ok(Git::new(exe, ctx.runner).with_env(ctx.git_env.clone()))
}

pub(crate) fn require_roots(ctx: &ScanContext) -> Result<(), ScanProblem> {
    if ctx.project_roots.is_empty() {
        return Err(ScanProblem::NoProjects);
    }
    Ok(())
}

/// Repositorios de todas as raizes de projetos, com os aninhados (clones e
/// submodulos) como repositorios proprios, sem repetir.
pub(crate) fn all_repositories(ctx: &ScanContext) -> Vec<PathBuf> {
    let mut repositories: Vec<PathBuf> = Vec::new();
    for repo in ctx
        .project_roots
        .iter()
        .flat_map(|root| repositories_in(root))
        .flat_map(|repo| with_nested_repositories(&repo, NESTED_REPOSITORY_DEPTH))
    {
        if !repositories.contains(&repo) {
            repositories.push(repo);
        }
    }
    repositories
}

/// `repo` ou `repo (subpasta)`, onde a subpasta e a pasta `container` do item,
/// relativa ao repositorio.
pub(crate) fn repo_label(repo: &Path, container: &Path) -> String {
    let repo_name = repo
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let subfolder = container
        .strip_prefix(repo)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty());
    match subfolder {
        Some(relative) => format!("{repo_name} ({})", relative.display()),
        None => repo_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::config::Config;

    fn fake_repo(path: &Path) {
        fs::create_dir_all(path.join(".git")).unwrap();
    }

    fn make_dir(path: &Path) {
        fs::create_dir_all(path).unwrap();
    }

    fn sandbox() -> (tempfile::TempDir, Environment) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let env = Environment::for_tests(&home);
        (dir, env)
    }

    #[test]
    fn repository_can_be_folder_or_file() {
        let (_dir, env) = sandbox();
        let folder_repo = env.home.join("a");
        let file_repo = env.home.join("b");
        fake_repo(&folder_repo);
        make_dir(&file_repo);
        fs::write(file_repo.join(".git"), "gitdir: ../x\n").unwrap();
        assert!(is_repository(&folder_repo) && is_repository(&file_repo));
        assert!(!is_repository(&env.home));
    }

    #[test]
    fn repositories_in_root_or_children() {
        let (_dir, env) = sandbox();
        let root = env.home.join("field");
        fake_repo(&root.join("b"));
        fake_repo(&root.join("a"));
        make_dir(&root.join("c"));
        assert_eq!(repositories_in(&root), vec![root.join("a"), root.join("b")]);
        fake_repo(&root);
        assert_eq!(repositories_in(&root), vec![root.clone()]);
    }

    #[test]
    fn find_named_dirs_walls_nested_repositories() {
        let (_dir, env) = sandbox();
        let repo = env.home.join("repo");
        make_dir(&repo.join("dist"));
        fake_repo(&repo.join("vendor/lib"));
        make_dir(&repo.join("vendor/lib/dist"));
        make_dir(&repo.join("sub/dist"));
        fs::write(repo.join("sub/.git"), "gitdir: ../.git/modules/sub\n").unwrap();
        fake_repo(&repo.join("build"));
        let found = find_named_dirs(&repo, &["dist", "build"], 3);
        assert_eq!(found, vec![repo.join("dist")]);
    }

    #[test]
    fn with_nested_repositories_lists_inner_repos() {
        let (_dir, env) = sandbox();
        let repo = env.home.join("repo");
        fake_repo(&repo);
        fake_repo(&repo.join("third_party/lib"));
        fake_repo(&repo.join("third_party/lib/deep"));
        fake_repo(&repo.join("a/b/c"));
        fake_repo(&repo.join("a/b/c/d/e/f/g"));
        fake_repo(&repo.join("node_modules/pkg"));
        fake_repo(&repo.join(".hidden/r"));
        fake_repo(&repo.join("vendor/pkg"));
        let all = with_nested_repositories(&repo, 3);
        assert_eq!(
            all,
            vec![
                repo.clone(),
                repo.join("a/b/c"),
                repo.join("third_party/lib"),
                repo.join("third_party/lib/deep"),
            ]
        );
    }

    #[test]
    fn find_named_dirs_nested_and_walled() {
        let (_dir, env) = sandbox();
        let repo = env.home.join("repo");
        for relative in [
            "client-v2/node_modules",
            "node_modules/pkg/node_modules",
            ".hidden/node_modules",
            "client-v2/.angular/cache",
            "dist/inner/dist",
            "a/b/c/node_modules",
        ] {
            make_dir(&repo.join(relative));
        }
        let found = find_named_dirs(&repo, &["node_modules", ".angular/cache", "dist"], 3);
        assert_eq!(
            found,
            vec![
                repo.join("node_modules"),
                repo.join("dist"),
                repo.join("client-v2/node_modules"),
                repo.join("client-v2/.angular/cache"),
            ]
        );
        let shallow = find_named_dirs(&repo, &["node_modules"], 1);
        assert_eq!(shallow, vec![repo.join("node_modules")]);
    }

    #[test]
    fn find_named_dirs_does_not_follow_symlinks() {
        let (_dir, env) = sandbox();
        let repo = env.home.join("repo");
        make_dir(&repo.join("real/dist"));
        std::os::unix::fs::symlink(repo.join("real"), repo.join("link")).unwrap();
        let found = find_named_dirs(&repo, &["dist"], 3);
        assert_eq!(found, vec![repo.join("real/dist")]);
    }

    #[test]
    fn find_named_dirs_skips_dependency_folders() {
        let (_dir, env) = sandbox();
        let repo = env.home.join("repo");
        for dependencies in DEPENDENCY_DIRS {
            make_dir(&repo.join(dependencies).join("pacote/dist"));
        }
        make_dir(&repo.join("app/dist"));
        let found = find_named_dirs(&repo, &["dist"], 3);
        assert_eq!(found, vec![repo.join("app/dist")]);
    }

    #[test]
    fn autodetect_needs_two_repos() {
        let (_dir, mut env) = sandbox();
        fake_repo(&env.home.join("field/a"));
        fake_repo(&env.home.join("field/b"));
        fake_repo(&env.home.join("solo/a"));
        fake_repo(&env.home.join("trabalho/clientes/x"));
        fake_repo(&env.home.join("trabalho/clientes/y"));
        fake_repo(&env.home.join(".oculta/a"));
        fake_repo(&env.home.join(".oculta/b"));
        fake_repo(&env.home.join("snap/a"));
        fake_repo(&env.home.join("snap/b"));
        fake_repo(&env.home.join("Downloads/a"));
        fake_repo(&env.home.join("Downloads/b"));
        fake_repo(&env.home.join("fundo/a/b/c"));
        env.downloads_dir = Some(env.home.join("Baixados"));
        fake_repo(&env.home.join("Baixados/a"));
        fake_repo(&env.home.join("Baixados/b"));
        assert_eq!(
            autodetect_roots(&env),
            vec![env.home.join("field"), env.home.join("trabalho/clientes")]
        );
    }

    #[test]
    fn configured_roots_win() {
        let (_dir, env) = sandbox();
        fake_repo(&env.home.join("field/a"));
        fake_repo(&env.home.join("field/b"));
        let configured = Config {
            project_roots: vec![env.home.join("outra")],
            ..Config::default()
        };
        assert_eq!(
            effective_roots(&configured, &env),
            vec![env.home.join("outra")]
        );
        assert_eq!(
            effective_roots(&Config::default(), &env),
            vec![env.home.join("field")]
        );
    }
}
