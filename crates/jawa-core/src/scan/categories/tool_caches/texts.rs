use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Caches de ferramentas";

pub(super) const SUMMARY: &str =
    "Downloads e caches de ferramentas de desenvolvimento, refeitos quando forem precisos.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Pastas que as ferramentas usam para não baixar ou compilar de novo: navegadores \
                 do Playwright, Cypress e Puppeteer, caches do yarn, pnpm, pip, Prisma, Go, \
                 node-gyp e TypeScript, miniaturas de imagens, o cache do Gradle, o registry do \
                 Cargo e versões antigas do Claude Code.",
    why_it_grows: "Cada versão nova de ferramenta, navegador ou dependência é guardada ao lado \
                   das antigas, e quase nenhuma ferramenta apaga o que não usa mais.",
    effect: "As pastas são apagadas. Cada ferramenta baixa ou recria o que precisar na próxima \
             vez que rodar.",
    reversibility_detail: "Regenerável: nada de seu se perde. A versão do Claude Code em uso \
                           (o alvo de ~/.local/bin/claude) nunca é oferecida.",
    recreate_cost: "Tempo e internet na primeira execução de cada ferramenta depois da limpeza.",
    recommendation: "Pode limpar quando precisar de espaço, de preferência sem builds rodando.",
};
