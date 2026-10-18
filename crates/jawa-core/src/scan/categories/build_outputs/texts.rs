//! Textos da categoria, em PT-BR, exibidos por `jawa explicar` e na TUI.

use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Saídas de build";

pub(super) const SUMMARY: &str =
    "Pastas geradas por builds (dist, build, www, coverage e caches de build) ignoradas pelo git.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Pastas que as ferramentas de build criam dentro dos seus repositórios: dist, \
                 build, www, coverage, .angular/cache e .nx/cache. Só entram as que o git \
                 ignora; nada versionado é oferecido.",
    why_it_grows: "Cada build reescreve a saída e os caches guardam versões antigas de tudo o \
                   que já foi compilado.",
    effect: "A pasta é apagada. O próximo build, teste ou `ng serve` a recria.",
    reversibility_detail: "Regenerável: a pasta é produto do código-fonte. Exceção: se ela \
                           guarda um APK, AAB, IPA ou keystore assinado, o item vira Revisar, \
                           porque refazer a assinatura pode não ser possível.",
    recreate_cost: "O tempo de um build completo; o primeiro build depois fica mais lento.",
    recommendation: "Pode limpar. Nos itens marcados Revisar, confira antes se o arquivo \
                     assinado tem outra cópia.",
};
