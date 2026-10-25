use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Builds do Rust";

pub(super) const SUMMARY: &str =
    "Pastas target/ de projetos Rust, com tudo o que o cargo compilou.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "A pasta target/ ao lado de um Cargo.toml (e src-tauri/target), onde o cargo \
                 guarda objetos compilados, dependências e binários de debug e release.",
    why_it_grows: "Cada versão do compilador e cada mudança de dependência deixa artefatos \
                   novos, e o cargo não apaga os antigos.",
    effect: "A pasta é apagada. O próximo `cargo build` recompila tudo do zero.",
    reversibility_detail: "Regenerável: tudo vem do código-fonte e das dependências.",
    recreate_cost: "Minutos de compilação por projeto grande; o primeiro build depois é lento.",
    recommendation: "Pode limpar. Feche `cargo watch` e o rust-analyzer do projeto antes.",
};
