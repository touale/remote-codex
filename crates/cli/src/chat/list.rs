use console::{Alignment, measure_text_width, pad_str, strip_ansi_codes, truncate_str};
use remote_codex_client::{Result, application::CachedSession};
use std::io::Write;

mod picker;
#[cfg(test)]
mod tests;

pub(super) use picker::choose;

pub(super) struct Row {
    title: String,
    server: String,
    workspace: String,
    active: String,
    id: String,
}

impl Row {
    pub(super) fn new(entry: &CachedSession, now: u64) -> Self {
        let title = clean(&entry.session.title);
        Self {
            title: if title.trim().is_empty() {
                "Untitled session".into()
            } else {
                title
            },
            server: clean(&entry.server),
            workspace: clean(&entry.session.cwd),
            active: age(entry.session.updated_at, now),
            id: clean(&entry.session.id),
        }
    }

    fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.title, self.server, self.workspace, self.id
        )
    }
}

// Widths are terminal cells, never UTF-8 bytes or character counts.
struct Layout {
    title: usize,
    server: usize,
    workspace: usize,
    active: usize,
    id: usize,
}

impl Layout {
    fn new(columns: usize, id: usize) -> Self {
        // Reserve the selection marker and one cell to avoid terminal autowrap.
        let available = columns.saturating_sub(3 + if id > 0 { id + 2 } else { 0 });
        let active = available.min(6);
        let server = if columns >= 45 { 16 } else { 0 };
        let workspace = if columns >= 70 {
            (columns / 4).min(28)
        } else {
            0
        };
        let metadata = active
            + server
            + workspace
            + 2 * (usize::from(server > 0) + usize::from(workspace > 0));
        Self {
            title: available.saturating_sub(metadata + 2),
            server,
            workspace,
            active,
            id,
        }
    }

    fn header(&self) -> String {
        self.render(&Row {
            title: "Session".into(),
            server: "Server".into(),
            workspace: "Workspace".into(),
            active: "Active".into(),
            id: "ID".into(),
        })
    }

    fn render(&self, row: &Row) -> String {
        [
            (row.title.as_str(), self.title, false),
            (&row.server, self.server, false),
            (&row.workspace, self.workspace, true),
            (&row.active, self.active, false),
            (&row.id, self.id, false),
        ]
        .into_iter()
        .filter(|(_, width, _)| *width > 0)
        .map(|(text, width, from_left)| {
            let text = if from_left {
                suffix(text, width)
            } else {
                truncate_str(text, width, "…").into_owned()
            };
            pad_str(&text, width, Alignment::Left, None).into_owned()
        })
        .collect::<Vec<_>>()
        .join("  ")
    }
}

fn clean(value: &str) -> String {
    strip_ansi_codes(value)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn suffix(value: &str, width: usize) -> String {
    if measure_text_width(value) <= width {
        return value.into();
    }
    let mut tail = "";
    for (index, _) in value.char_indices().rev() {
        let candidate = &value[index..];
        if measure_text_width(candidate) >= width {
            break;
        }
        tail = candidate;
    }
    format!("…{tail}")
}

pub(super) fn print(rows: &[Row]) -> Result<()> {
    let id_width = rows
        .iter()
        .map(|row| measure_text_width(&row.id))
        .max()
        .unwrap_or(2)
        .max(2);
    let layout = Layout::new(160, id_width);
    let mut output = std::io::stdout().lock();
    writeln!(output, "{}", layout.header())?;
    for row in rows {
        writeln!(output, "{}", layout.render(row))?;
    }
    Ok(())
}

fn age(timestamp: i64, now: u64) -> String {
    // Match the desktop Date range; absent timestamps must not look like 1970.
    if !(1..=8_640_000_000_000).contains(&timestamp) {
        return "—".into();
    }
    let seconds = now.saturating_sub(timestamp as u64);
    let (unit, suffix) = match seconds {
        0..60 => (1, "s"),
        60..3600 => (60, "m"),
        3600..86400 => (3600, "h"),
        _ => (86400, "d"),
    };
    format!("{}{suffix}", seconds / unit)
}
