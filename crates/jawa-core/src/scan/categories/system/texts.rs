use crate::scan::categories::Explanation;

pub(super) const TITLE: &str = "Sistema (modo guiado)";

pub(super) const SUMMARY: &str =
    "Logs do journald, pacotes do apt, revisões antigas de snap e relatórios de falha.";

pub(super) const EXPLANATION: Explanation = Explanation {
    what_it_is: "Arquivos do sistema que só o root pode apagar: o journald acima de 200 MB, os \
                 .deb baixados pelo apt, as revisões desativadas de snaps e os relatórios em \
                 /var/crash.",
    why_it_grows: "O sistema guarda logs, pacotes baixados e versões anteriores de snaps para \
                   poder voltar atrás.",
    effect: "O Jawa nunca roda nada aqui: ele mede e mostra o comando com sudo para você rodar, \
             se quiser.",
    reversibility_detail: "Os comandos apagam de vez: logs antigos, pacotes baixados, a \
                           revisão anterior do snap e os relatórios de falha.",
    recreate_cost: "Os pacotes são baixados de novo quando precisar; logs e relatórios não \
                    voltam.",
    recommendation: "Rode os comandos que fizerem sentido para você, um de cada vez.",
};
