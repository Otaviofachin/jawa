use std::io;

use thiserror::Error;

use crate::model::finding::FindingId;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("o Jawa não roda como root: use seu usuário comum, sem sudo")]
    RunningAsRoot,
    #[error("não foi possível descobrir a pasta pessoal")]
    MissingHome,
    #[error("a pasta pessoal não pode ser a raiz `/`: confira a variável HOME")]
    HomeIsRoot,
    #[error("a pasta pessoal {} não pertence ao seu usuário: confira a variável HOME", .0.display())]
    HomeNotOwned(std::path::PathBuf),
    #[error("erro de entrada e saída: {0}")]
    Io(#[from] io::Error),
    #[error("configuração inválida: {0}")]
    InvalidConfig(String),
    #[error("outra execução do jawa está em andamento")]
    Busy,
    #[error("o id {0} não está na última medição; rode `jawa medir` de novo.")]
    UnknownId(FindingId),
}
