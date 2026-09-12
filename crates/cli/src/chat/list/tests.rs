use super::*;

#[test]
fn short_activity_time_handles_boundaries_and_invalid_clocks() {
    let now = 2_000_000;
    for (elapsed, expected) in [
        (0, "0s"),
        (59, "59s"),
        (60, "1m"),
        (3599, "59m"),
        (3600, "1h"),
        (86399, "23h"),
        (86400, "1d"),
        (259200, "3d"),
    ] {
        assert_eq!(age((now - elapsed) as i64, now), expected);
    }
    assert_eq!(age(now as i64 + 100, now), "0s");
    for timestamp in [0, -1, i64::MAX] {
        assert_eq!(age(timestamp, now), "—");
    }
}

#[test]
fn rows_fit_terminal_cells_and_preserve_directory_suffixes_and_ids() {
    let mut row = Row {
        title: String::new(),
        server: "a-very-long-server-name".into(),
        workspace: "/workspace/很长的中文目录/paper/excavate".into(),
        active: "3d".into(),
        id: "01a09015-25db-7a30-9fa2-49c800433dd0".into(),
    };
    for title in [
        "A long English title ".repeat(40),
        "中文长标题".repeat(40),
        "👩‍💻 Review 🦀 ".repeat(40),
    ] {
        row.title = title;
        for columns in [12, 40, 44, 45, 69, 70, 80, 100, 160, 200] {
            let layout = Layout::new(columns, 0);
            let line = layout.render(&row);
            assert!(
                measure_text_width(&line) <= columns - 3,
                "{columns}: {line}"
            );
            assert_eq!(
                measure_text_width(&line),
                measure_text_width(&layout.header())
            );
            assert!(line.contains("3d"));
            if columns >= 70 {
                assert!(line.contains("excavate"));
            } else {
                assert!(!line.contains("excavate"));
            }
            if columns < 45 {
                assert!(!line.contains("server"));
            }
        }
    }
    let line = Layout::new(160, row.id.len()).render(&row);
    assert!(line.ends_with(&row.id));
    assert!(measure_text_width(&line) <= 160);
}

#[test]
fn untrusted_display_text_cannot_inject_terminal_control_sequences() {
    assert_eq!(clean("\x1b[31mHello\x1b[0m\nWorld\t!\r"), "Hello World ! ");
    assert!(!clean("\x1b]0;title\x07Hello").chars().any(char::is_control));
    assert_eq!(clean("中文 👩‍💻"), "中文 👩‍💻");
}
