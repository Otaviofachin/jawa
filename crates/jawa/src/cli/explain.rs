use std::process::ExitCode;

use jawa_core::model::finding::CategoryId;
use jawa_core::scan::categories::registry;

use super::{CliError, CliResult};

pub fn run(id: CategoryId) -> CliResult {
    let category = registry()
        .iter()
        .find(|category| category.descriptor().id == id)
        .ok_or_else(|| {
            CliError::Message(format!(
                "a categoria {id} ainda não está disponível nesta versão"
            ))
        })?;
    let descriptor = category.descriptor();
    let explanation = &descriptor.explanation;
    println!("{} ({})", descriptor.title, descriptor.id);
    println!("{}\n", descriptor.summary);
    let sections = [
        ("O que é", explanation.what_it_is),
        ("Por que cresce", explanation.why_it_grows),
        ("O que acontece ao limpar", explanation.effect),
        ("Dá para desfazer?", explanation.reversibility_detail),
        ("Custo para recriar", explanation.recreate_cost),
        ("Recomendação", explanation.recommendation),
    ];
    for (heading, text) in sections {
        println!("{heading}\n  {text}\n");
    }
    Ok(ExitCode::SUCCESS)
}
