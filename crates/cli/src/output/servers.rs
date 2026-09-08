use crate::ui;
use console::{Alignment, Term, measure_text_width, pad_str, truncate_str};
use remote_codex_client::{
    Result,
    application::{ServerStatus, ServerSummary},
};
use std::io::{self, Write};

pub(crate) fn server_list(records: &[ServerSummary], refreshed: &[ServerStatus]) -> Result<()> {
    let mut out = io::stdout().lock();
    if records.is_empty() {
        writeln!(out, "No servers configured.")?;
        writeln!(
            out,
            "Add one: remote-codex server add -n NAME --addr USER@HOST"
        )?;
        return Ok(());
    }
    let rows: Vec<_> = records
        .iter()
        .map(|record| {
            let check = refreshed.iter().find(|check| check.server == record.name);
            [
                ui::text(&record.name),
                ui::text(&record.endpoint.default_name()),
                status(check).to_owned(),
            ]
        })
        .collect();
    let width = Term::stdout()
        .size_checked()
        .filter(|(_, width)| *width > 0)
        .map(|(_, w)| usize::from(w))
        .unwrap_or(100);
    if width < 40 {
        // Keep addresses readable on small terminals without squeezing columns.
        for row in &rows {
            writeln!(out, "{}", truncate_str(&row[0], width, "…"))?;
            writeln!(out, "{}", truncate_str(&row[1], width, "…"))?;
            writeln!(out, "{}\n", truncate_str(&row[2], width, "…"))?;
        }
    } else {
        let status_width = rows
            .iter()
            .map(|r| measure_text_width(&r[2]))
            .max()
            .unwrap_or(6)
            .max(6);
        let available = width.saturating_sub(status_width + 4);
        let name_width = rows
            .iter()
            .map(|r| measure_text_width(&r[0]))
            .max()
            .unwrap_or(4)
            .clamp(4, 24)
            .min(available / 3);
        let address_width = rows
            .iter()
            .map(|r| measure_text_width(&r[1]))
            .max()
            .unwrap_or(7)
            .max(7)
            .min(available - name_width);
        write_row(
            &mut out,
            ["NAME", "ADDRESS", "STATUS"],
            name_width,
            address_width,
        )?;
        for row in &rows {
            write_row(
                &mut out,
                [&row[0], &row[1], &row[2]],
                name_width,
                address_width,
            )?;
        }
    }
    let mut notes = Vec::new();
    for record in records {
        if let Some(check) = refreshed.iter().find(|check| check.server == record.name) {
            let name = ui::text(&record.name);
            if let Some(error) = &check.error {
                notes.push(format!("{name}: {}", ui::text(error)));
            }
        }
    }
    if !notes.is_empty() {
        writeln!(out)?;
        for note in notes {
            writeln!(out, "{note}")?;
        }
    }
    Ok(())
}

fn status(check: Option<&ServerStatus>) -> &'static str {
    match check.map(|check| check.status.as_str()) {
        Some("ready") => "Ready",
        Some("unavailable") => "Unavailable",
        Some("timeout") => "Timed out",
        _ => "Unknown",
    }
}

fn write_row(
    out: &mut impl Write,
    values: [&str; 3],
    name_width: usize,
    address_width: usize,
) -> io::Result<()> {
    let name = truncate_str(values[0], name_width, "…");
    let address = truncate_str(values[1], address_width, "…");
    writeln!(
        out,
        "{}  {}  {}",
        pad_str(&name, name_width, Alignment::Left, None),
        pad_str(&address, address_width, Alignment::Left, None),
        values[2]
    )
}

#[cfg(test)]
mod tests {
    use super::write_row;

    #[test]
    fn truncation_preserves_utf8_and_pads_by_display_width() -> std::io::Result<()> {
        for (name, address, expected) in [
            ("测试服务器", "root@host", "测试…   root@host   Ready\n"),
            (
                "abcdefghij",
                "root@very-long.invalid",
                "abcde…  root@very…  Ready\n",
            ),
        ] {
            let mut output = Vec::new();
            write_row(&mut output, [name, address, "Ready"], 6, 10)?;
            assert_eq!(output, expected.as_bytes());
        }
        Ok(())
    }
}
