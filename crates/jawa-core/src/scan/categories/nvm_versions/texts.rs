//! Textos da categoria, em PT-BR, exibidos por `jawa explicar` e na TUI.

use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Versões antigas do Node (nvm)";

pub(super) const SUMMARY: &str = "Versões do Node instaladas pelo nvm que nenhum projeto pede.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Cada pasta em ~/.nvm/versions/node é uma versão completa do Node, com seus \
                 pacotes globais. Ficam protegidas a versão padrão (alias default), as pedidas \
                 por qualquer .nvmrc dos projetos e a ativa no PATH.",
    why_it_grows: "Cada `nvm install` guarda uma versão nova e nada remove as antigas.",
    effect: "A pasta da versão é apagada, junto com os pacotes globais instalados nela.",
    reversibility_detail: "Regenerável: `nvm install` baixa a versão de novo, mas os pacotes \
                           globais precisam ser reinstalados à mão (a nota lista quais).",
    recreate_cost: "Um download de algumas dezenas de MB mais a reinstalação dos globais.",
    recommendation: "Limpe as versões que ninguém pede. Sem a pasta de projetos configurada, o \
                     Jawa não sabe o que os .nvmrc pedem: confira antes.",
};
