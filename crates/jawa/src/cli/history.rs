use std::process::ExitCode;

use super::CliResult;
use crate::format::{display_safe, format_utc, human_bytes};
use jawa_core::api::Session;

pub fn run(session: &Session, last: usize) -> CliResult {
    let entries = session.history(last)?;
    if entries.is_empty() {
        println!("Nenhuma ação registrada ainda.");
        return Ok(ExitCode::SUCCESS);
    }
    for entry in entries {
        let line = format!(
            "{} UTC  {}  {}  {}  {}  {}",
            format_utc(entry.at_secs),
            entry.id,
            entry.category,
            human_bytes(entry.bytes),
            entry.location,
            entry.outcome
        );
        println!("{}", display_safe(&line));
    }
    Ok(ExitCode::SUCCESS)
}
