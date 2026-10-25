use std::borrow::Cow;

use jawa_core::clean::CleanReport;
use jawa_core::model::finding::{Reversibility, Risk};
use jawa_core::scan::categories::RECLAIM_COMMAND;

pub const SNAPSHOT_HINT: &str = "Parte do espaço pode estar presa em snapshots (Btrfs/Timeshift).";

const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
const THOUSAND: u128 = 1000;
const SECONDS_PER_DAY: u64 = 86_400;
const DAYS_TO_EPOCH_SHIFT: i64 = 719_468;

const REPLACEMENT: char = '\u{fffd}';

pub fn display_safe(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_unsafe_char) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|c| if is_unsafe_char(c) { REPLACEMENT } else { c })
            .collect(),
    )
}

fn is_unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

pub fn human_bytes(bytes: u64) -> String {
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut unit = 1;
    while unit < UNITS.len() - 1 && u128::from(bytes) >= THOUSAND.pow(unit as u32 + 1) {
        unit += 1;
    }
    let mut tenths = rounded_tenths(bytes, unit);
    if tenths >= 10_000 && unit < UNITS.len() - 1 {
        unit += 1;
        tenths = rounded_tenths(bytes, unit);
    }
    format!("{},{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

fn rounded_tenths(bytes: u64, unit: usize) -> u128 {
    let divisor = THOUSAND.pow(unit as u32);
    (u128::from(bytes) * 10 + divisor / 2) / divisor
}

pub fn risk_label(risk: Risk) -> &'static str {
    match risk {
        Risk::Safe => "Seguro",
        Risk::Review => "Revisar",
        Risk::Guided => "Guiado",
    }
}

pub fn reversibility_label(reversibility: Reversibility) -> &'static str {
    match reversibility {
        Reversibility::Regenerable => "Regenerável",
        Reversibility::Trash => "Lixeira",
        Reversibility::Irreversible => "Irreversível",
    }
}

pub fn render_table(headers: &[&str], rows: &[Vec<String>], right_aligned: &[usize]) -> String {
    let widths: Vec<usize> = (0..headers.len())
        .map(|column| {
            rows.iter()
                .map(|row| row[column].chars().count())
                .chain(std::iter::once(headers[column].chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let render_row = |cells: Vec<&str>| {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(column, cell)| pad(cell, widths[column], right_aligned.contains(&column)))
            .collect();
        padded.join("  ").trim_end().to_string()
    };
    let mut lines = vec![render_row(headers.to_vec())];
    lines.extend(
        rows.iter()
            .map(|row| render_row(row.iter().map(String::as_str).collect())),
    );
    lines.join("\n")
}

fn pad(cell: &str, width: usize, right: bool) -> String {
    let missing = width.saturating_sub(cell.chars().count());
    let filler = " ".repeat(missing);
    if right {
        format!("{filler}{cell}")
    } else {
        format!("{cell}{filler}")
    }
}

pub fn format_utc(unix_secs: u64) -> String {
    let days = i64::try_from(unix_secs / SECONDS_PER_DAY).unwrap_or(0);
    let (year, month, day) = civil_from_days(days);
    let seconds_of_day = unix_secs % SECONDS_PER_DAY;
    let hour = seconds_of_day / 3600;
    let minute = seconds_of_day % 3600 / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + DAYS_TO_EPOCH_SHIFT;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn space_lines(report: &CleanReport) -> Vec<String> {
    let mut lines = vec![
        format!("Espaço estimado: {}", human_bytes(report.estimated_bytes)),
        format!(
            "Liberado no disco (medido): {}",
            human_bytes(report.measured_bytes())
        ),
    ];
    let in_vm = report.docker_desktop_bytes();
    if in_vm > 0 {
        lines.push(format!(
            "Dentro da VM do Docker Desktop: {}. Só volta ao disco depois de recuperar o \
             disco virtual, com o Docker Desktop ligado: {RECLAIM_COMMAND}",
            human_bytes(in_vm)
        ));
    }
    let trashed = report.trashed_bytes();
    if trashed > 0 {
        lines.push(format!(
            "Na Lixeira: {}. Volta ao disco quando você esvaziar a Lixeira; até lá, dá \
             para restaurar.",
            human_bytes(trashed)
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_safe_replaces_control_characters() {
        assert_eq!(
            display_safe("filme\u{1b}[2J.iso\nfalso\t\u{7f}\u{9b}"),
            "filme\u{fffd}[2J.iso\u{fffd}falso\u{fffd}\u{fffd}\u{fffd}"
        );
        assert_eq!(display_safe("abc\u{202e}gpj.exe"), "abc\u{fffd}gpj.exe");
        assert!(matches!(
            display_safe("~/Downloads/ação.zip"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_000), "1,0 kB");
        assert_eq!(human_bytes(1_500_000), "1,5 MB");
        assert_eq!(human_bytes(3_844_000_000), "3,8 GB");
        assert_eq!(human_bytes(999_950), "1,0 MB");
        assert_eq!(human_bytes(u64::MAX), "18446744,1 TB");
    }

    #[test]
    fn labels_are_portuguese() {
        assert_eq!(risk_label(Risk::Review), "Revisar");
        assert_eq!(reversibility_label(Reversibility::Trash), "Lixeira");
    }

    #[test]
    fn table_aligns_columns() {
        let rows = vec![
            vec!["a".to_string(), "1,5 MB".to_string()],
            vec!["bbb".to_string(), "20 B".to_string()],
        ];
        let table = render_table(&["Nome", "Tam"], &rows, &[1]);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines[0], "Nome     Tam");
        assert_eq!(lines[1], "a     1,5 MB");
        assert_eq!(lines[2], "bbb     20 B");
    }

    #[test]
    fn utc_timestamp_formats() {
        assert_eq!(format_utc(0), "1970-01-01 00:00");
        assert_eq!(format_utc(1_760_000_000), "2025-10-09 08:53");
        assert_eq!(format_utc(1_709_164_800), "2024-02-29 00:00");
    }
}
