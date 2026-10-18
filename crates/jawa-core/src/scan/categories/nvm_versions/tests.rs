use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;
use crate::scan::categories::test_support::{Fixture, fixture, write_bytes};

const TWO_MIB: usize = 2 * 1024 * 1024;
const VERSIONS: [&str; 7] = [
    "v14.21.3", "v16.20.2", "v18.19.0", "v18.20.4", "v20.11.1", "v21.7.3", "v22.1.0",
];

fn install(fx: &Fixture, names: &[&str]) -> Vec<NodeVersion> {
    let base = fx.env.nvm_dir.join("versions/node");
    let mut installed = Vec::new();
    for name in names {
        let dir = base.join(name);
        write_bytes(&dir.join("lib/node_modules/npm/blob"), TWO_MIB);
        let numbers = parse_numbers(name);
        installed.push(NodeVersion { numbers, dir });
    }
    installed.sort();
    installed
}

fn parse_numbers(name: &str) -> [u32; 3] {
    let parts: Vec<u32> = name
        .trim_start_matches('v')
        .split('.')
        .map(|part| part.parse().unwrap())
        .collect();
    [parts[0], parts[1], parts[2]]
}

fn alias(fx: &Fixture, name: &str, target: &str) {
    let path = fx.env.nvm_dir.join("alias").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("{target}\n")).unwrap();
}

fn resolved(request: &str, installed: &[NodeVersion], aliases: &Path) -> Option<[u32; 3]> {
    resolve(request, installed, aliases).map(|version| version.numbers)
}

