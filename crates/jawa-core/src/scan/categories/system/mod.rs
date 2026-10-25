mod texts;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{Category, Descriptor, ScanOutput, ScanProblem};
use crate::model::finding::{
    Action, CategoryId, CommandSpec, Finding, FindingId, GuideSpec, Reversibility, Risk,
};
use crate::scan::{MIN_ITEM_BYTES, ScanContext, join_footnotes};
use crate::system::runner::CommandOutput;

pub const JOURNAL_LIMIT_BYTES: u64 = 200 * 1024 * 1024;
const JOURNAL_COMMAND: &str = "sudo journalctl --vacuum-size=200M";
const APT_COMMAND: &str = "sudo apt-get clean";
const CRASH_COMMAND: &str = "sudo rm -f /var/crash/*.crash";
const JOURNAL_LOCATION: &str = "/var/log/journal";
const BLOCK_SIZE: u64 = 512;
const BINARY_UNITS: [(char, i32); 5] = [('B', 0), ('K', 1), ('M', 2), ('G', 3), ('T', 4)];

pub fn parse_journal_usage(stdout: &str) -> Option<u64> {
    let (_, after) = stdout.split_once("take up ")?;
    let size = after.split_whitespace().next()?;
    let unit = size.chars().last()?;
    let (_, power) = BINARY_UNITS.iter().find(|(letter, _)| *letter == unit)?;
    let value: f64 = size.strip_suffix(unit)?.parse().ok()?;
    let bytes = (value * 1024f64.powi(*power)).round();
    (bytes.is_finite() && bytes >= 0.0).then_some(bytes as u64)
}

pub fn parse_snap_disabled(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .skip(1)
        .filter_map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            let notes = columns.last()?;
            let disabled = notes.split(',').any(|note| note == "disabled");
            (disabled && columns.len() >= 3)
                .then(|| (columns[0].to_string(), columns[2].to_string()))
        })
        .collect()
}

static DESCRIPTOR: Descriptor = Descriptor {
    id: CategoryId::System,
    title: texts::TITLE,
    summary: texts::SUMMARY,
    explanation: texts::EXPLANATION,
};

pub struct System;

impl Category for System {
    fn descriptor(&self) -> &'static Descriptor {
        &DESCRIPTOR
    }

    fn roots(&self, _ctx: &ScanContext) -> Vec<PathBuf> {
        Vec::new()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<ScanOutput, ScanProblem> {
        let parts = [
            journal_part(ctx),
            apt_part(ctx),
            snap_part(ctx),
            crash_part(ctx),
        ];
        let mut findings = Vec::new();
        let mut notes = Vec::new();
        for part in parts {
            findings.extend(part.findings);
            notes.push(part.footnote);
        }
        findings.retain(|finding| finding.bytes >= MIN_ITEM_BYTES);
        Ok(ScanOutput {
            findings,
            footnote: join_footnotes(notes),
        })
    }
}

fn skipped(note: &str) -> ScanOutput {
    ScanOutput {
        findings: Vec::new(),
        footnote: Some(note.to_string()),
    }
}

fn read(ctx: &ScanContext, exe: &Path, args: &[&str]) -> Option<CommandOutput> {
    ctx.runner
        .run(&CommandSpec {
            program: exe.to_path_buf(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            stdin: None,
            env: Vec::new(),
            timeout: ctx.read_timeout,
        })
        .ok()
        .filter(|output| output.success)
}

fn journal_part(ctx: &ScanContext) -> ScanOutput {
    let Some(exe) = ctx.env.tools.journalctl.as_deref() else {
        return skipped("journalctl não foi encontrado; o journald não foi medido.");
    };
    let usage =
        read(ctx, exe, &["--disk-usage"]).and_then(|output| parse_journal_usage(&output.stdout));
    let Some(usage) = usage else {
        return skipped("Não consegui ler o uso do journald.");
    };
    if usage <= JOURNAL_LIMIT_BYTES {
        return ScanOutput::default();
    }
    vec![guided(GuidedItem {
        label: "Logs do journald acima de 200 MB".to_string(),
        location: JOURNAL_LOCATION.to_string(),
        bytes: usage - JOURNAL_LIMIT_BYTES,
        command: JOURNAL_COMMAND.to_string(),
        note: "Mantém os 200 MB de log mais recentes.",
    })]
    .into()
}

fn apt_part(ctx: &ScanContext) -> ScanOutput {
    if ctx.env.tools.apt.is_none() {
        return skipped("apt não foi encontrado; os pacotes baixados não foram medidos.");
    }
    let archives = &ctx.system.apt_archives;
    vec![guided(GuidedItem {
        label: "Pacotes .deb baixados pelo apt".to_string(),
        location: archives.display().to_string(),
        bytes: sum_with_suffix(archives, ".deb"),
        command: APT_COMMAND.to_string(),
        note: "Instaladores já usados; o apt baixa de novo se precisar.",
    })]
    .into()
}

fn crash_part(ctx: &ScanContext) -> ScanOutput {
    let crash_dir = &ctx.system.crash_dir;
    vec![guided(GuidedItem {
        label: "Relatórios de falha".to_string(),
        location: crash_dir.display().to_string(),
        bytes: sum_with_suffix(crash_dir, ".crash"),
        command: CRASH_COMMAND.to_string(),
        note: "Relatórios de programas que travaram; só servem para enviar aos desenvolvedores.",
    })]
    .into()
}

fn snap_part(ctx: &ScanContext) -> ScanOutput {
    let Some(exe) = ctx.env.tools.snap.as_deref() else {
        return skipped("snap não foi encontrado; as revisões de snap não foram medidas.");
    };
    let Some(output) = read(ctx, exe, &["list", "--all"]) else {
        return skipped("Não consegui ler `snap list --all`.");
    };
    parse_snap_disabled(&output.stdout)
        .into_iter()
        .filter(|(name, revision)| is_safe_snap_name(name) && is_revision(revision))
        .map(|(name, revision)| snap_finding(ctx, &name, &revision))
        .collect::<Vec<_>>()
        .into()
}

fn snap_finding(ctx: &ScanContext, name: &str, revision: &str) -> Finding {
    let file = ctx.system.snaps_dir.join(format!("{name}_{revision}.snap"));
    guided(GuidedItem {
        label: format!("Snap {name} (revisão {revision} desativada)"),
        location: file.display().to_string(),
        bytes: allocated(&file),
        command: format!("sudo snap remove {name} --revision={revision}"),
        note: "Revisão antiga que o snap guarda para poder voltar atrás.",
    })
}

fn is_safe_snap_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_revision(revision: &str) -> bool {
    !revision.is_empty() && revision.chars().all(|c| c.is_ascii_digit())
}

fn sum_with_suffix(dir: &Path, suffix: &str) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(suffix))
        .map(|entry| allocated(&entry.path()))
        .sum()
}

fn allocated(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map_or(0, |metadata| metadata.blocks() * BLOCK_SIZE)
}

struct GuidedItem {
    label: String,
    location: String,
    bytes: u64,
    command: String,
    note: &'static str,
}

fn guided(item: GuidedItem) -> Finding {
    let note = format!("{} Para limpar, rode: {}", item.note, item.command);
    let action = Action::Guide(GuideSpec {
        command_line: item.command,
    });
    Finding {
        id: FindingId::of(CategoryId::System, &action),
        category: CategoryId::System,
        label: item.label,
        location: item.location,
        bytes: item.bytes,
        risk: Risk::Guided,
        reversibility: Reversibility::Irreversible,
        note: Some(note),
        action,
        fingerprint: None,
        blocked: None,
    }
}

#[cfg(test)]
mod tests;
