//! Textos da categoria, em PT-BR, exibidos por `jawa explicar` e na TUI.

use crate::scan::categories::Explanation;

pub(super) const IMAGES_TITLE: &str = "Docker: imagens e cache de build";

pub(super) const IMAGES_SUMMARY: &str =
    "Imagens sem container e o cache de build do Docker, com o tamanho do `docker system df`.";

pub(super) const IMAGES_EXPLANATION: Explanation = Explanation {
    what_it_is: "Imagens que nenhum container usa e as camadas que o BuildKit guarda para \
                 acelerar os builds.",
    why_it_grows: "Cada `docker pull` e cada build deixa imagens e camadas novas, e o Docker não \
                   apaga as antigas sozinho.",
    effect: "O Jawa roda `docker image prune -a -f` e `docker builder prune -a -f`. Imagens em \
             uso por algum container ficam.",
    reversibility_detail: "Regenerável: as imagens voltam com `docker pull` ou com o próximo \
                           build.",
    recreate_cost: "Downloads e builds mais lentos na próxima vez.",
    recommendation: "Pode limpar. Precisa do Docker rodando.",
};

pub(super) const VOLUMES_TITLE: &str = "Docker: volumes e containers parados";

pub(super) const VOLUMES_SUMMARY: &str =
    "Volumes sem nenhum container, um por um, e os containers parados.";

pub(super) const VOLUMES_EXPLANATION: Explanation = Explanation {
    what_it_is: "Volumes que nenhum container usa, nem parado, cada um com o projeto do compose \
                 que o criou, e os containers parados.",
    why_it_grows: "Bancos locais (Postgres, MySQL, Redis) gravam nos volumes, e eles ficam no \
                   disco quando o projeto é desmontado.",
    effect: "Cada volume escolhido é apagado com `docker volume rm <nome>`; os containers \
             parados medidos, com `docker container rm <ids>`. Nunca há `prune`.",
    reversibility_detail: "Irreversível: os dados do volume ou do container somem de vez.",
    recreate_cost: "Recriar o banco local do zero, com migrações e dados de teste.",
    recommendation: "Escolha volume por volume e só os de projetos que você não usa mais.",
};

pub(super) const DESKTOP_TITLE: &str = "Disco virtual do Docker Desktop";

pub(super) const DESKTOP_SUMMARY: &str =
    "O arquivo Docker.raw da máquina virtual do Docker Desktop, com o tamanho real.";

pub(super) const DESKTOP_EXPLANATION: Explanation = Explanation {
    what_it_is: "O disco da máquina virtual do Docker Desktop \
                 (~/.docker/desktop/vms/0/data/Docker.raw), onde ficam todas as imagens, \
                 containers e volumes.",
    why_it_grows: "O arquivo cresce com o uso e não devolve sozinho o espaço do que foi apagado \
                   dentro dele.",
    effect: "Com o Docker Desktop instalado, o Jawa só mostra o comando oficial de recuperação \
             de espaço. Sem o Docker Desktop, oferece apagar a pasta da máquina virtual que \
             sobrou.",
    reversibility_detail: "Irreversível: apagar o disco leva junto tudo o que estava no Docker \
                           Desktop.",
    recreate_cost: "Baixar de novo todas as imagens e recriar containers e volumes.",
    recommendation: "Com o Docker Desktop instalado, rode o comando guiado. Sem ele, apague só \
                     se não precisar de nada que estava lá.",
};