fn project_with_nvmrc(fx: &Fixture, relative: &str, content: &str) {
    let repo = fx.home.join("field/app");
    fs::create_dir_all(repo.join(".git")).unwrap();
    let path = repo.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn resolves_like_nvm() {
    let fx = fixture();
    let installed = install(&fx, &VERSIONS);
    alias(&fx, "default", "lts/iron");
    alias(&fx, "trabalho", "default");
    alias(&fx, "loop", "loop");
    let aliases = fx.env.nvm_dir.join("alias");
    let cases: [(&str, Option<[u32; 3]>); 21] = [
        ("node", Some([22, 1, 0])),
        ("stable", Some([22, 1, 0])),
        ("lts/*", Some([22, 1, 0])),
        ("lts/iron", Some([20, 11, 1])),
        ("lts/hydrogen", Some([18, 20, 4])),
        ("lts/gallium", Some([16, 20, 2])),
        ("lts/fermium", Some([14, 21, 3])),
        ("lts/boron", None),
        ("v18", Some([18, 20, 4])),
        ("18", Some([18, 20, 4])),
        ("18.19", Some([18, 19, 0])),
        ("18.19.0", Some([18, 19, 0])),
        ("v20.11.1", Some([20, 11, 1])),
        ("18.2", None),
        ("19", None),
        ("default", Some([20, 11, 1])),
        ("trabalho", Some([20, 11, 1])),
        ("loop", None),
        ("inexistente", None),
        ("../../etc/passwd", None),
        ("", None),
    ];
    for (request, expected) in cases {
        assert_eq!(
            resolved(request, &installed, &aliases),
            expected,
            "{request}"
        );
    }
    let without_even_latest = install(&fixture(), &["v20.11.1", "v21.7.3"]);
    assert_eq!(
        resolved("lts/*", &without_even_latest, &aliases),
        Some([20, 11, 1])
    );
}

#[test]
fn lts_reads_nvm_alias_files_first() {
    let fx = fixture();
    let installed = install(&fx, &["v22.11.0", "v24.2.0", "v26.0.0"]);
    let aliases = fx.env.nvm_dir.join("alias");
    assert_eq!(resolved("lts/*", &installed, &aliases), Some([26, 0, 0]));
    alias(&fx, "lts/*", "lts/krypton");
    alias(&fx, "lts/krypton", "v24.2.0");
    alias(&fx, "lts/jod", "v22.11.0");
    assert_eq!(resolved("lts/*", &installed, &aliases), Some([24, 2, 0]));
    assert_eq!(
        resolved("lts/krypton", &installed, &aliases),
        Some([24, 2, 0])
    );
    assert_eq!(resolved("lts/jod", &installed, &aliases), Some([22, 11, 0]));
    alias(&fx, "lts/argon", "v4.9.1");
    assert_eq!(resolved("lts/argon", &installed, &aliases), None);
}

#[test]
fn default_lts_alias_protects_the_nvm_lts() {
    let fx = fixture();
    install(&fx, &["v24.2.0", "v26.0.0"]);
    alias(&fx, "default", "lts/*");
    alias(&fx, "lts/*", "lts/krypton");
    alias(&fx, "lts/krypton", "v24.2.0");
    let output = scan_with(&fx.ctx(), None).unwrap();
    let labels: Vec<&str> = output.findings.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(labels, vec!["Node v26.0.0"]);
}

#[test]
fn protects_default_nvmrc_and_active() {
    let fx = fixture();
    let installed = install(&fx, &VERSIONS);
    alias(&fx, "default", "lts/iron");
    project_with_nvmrc(&fx, "web/.nvmrc", "v18\n");
    let active = installed
        .iter()
        .find(|version| version.numbers == [22, 1, 0])
        .unwrap();
    let node = active.dir.join("bin/node");
    write_bytes(&node, 16);
    fs::set_permissions(&node, fs::Permissions::from_mode(0o755)).unwrap();
    let shim_dir = fx.home.join("shims");
    fs::create_dir_all(&shim_dir).unwrap();
    symlink(&node, shim_dir.join("node")).unwrap();
    let ctx = fx.ctx_with(
        &crate::system::runner::SystemRunner,
        vec![fx.home.join("field")],
    );
    let path_var = Some(OsString::from(shim_dir));
    assert_eq!(
        protected(&ctx, &installed, path_var).versions,
        BTreeSet::from([[18, 20, 4], [20, 11, 1], [22, 1, 0]])
    );
}

#[test]
fn lists_global_packages_in_note() {
    let fx = fixture();
    let installed = install(&fx, &["v14.21.3", "v22.1.0"]);
    let modules = installed[0].dir.join("lib/node_modules");
    for relative in [
        "ionic/package.json",
        "@angular/cli/package.json",
        "corepack/x",
    ] {
        write_bytes(&modules.join(relative), 8);
    }
    assert_eq!(
        global_packages(&installed[0].dir),
        vec!["@angular/cli", "ionic"]
    );
    assert!(global_packages(&installed[1].dir).is_empty());
    project_with_nvmrc(&fx, ".nvmrc", "22\n");
    let ctx = fx.ctx_with(
        &crate::system::runner::SystemRunner,
        vec![fx.home.join("field")],
    );
    let output = scan_with(&ctx, None).unwrap();
    assert_eq!(output.findings.len(), 1);
    let finding = &output.findings[0];
    assert_eq!(finding.label, "Node v14.21.3");
    assert_eq!(finding.location, "~/.nvm/versions/node/v14.21.3");
    assert_eq!(finding.risk, crate::model::finding::Risk::Review);
    assert_eq!(
        finding.note.as_deref(),
        Some(
            "Nenhum projeto pede esta versão. Globais que vão junto: @angular/cli, ionic. \
             Para voltar: nvm install 14.21.3."
        )
    );
    assert_eq!(
        output.footnote.as_deref(),
        Some(
            "1 versão ficou protegida (padrão, .nvmrc de projeto ou ativa no PATH). \
             Não achei o node ativo no PATH; só o alias default e os .nvmrc protegem versões."
        )
    );
}

#[test]
fn wildcard_requests_protect_major() {
    let fx = fixture();
    let installed = install(&fx, &VERSIONS);
    let aliases = fx.env.nvm_dir.join("alias");
    for request in ["18.x", "v18.x.x", "18.*", "18.X", "v18.*.*"] {
        assert_eq!(
            resolved(request, &installed, &aliases),
            Some([18, 20, 4]),
            "{request}"
        );
    }
    assert_eq!(resolved("18.19.x", &installed, &aliases), Some([18, 19, 0]));
    assert_eq!(resolved("x.18", &installed, &aliases), None);
    project_with_nvmrc(&fx, ".nvmrc", "18.x\n");
    let ctx = fx.ctx_with(
        &crate::system::runner::SystemRunner,
        vec![fx.home.join("field")],
    );
    let output = scan_with(&ctx, None).unwrap();
    assert!(
        output
            .findings
            .iter()
            .all(|finding| !finding.location.contains("v18.20.4"))
    );
    assert_eq!(output.findings.len(), VERSIONS.len() - 1);
}

#[test]
fn unknown_request_offers_nothing() {
    let fx = fixture();
    install(&fx, &VERSIONS);
    for request in ["lts/-1", "iojs-*", "uma frase qualquer"] {
        project_with_nvmrc(&fx, ".nvmrc", &format!("{request}\n"));
        let ctx = fx.ctx_with(
            &crate::system::runner::SystemRunner,
            vec![fx.home.join("field")],
        );
        let output = scan_with(&ctx, None).unwrap();
        assert!(output.findings.is_empty(), "{request}");
        let expected = format!(
            "Não entendi a versão pedida em {}: '{request}'. Nenhuma versão foi oferecida.",
            fx.home.join("field/app/.nvmrc").display()
        );
        assert_eq!(output.footnote.as_deref(), Some(expected.as_str()));
    }
}

#[test]
fn unknown_default_alias_offers_nothing() {
    let fx = fixture();
    install(&fx, &VERSIONS);
    alias(&fx, "default", "algo-estranho");
    let ctx = fx.ctx_with(
        &crate::system::runner::SystemRunner,
        vec![fx.home.join("field")],
    );
    let output = scan_with(&ctx, None).unwrap();
    assert!(output.findings.is_empty());
    assert!(output.footnote.unwrap().contains("'algo-estranho'"));
}

#[test]
fn note_without_globals() {
    let fx = fixture();
    install(&fx, &["v16.20.2"]);
    let ctx = fx.ctx_with(
        &crate::system::runner::SystemRunner,
        vec![fx.home.join("field")],
    );
    let output = scan_with(&ctx, None).unwrap();
    assert_eq!(
        output.findings[0].note.as_deref(),
        Some("Nenhum projeto pede esta versão. Para voltar: nvm install 16.20.2.")
    );
}

#[test]
fn warns_without_projects() {
    let fx = fixture();
    install(&fx, &["v16.20.2"]);
    let ctx = fx.ctx_with(&crate::system::runner::SystemRunner, Vec::new());
    let output = scan_with(&ctx, None).unwrap();
    assert_eq!(output.findings.len(), 1);
    assert!(output.footnote.unwrap().contains("Sem pastas de projetos"));
}

#[test]
fn missing_nvm() {
    let fx = fixture();
    let ctx = fx.ctx_with(&crate::system::runner::SystemRunner, Vec::new());
    assert_eq!(scan_with(&ctx, None), Err(ScanProblem::ToolMissing("nvm")));
}

#[test]
fn roots_are_the_versions_dir() {
    let fx = fixture();
    let ctx = fx.ctx();
    assert_eq!(
        NvmVersions.roots(&ctx),
        vec![fx.env.nvm_dir.join("versions/node")]
    );
}
