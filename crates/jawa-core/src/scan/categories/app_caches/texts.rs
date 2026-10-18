//! Textos da categoria, em PT-BR, exibidos por `jawa explicar` e na TUI.

use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Caches de aplicativos";

pub(super) const SUMMARY: &str =
    "Caches de navegadores, VS Code, Slack, Discord e Spotify, só com o app fechado.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Os caches do Chrome, Chromium, Firefox e Spotify em ~/.cache e as subpastas de \
                 cache do VS Code, Slack e Discord em ~/.config. Do ~/.config só entram as \
                 pastas de cache conhecidas; configurações, extensões e sessões nunca.",
    why_it_grows: "Os apps guardam páginas, imagens, código compilado e logs para abrir mais \
                   rápido, e raramente limpam sozinhos.",
    effect: "As pastas de cache são apagadas. O app recria tudo ao abrir. Com o app aberto, o \
             item fica bloqueado.",
    reversibility_detail: "Regenerável: logins, favoritos e configurações ficam fora dessas \
                           pastas.",
    recreate_cost: "O app abre mais devagar na primeira vez e baixa de novo o que precisar.",
    recommendation: "Feche o app e limpe quando precisar de espaço.",
};
