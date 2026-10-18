#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use jawa_core::Environment;
use jawa_core::model::finding::{
    Action, CategoryId, Finding, FindingId, Reversibility, Risk, fingerprint_of,
};
use jawa_core::safety::ledger::{InstanceLock, Ledger};

const TWO_MIB: usize = 2 * 1024 * 1024;
const MISSING_ID: &str = "0123456789";

struct Sandbox {
    _dir: tempfile::TempDir,
    home: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        fs::create_dir_all(home.join("bin")).unwrap();
        Sandbox { _dir: dir, home }
    }

    fn env(&self) -> Environment {
        Environment::for_tests(&self.home)
    }

    fn seed_npm_cache(&self) -> PathBuf {
        let cache = self.home.join(".npm/_cacache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("blob"), vec![7u8; TWO_MIB]).unwrap();
        cache
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_jawa"))
            .args(args)
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CACHE_HOME", self.home.join(".cache"))
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("XDG_DATA_HOME", self.home.join(".local/share"))
            .env("XDG_STATE_HOME", self.home.join(".local/state"))
            .env("NVM_DIR", self.home.join(".nvm"))
            .env("PATH", self.home.join("bin"))
            .env("JAWA_SYSTEM_ROOT", self.home.join("sistema"))
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn measured_ids(&self) -> Vec<String> {
        let output = self.run(&["medir", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        report["itens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect()
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn history_file(home: &Path) -> PathBuf {
    home.join(".local/state/jawa/historico.jsonl")
}

#[test]
fn measure_lists_npm_cache_and_saves_ledger() {
    let sandbox = Sandbox::new();
    sandbox.seed_npm_cache();
    let output = sandbox.run(&["medir"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("npm-cache"), "{text}");
    assert!(text.contains("~/.npm/_cacache"), "{text}");
    assert!(text.contains("Seguro"), "{text}");
    assert!(text.contains("MB"), "{text}");
    assert!(
        sandbox
            .home
            .join(".local/state/jawa/ultima-medicao.json")
            .exists()
    );
}

#[test]
fn measure_json_lists_items_and_problems() {
    let sandbox = Sandbox::new();
    sandbox.seed_npm_cache();
    assert_eq!(sandbox.measured_ids().len(), 1);
}

#[test]
fn system_paths_follow_the_test_root_in_debug_builds() {
    let sandbox = Sandbox::new();
    let crash = sandbox.home.join("sistema/var/crash");
    fs::create_dir_all(&crash).unwrap();
    fs::write(crash.join("_usr_bin_app.1000.crash"), vec![7u8; TWO_MIB]).unwrap();
    let output = sandbox.run(&["medir", "--categoria", "sistema"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("sudo rm -f /var/crash/*.crash"), "{text}");
    assert!(text.contains(&crash.display().to_string()), "{text}");
}

#[test]
fn invalid_config_stops_with_the_file_path() {
    let sandbox = Sandbox::new();
    sandbox.seed_npm_cache();
    let config = sandbox.home.join(".config/jawa/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "idle_dias = 10\n").unwrap();
    for args in [vec!["medir"], vec![]] {
        let output = sandbox.run(&args);
        assert_eq!(code(&output), 1, "{}", stdout(&output));
        let message = stderr(&output);
        assert!(message.contains("configuração inválida"), "{message}");
        assert!(message.contains(&config.display().to_string()), "{message}");
    }
}

#[test]
fn help_is_in_portuguese() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["limpar", "--help"],
        vec!["medir", "-h"],
        vec!["explicar", "--help"],
        vec!["historico", "--help"],
    ] {
        let output = sandbox.run(&args);
        assert_eq!(code(&output), 0, "{args:?}: {}", stderr(&output));
        let text = stdout(&output);
        for english in [
            "Usage",
            "Print help",
            "Print version",
            "Options",
            "Commands",
            "Arguments",
            "[OPTIONS]",
        ] {
            assert!(!text.contains(english), "{args:?} tem '{english}':\n{text}");
        }
        assert!(text.contains("Uso:"), "{args:?}:\n{text}");
        assert!(text.contains("Mostra esta ajuda"), "{args:?}:\n{text}");
    }
    let version = sandbox.run(&["--version"]);
    assert_eq!(code(&version), 0);
    assert!(stdout(&version).contains(env!("CARGO_PKG_VERSION")));
    let root_help = stdout(&sandbox.run(&["--help"]));
    assert!(root_help.contains("Mostra a versão"), "{root_help}");
    assert!(!root_help.contains("\n  help "), "{root_help}");
}

#[test]
fn root_help_shows_only_the_interface() {
    let sandbox = Sandbox::new();
    let root_help = stdout(&sandbox.run(&["--help"]));
    assert!(root_help.contains("abre a interface"), "{root_help}");
    for hidden in ["medir", "explicar", "limpar", "historico", "Comandos"] {
        assert!(
            !root_help.contains(hidden),
            "--help mostra '{hidden}':\n{root_help}"
        );
    }
    let still_works = sandbox.run(&["historico"]);
    assert_eq!(code(&still_works), 0, "{}", stderr(&still_works));
}

#[test]
fn measure_accepts_category_filter_and_rejects_unknown() {
    let sandbox = Sandbox::new();
    sandbox.seed_npm_cache();
    let filtered = sandbox.run(&["medir", "--categoria", "npm-cache"]);
    assert_eq!(code(&filtered), 0);
    let unknown = sandbox.run(&["medir", "--categoria", "nada"]);
    assert_eq!(code(&unknown), 1);
    assert!(stderr(&unknown).contains("categoria desconhecida"));
}

#[test]
fn explain_prints_six_sections() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["explicar", "npm-cache"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    for heading in [
        "O que é",
        "Por que cresce",
        "O que acontece ao limpar",
        "Dá para desfazer?",
        "Custo para recriar",
        "Recomendação",
    ] {
        assert!(text.contains(heading), "faltou {heading}: {text}");
    }
}

#[test]
fn clean_without_measure_fails_with_hint() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["limpar", MISSING_ID, "--sim"]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("jawa medir"));
}

#[test]
fn clean_unknown_id_exits_1_and_deletes_nothing() {
    let sandbox = Sandbox::new();
    let cache = sandbox.seed_npm_cache();
    sandbox.measured_ids();
    let output = sandbox.run(&["limpar", MISSING_ID, "--sim"]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains(MISSING_ID));
    assert!(cache.exists());
}

#[test]
fn clean_malformed_id_exits_1() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["limpar", "xyz", "--sim"]);
    assert_eq!(code(&output), 1);
}

#[test]
fn clean_simulate_changes_nothing() {
    let sandbox = Sandbox::new();
    let cache = sandbox.seed_npm_cache();
    let ids = sandbox.measured_ids();
    let output = sandbox.run(&["limpar", &ids[0], "--simular"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stdout(&output).contains("~/.npm/_cacache"));
    assert!(cache.exists());
    let history = fs::read_to_string(history_file(&sandbox.home)).unwrap_or_default();
    assert!(history.is_empty());
}

#[test]
fn clean_safe_flag_adds_every_safe_item() {
    let sandbox = Sandbox::new();
    let cache = sandbox.seed_npm_cache();
    sandbox.measured_ids();
    let output = sandbox.run(&["limpar", "--seguros", "--sim"]);
    assert_eq!(code(&output), 0, "{}{}", stdout(&output), stderr(&output));
    assert!(!cache.exists());
}

#[test]
fn clean_with_yes_deletes_and_records_history() {
    let sandbox = Sandbox::new();
    let cache = sandbox.seed_npm_cache();
    let ids = sandbox.measured_ids();
    let output = sandbox.run(&["limpar", &ids[0], "--sim"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!cache.exists());
    let history = fs::read_to_string(history_file(&sandbox.home)).unwrap();
    assert!(history.contains(&ids[0]));
    let listing = sandbox.run(&["historico"]);
    assert_eq!(code(&listing), 0);
    assert!(stdout(&listing).contains(&ids[0]));
    assert!(stdout(&listing).contains("feito"));
}

#[test]
fn clean_refuses_non_tty_without_yes() {
    let sandbox = Sandbox::new();
    let cache = sandbox.seed_npm_cache();
    let ids = sandbox.measured_ids();
    let output = sandbox.run(&["limpar", &ids[0]]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("--sim"));
    assert!(cache.exists());
}

fn save_irreversible_finding(sandbox: &Sandbox, target: &Path) -> String {
    fs::create_dir_all(target).unwrap();
    let action = Action::Delete(target.to_path_buf());
    let finding = Finding {
        id: FindingId::of(CategoryId::NpmCache, &action),
        category: CategoryId::NpmCache,
        label: "dados importantes".to_string(),
        location: "~/dados".to_string(),
        bytes: TWO_MIB as u64,
        risk: Risk::Review,
        reversibility: Reversibility::Irreversible,
        note: None,
        fingerprint: fingerprint_of(target).ok(),
        blocked: None,
        action,
    };
    let id = finding.id.to_string();
    let env = sandbox.env();
    Ledger::new(&env, vec![finding]).save(&env).unwrap();
    id
}

#[test]
fn irreversible_needs_flag() {
    let sandbox = Sandbox::new();
    let outside_roots = sandbox.home.join("dados");
    let id = save_irreversible_finding(&sandbox, &outside_roots);
    let output = sandbox.run(&["limpar", &id, "--sim"]);
    assert_eq!(code(&output), 1, "{}", stdout(&output));
    assert!(outside_roots.exists());
    let message = stderr(&output);
    assert!(message.contains("irreversível"), "{message}");
    assert!(message.contains("--confirmar-irreversivel"), "{message}");
    assert!(!history_file(&sandbox.home).exists());
    let simulated = sandbox.run(&["limpar", &id, "--simular"]);
    assert_eq!(code(&simulated), 0, "{}", stderr(&simulated));
    assert!(stdout(&simulated).contains("dados importantes"));
}

#[test]
fn irreversible_runs_with_flag_inside_roots() {
    let sandbox = Sandbox::new();
    let inside_roots = sandbox.home.join(".npm/dados");
    let id = save_irreversible_finding(&sandbox, &inside_roots);
    let refused = sandbox.run(&["limpar", &id, "--sim"]);
    assert_eq!(code(&refused), 1, "{}", stdout(&refused));
    assert!(inside_roots.exists());
    let output = sandbox.run(&["limpar", &id, "--sim", "--confirmar-irreversivel"]);
    assert_eq!(code(&output), 0, "{}{}", stdout(&output), stderr(&output));
    assert!(!inside_roots.exists());
}

#[test]
fn second_instance_is_refused() {
    let sandbox = Sandbox::new();
    let env = sandbox.env();
    let _held = InstanceLock::acquire(&env).unwrap();
    let cleaning = sandbox.run(&["limpar", MISSING_ID, "--sim"]);
    assert_eq!(code(&cleaning), 1);
    assert!(stderr(&cleaning).contains("outra execução do jawa está em andamento"));
    let measuring = sandbox.run(&["medir"]);
    assert_eq!(code(&measuring), 1);
}

#[test]
fn history_without_entries_says_so() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["historico", "--ultimos", "5"]);
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("Nenhuma ação"));
}

#[test]
fn no_subcommand_without_terminal_explains_and_exits_1() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&[]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("a interface precisa de um terminal interativo"));
}

#[test]
fn no_subcommand_while_busy_exits_1() {
    let sandbox = Sandbox::new();
    let env = sandbox.env();
    let _held = InstanceLock::acquire(&env).unwrap();
    let output = sandbox.run(&[]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("outra execução do jawa está em andamento"));
}
