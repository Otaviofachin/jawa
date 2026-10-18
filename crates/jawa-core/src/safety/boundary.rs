use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::system::env::Environment;

/// Subpastas de `config_dir` que podem ser limpas (S19).
pub const CONFIG_CACHE_ALLOWLIST: &[&str] = &[
    "Code/Cache",
    "Code/CachedData",
    "Code/CachedExtensionVSIXs",
    "Code/Code Cache",
    "Code/GPUCache",
    "Code/logs",
    "Slack/Cache",
    "Slack/Code Cache",
    "Slack/GPUCache",
    "Slack/logs",
    "discord/Cache",
    "discord/Code Cache",
    "discord/GPUCache",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BoundaryViolation {
    #[error("o caminho precisa ser absoluto")]
    Relative,
    #[error("o alvo é a própria raiz da categoria")]
    IsRoot,
    #[error("o caminho fica fora das pastas permitidas")]
    OutsideRoots,
    #[error("o caminho está protegido pela lista negra: {}", .0.display())]
    Denied(PathBuf),
    #[error("o alvo é um ponto de montagem")]
    IsMountPoint,
}

/// Valida o alvo e devolve `pai.canonicalize().join(nome)`; nunca segue o
/// proprio alvo, de modo que um symlink e tratado como o link em si.
pub fn check_target(
    path: &Path,
    roots: &[PathBuf],
    env: &Environment,
) -> Result<PathBuf, BoundaryViolation> {
    if !path.is_absolute() {
        return Err(BoundaryViolation::Relative);
    }
    let (resolved, parent) = resolve(path)?;
    if let Some(rule) = denied_reason(&resolved, env) {
        return Err(BoundaryViolation::Denied(rule));
    }
    inside_any_root(&resolved, roots)?;
    if is_mount_point(&resolved, &parent) {
        return Err(BoundaryViolation::IsMountPoint);
    }
    Ok(resolved)
}

fn resolve(path: &Path) -> Result<(PathBuf, PathBuf), BoundaryViolation> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(BoundaryViolation::OutsideRoots);
    };
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| BoundaryViolation::OutsideRoots)?;
    Ok((canonical_parent.join(name), canonical_parent))
}

