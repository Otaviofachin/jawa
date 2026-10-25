use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Cache do npm";

pub(super) const SUMMARY: &str = "Cópia local de cada pacote baixado pelo npm e pelo npx.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "A cópia local de cada pacote que o npm já baixou (~/.npm/_cacache) e a pasta \
                 _npx, com os pacotes que você rodou por npx.",
    why_it_grows: "Cada versão de cada pacote instalado fica guardada, e o npm quase nunca \
                   apaga nada sozinho.",
    effect: "Os arquivos são apagados. O próximo `npm install` ou `npx` baixa de novo o que \
             precisar.",
    reversibility_detail: "Regenerável: nada de seu se perde, tudo volta a ser baixado do registry.",
    recreate_cost: "Só tempo e internet: o primeiro install de cada projeto fica mais lento.",
    recommendation: "Pode limpar sem medo quando precisar de espaço.",
};
