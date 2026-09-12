use super::{Layout, Row, suffix};
use console::{Term, style, truncate_str};
use crossterm::{
    event::{self, Event, KeyCode as Key, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use remote_codex_client::{ClientError, Result};
use std::io;

pub(in crate::chat) fn choose(rows: &[Row]) -> Result<usize> {
    let screen = Screen::open()?;
    let matcher = SkimMatcherV2::default();
    let search: Vec<_> = rows.iter().map(Row::search_text).collect();
    let mut query = String::new();
    let mut cursor = 0;
    let mut matches = matching(&search, &query, &matcher);
    let mut selected: usize = 0;
    loop {
        let page = screen.render(rows, &matches, selected, &query, cursor)?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.code == Key::Esc
            || (matches!(key.code, Key::Char('c' | 'C'))
                && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Err(ClientError::Argument("selection cancelled"));
        }
        match key.code {
            _ if page == 0 => {}
            Key::Enter => {
                if let Some(&index) = matches.get(selected) {
                    return Ok(index);
                }
            }
            Key::Up | Key::BackTab => selected = selected.saturating_sub(1),
            Key::Down | Key::Tab => selected = (selected + 1).min(matches.len().saturating_sub(1)),
            Key::PageUp => selected = selected.saturating_sub(page),
            Key::PageDown => selected = (selected + page).min(matches.len().saturating_sub(1)),
            Key::Home => selected = 0,
            Key::End => selected = matches.len().saturating_sub(1),
            Key::Left if cursor > 0 => {
                cursor = query[..cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i);
            }
            Key::Right if cursor < query.len() => {
                cursor += query[cursor..].chars().next().map_or(0, char::len_utf8);
            }
            Key::Backspace if cursor > 0 => {
                let previous = query[..cursor]
                    .char_indices()
                    .next_back()
                    .map_or(0, |(i, _)| i);
                query.drain(previous..cursor);
                cursor = previous;
                matches = matching(&search, &query, &matcher);
                selected = 0;
            }
            Key::Delete if cursor < query.len() => {
                query.remove(cursor);
                matches = matching(&search, &query, &matcher);
                selected = 0;
            }
            Key::Char(c)
                if !c.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                query.insert(cursor, c);
                cursor += c.len_utf8();
                matches = matching(&search, &query, &matcher);
                selected = 0;
            }
            _ => {}
        }
    }
}

fn matching(search: &[String], query: &str, matcher: &SkimMatcherV2) -> Vec<usize> {
    let mut matches: Vec<_> = search
        .iter()
        .enumerate()
        .filter_map(|(index, text)| {
            let score = if query.is_empty() {
                Some(0)
            } else {
                matcher.fuzzy_match(text, query)
            };
            score.map(|score| (index, score))
        })
        .collect();
    // Stable ties retain the catalog's recent-activity order.
    matches.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
    matches.into_iter().map(|(index, _)| index).collect()
}

// Own a temporary screen so redraws cannot erase preceding shell output, even
// when resized. Drop restores it on selection, cancellation or IO failure.
struct Screen(Term);

impl Screen {
    fn open() -> io::Result<Self> {
        enable_raw_mode()?;
        let screen = Self(Term::buffered_stderr());
        screen.0.write_str("\x1b[?1049h")?;
        screen.0.hide_cursor()?;
        screen.0.flush()?;
        Ok(screen)
    }

    fn render(
        &self,
        rows: &[Row],
        matches: &[usize],
        selected: usize,
        query: &str,
        cursor: usize,
    ) -> io::Result<usize> {
        let (height, width) = self.0.size();
        let width = usize::from(width);
        let height = usize::from(height);
        // Buffer the complete frame, clearing only the owned screen.
        self.0.move_cursor_to(0, 0)?;
        self.0.clear_to_end_of_screen()?;
        if height < 8 || width < 16 {
            self.0.write_str(&truncate_str(
                "Enlarge terminal, then press a key. Esc cancels.",
                width.saturating_sub(1),
                "",
            ))?;
            self.0.flush()?;
            return Ok(0);
        }
        let page = height.saturating_sub(7).max(1);
        let layout = Layout::new(width, 0);
        let start = selected / page * page;
        let mut lines = vec![
            style("Resume session").bold().to_string(),
            format!(
                "Search: {}▏{}",
                suffix(&query[..cursor], width.saturating_sub(10)),
                &query[cursor..]
            ),
            String::new(),
            style(format!("  {}", layout.header())).dim().to_string(),
        ];
        for &index in matches.iter().skip(start).take(page) {
            let label = layout.render(&rows[index]);
            lines.push(if matches.get(selected) == Some(&index) {
                style(format!("❯ {label}")).cyan().bold().to_string()
            } else {
                format!("  {label}")
            });
        }
        if matches.is_empty() {
            lines.push("  No matching sessions".into());
        }
        lines.push(String::new());
        lines.push(
            style(format!(
                "↑↓ Select · Enter Resume · Esc Cancel  ({}/{})",
                if matches.is_empty() { 0 } else { selected + 1 },
                matches.len()
            ))
            .dim()
            .to_string(),
        );
        for (index, line) in lines.iter().take(height.saturating_sub(1)).enumerate() {
            if index > 0 {
                self.0.write_str("\r\n")?;
            }
            self.0
                .write_str(&truncate_str(line, width.saturating_sub(1), ""))?;
        }
        self.0.flush()?;
        Ok(page)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = self.0.write_str("\x1b[?1049l");
        let _ = self.0.show_cursor();
        let _ = self.0.flush();
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_uses_full_fields_and_returns_original_indices_with_stable_ties() {
        let row = |id: &str| Row {
            title: format!("{}unique-tail", "相同的长标题".repeat(40)),
            server: "dev".into(),
            workspace: "/hidden-prefix/project".into(),
            active: "2m".into(),
            id: id.into(),
        };
        let rows = [row("session-first"), row("session-second")];
        assert_eq!(
            Layout::new(70, 0).render(&rows[0]),
            Layout::new(70, 0).render(&rows[1])
        );
        let search: Vec<_> = rows.iter().map(Row::search_text).collect();
        let matcher = SkimMatcherV2::default();
        for query in ["", "unique-tail", "hidden-prefix", "dev"] {
            assert_eq!(matching(&search, query, &matcher), vec![0, 1]);
        }
        assert_eq!(matching(&search, "session-second", &matcher), vec![1]);
        assert!(matching(&search, "does-not-exist", &matcher).is_empty());
    }
}
