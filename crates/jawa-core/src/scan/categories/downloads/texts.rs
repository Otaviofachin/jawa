//! Textos da categoria, em PT-BR, exibidos por `jawa explicar` e na TUI.

use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Downloads grandes e antigos";

pub(super) const SUMMARY: &str =
    "Pacotes, arquivos grandes e itens esquecidos na pasta de downloads, enviados à Lixeira.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Itens da pasta de downloads: pacotes de instalação (.deb, .rpm, .AppImage, \
                 .iso, .img, .tar.gz, .tgz, .tar.xz, .zip) de 10 MB ou mais, qualquer item de \
                 100 MB ou mais e itens de 10 MB ou mais parados há 90 dias. Ocultos e \
                 downloads em andamento ficam de fora.",
    why_it_grows: "Instaladores e arquivos baixados uma vez ficam esquecidos depois de usados.",
    effect: "Os itens escolhidos vão para a Lixeira.",
    reversibility_detail: "Pela Lixeira: dá para restaurar até você esvaziá-la. O espaço só \
                           volta depois disso.",
    recreate_cost: "Baixar de novo, se precisar.",
    recommendation: "Revise item por item: o Jawa não sabe o que ainda é importante para você.",
};