fn canonical_or_raw(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

fn denied_reason(resolved: &Path, env: &Environment) -> Option<PathBuf> {
    let home = canonical_or_raw(env.home.clone());
    let protected_dirs = protected_subtrees(&home, env);
    let protected_exact = [PathBuf::from("/"), home.clone()];
    protected_dirs
        .iter()
        .find(|rule| resolved.starts_with(rule) || rule.starts_with(resolved))
        .or_else(|| {
            protected_exact
                .iter()
                .find(|rule| rule.starts_with(resolved))
        })
        .cloned()
        .or_else(|| git_component_prefix(resolved))
        .or_else(|| denied_config(resolved, &home, env))
}

/// Caminhos da S4 cuja subarvore inteira, e tambem qualquer ancestral, e proibida.
/// Vale o caminho literal sob a pasta pessoal e o resolvido pelo XDG.
fn protected_subtrees(home: &Path, env: &Environment) -> Vec<PathBuf> {
    let mut rules: Vec<PathBuf> = ["/etc", "/usr", "/var", "/opt", "/boot"]
        .iter()
        .map(PathBuf::from)
        .collect();
    rules.extend(
        [".ssh", ".gnupg", ".aws", ".kube", ".docker/config.json"]
            .iter()
            .map(|relative| home.join(relative)),
    );
    rules.push(home.join(".local/share/keyrings"));
    rules.push(canonical_or_raw(env.data_dir.clone()).join("keyrings"));
    rules.dedup();
    rules
}

fn git_component_prefix(resolved: &Path) -> Option<PathBuf> {
    let mut prefix = PathBuf::new();
    for component in resolved.components() {
        prefix.push(component);
        if component == Component::Normal(".git".as_ref()) {
            return Some(prefix);
        }
    }
    None
}

fn denied_config(resolved: &Path, home: &Path, env: &Environment) -> Option<PathBuf> {
    let config_dirs = [
        home.join(".config"),
        canonical_or_raw(env.config_dir.clone()),
    ];
    config_dirs.into_iter().find(|config_dir| {
        let is_ancestor = config_dir.starts_with(resolved);
        let is_inside_forbidden_part = resolved.starts_with(config_dir)
            && !CONFIG_CACHE_ALLOWLIST
                .iter()
                .any(|item| resolved.starts_with(config_dir.join(item)));
        is_ancestor || is_inside_forbidden_part
    })
}

fn inside_any_root(resolved: &Path, roots: &[PathBuf]) -> Result<(), BoundaryViolation> {
    let canonical_roots: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    if canonical_roots.iter().any(|root| root == resolved) {
        return Err(BoundaryViolation::IsRoot);
    }
    if canonical_roots
        .iter()
        .any(|root| resolved.starts_with(root))
    {
        return Ok(());
    }
    Err(BoundaryViolation::OutsideRoots)
}

/// Falha fechado: se o lstat falhar, o alvo e tratado como ponto de montagem.
fn is_mount_point(resolved: &Path, parent: &Path) -> bool {
    match (fs::symlink_metadata(resolved), fs::symlink_metadata(parent)) {
        (Ok(target), Ok(parent)) => target.dev() != parent.dev(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    struct Fixture {
        _dir: tempfile::TempDir,
        home: PathBuf,
        root: PathBuf,
        env: Environment,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let root = home.join("projects");
        fs::create_dir_all(&root).unwrap();
        let env = Environment::for_tests(&home);
        Fixture {
            _dir: dir,
            home,
            root,
            env,
        }
    }

    #[test]
    fn accepts_child_of_root() {
        let fx = fixture();
        fs::create_dir(fx.root.join("a")).unwrap();
        let checked = check_target(&fx.root.join("a"), std::slice::from_ref(&fx.root), &fx.env);
        assert_eq!(checked, Ok(fx.root.join("a")));
    }

    #[test]
    fn refuses_relative_path() {
        let fx = fixture();
        let checked = check_target(Path::new("a/b"), std::slice::from_ref(&fx.root), &fx.env);
        assert_eq!(checked, Err(BoundaryViolation::Relative));
    }

    #[test]
    fn refuses_root_itself() {
        let fx = fixture();
        let checked = check_target(&fx.root, std::slice::from_ref(&fx.root), &fx.env);
        assert_eq!(checked, Err(BoundaryViolation::IsRoot));
    }

    #[test]
    fn refuses_outside() {
        let fx = fixture();
        fs::create_dir(fx.home.join("other")).unwrap();
        let checked = check_target(
            &fx.home.join("other/x"),
            std::slice::from_ref(&fx.root),
            &fx.env,
        );
        assert_eq!(checked, Err(BoundaryViolation::OutsideRoots));
    }

    #[test]
    fn refuses_dot_dot_escape() {
        let fx = fixture();
        fs::create_dir(fx.root.join("sub")).unwrap();
        let sneaky = fx.root.join("sub/../../victim");
        let checked = check_target(&sneaky, std::slice::from_ref(&fx.root), &fx.env);
        assert_eq!(checked, Err(BoundaryViolation::OutsideRoots));
    }

    #[test]
    fn refuses_symlink_escape() {
        let fx = fixture();
        let outside = fx.home.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, fx.root.join("link")).unwrap();
        let checked = check_target(
            &fx.root.join("link/victim"),
            std::slice::from_ref(&fx.root),
            &fx.env,
        );
        assert_eq!(checked, Err(BoundaryViolation::OutsideRoots));
    }

    #[test]
    fn deletes_symlink_item_as_link() {
        let fx = fixture();
        let outside = fx.home.join("outside");
        fs::create_dir(&outside).unwrap();
        symlink(&outside, fx.root.join("link")).unwrap();
        let checked = check_target(
            &fx.root.join("link"),
            std::slice::from_ref(&fx.root),
            &fx.env,
        );
        assert_eq!(checked, Ok(fx.root.join("link")));
    }

    #[test]
    fn refuses_ssh_even_if_root_is_home() {
        let fx = fixture();
        fs::create_dir(fx.home.join(".ssh")).unwrap();
        let checked = check_target(
            &fx.home.join(".ssh/id"),
            std::slice::from_ref(&fx.home),
            &fx.env,
        );
        assert_eq!(
            checked,
            Err(BoundaryViolation::Denied(fx.home.join(".ssh")))
        );
    }

    #[test]
    fn refuses_git_component() {
        let fx = fixture();
        fs::create_dir_all(fx.root.join("repo/.git")).unwrap();
        let checked = check_target(
            &fx.root.join("repo/.git/objects"),
            std::slice::from_ref(&fx.root),
            &fx.env,
        );
        assert!(matches!(checked, Err(BoundaryViolation::Denied(_))));
    }

    #[test]
    fn refuses_config_except_allowlist() {
        let fx = fixture();
        let code_dir = fx.env.config_dir.join("Code");
        fs::create_dir_all(code_dir.join("User")).unwrap();
        fs::create_dir_all(code_dir.join("Cache")).unwrap();
        let roots = [code_dir.clone()];
        let user = check_target(&code_dir.join("User"), &roots, &fx.env);
        assert!(matches!(user, Err(BoundaryViolation::Denied(_))));
        let cache = check_target(&code_dir.join("Cache"), &roots, &fx.env);
        assert_eq!(cache, Ok(code_dir.join("Cache")));
    }

    #[test]
    fn refuses_mount_point() {
        let fx = fixture();
        let checked = check_target(Path::new("/proc"), &[PathBuf::from("/")], &fx.env);
        assert_eq!(checked, Err(BoundaryViolation::IsMountPoint));
    }

    #[test]
    fn refuses_ancestor_of_docker_config() {
        let fx = fixture();
        fs::create_dir_all(fx.home.join(".docker")).unwrap();
        let checked = check_target(
            &fx.home.join(".docker"),
            std::slice::from_ref(&fx.home),
            &fx.env,
        );
        assert_eq!(
            checked,
            Err(BoundaryViolation::Denied(
                fx.home.join(".docker/config.json")
            ))
        );
    }

    #[test]
    fn refuses_ancestor_of_keyrings() {
        let fx = fixture();
        fs::create_dir_all(fx.home.join(".local/share/keyrings")).unwrap();
        for target in [".local/share", ".local"] {
            let checked = check_target(
                &fx.home.join(target),
                std::slice::from_ref(&fx.home),
                &fx.env,
            );
            assert!(
                matches!(checked, Err(BoundaryViolation::Denied(_))),
                "{target}"
            );
        }
    }

    #[test]
    fn refuses_ancestor_of_config_dir() {
        let mut fx = fixture();
        fx.env.config_dir = fx.home.join("deep/cfg");
        fs::create_dir_all(&fx.env.config_dir).unwrap();
        let checked = check_target(
            &fx.home.join("deep"),
            std::slice::from_ref(&fx.home),
            &fx.env,
        );
        assert_eq!(
            checked,
            Err(BoundaryViolation::Denied(fx.home.join("deep/cfg")))
        );
    }

    #[test]
    fn refuses_config_payload_and_literal_paths_with_custom_xdg() {
        let mut fx = fixture();
        fx.env.config_dir = fx.home.join("xdgcfg");
        fx.env.data_dir = fx.home.join("xdgdata");
        fs::create_dir_all(fx.home.join(".config/App")).unwrap();
        fs::create_dir_all(fx.home.join("xdgcfg/Code/Cache")).unwrap();
        fs::create_dir_all(fx.home.join("xdgcfg/Other")).unwrap();
        fs::create_dir_all(fx.home.join(".local/share/keyrings/k")).unwrap();
        fs::create_dir_all(fx.home.join("xdgdata/keyrings/k")).unwrap();
        let roots = std::slice::from_ref(&fx.home);
        let literal = check_target(&fx.home.join(".config/App"), roots, &fx.env);
        assert_eq!(
            literal,
            Err(BoundaryViolation::Denied(fx.home.join(".config")))
        );
        let xdg = check_target(&fx.home.join("xdgcfg/Other"), roots, &fx.env);
        assert_eq!(xdg, Err(BoundaryViolation::Denied(fx.home.join("xdgcfg"))));
        let allowed = check_target(&fx.home.join("xdgcfg/Code/Cache"), roots, &fx.env);
        assert_eq!(allowed, Ok(fx.home.join("xdgcfg/Code/Cache")));
        for keyring in [".local/share/keyrings/k", "xdgdata/keyrings/k"] {
            let checked = check_target(&fx.home.join(keyring), roots, &fx.env);
            assert!(
                matches!(checked, Err(BoundaryViolation::Denied(_))),
                "{keyring}"
            );
        }
    }

    #[test]
    fn refuses_exact_protected_paths() {
        let fx = fixture();
        for relative in [
            ".aws",
            ".kube",
            ".docker/config.json",
            ".local/share/keyrings",
        ] {
            let target = fx.home.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, "x").unwrap();
        }
        let roots = std::slice::from_ref(&fx.home);
        let targets = [
            PathBuf::from("/"),
            PathBuf::from("/etc"),
            fx.home.clone(),
            fx.home.join(".aws"),
            fx.home.join(".kube"),
            fx.home.join(".docker/config.json"),
            fx.home.join(".local/share/keyrings"),
        ];
        for target in targets {
            let checked = check_target(&target, roots, &fx.env);
            assert!(
                matches!(
                    checked,
                    Err(BoundaryViolation::Denied(_)) | Err(BoundaryViolation::OutsideRoots)
                ),
                "{target:?}"
            );
        }
    }
}
