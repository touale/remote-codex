use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    terminal,
};

pub(crate) struct RawTerminal;
impl RawTerminal {
    pub(crate) fn enter() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}
impl Drop for RawTerminal {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Encode console keys for the ANSI input consumed by ConPTY and the remote PTY.
pub(crate) fn key_bytes(key: KeyEvent) -> Vec<u8> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let cursor = match key.code {
        KeyCode::Up => Some('A'),
        KeyCode::Down => Some('B'),
        KeyCode::Right => Some('C'),
        KeyCode::Left => Some('D'),
        KeyCode::Home => Some('H'),
        KeyCode::End => Some('F'),
        _ => None,
    };
    if let Some(cursor) = cursor {
        return if modifier == 1 {
            format!("\x1b[{cursor}")
        } else {
            format!("\x1b[1;{modifier}{cursor}")
        }
        .into_bytes();
    }
    let numbered = match key.code {
        KeyCode::Insert => Some(2),
        KeyCode::Delete => Some(3),
        KeyCode::PageUp => Some(5),
        KeyCode::PageDown => Some(6),
        KeyCode::F(n @ 5..=12) => Some([15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]),
        _ => None,
    };
    if let Some(number) = numbered {
        return if modifier == 1 {
            format!("\x1b[{number}~")
        } else {
            format!("\x1b[{number};{modifier}~")
        }
        .into_bytes();
    }
    let text = match key.code {
        KeyCode::Char(' ') if ctrl => "\0".into(),
        KeyCode::Char(c) if ctrl && c.is_ascii_alphabetic() => {
            char::from(c as u8 & 0x1f).to_string()
        }
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter if shift || ctrl => return format!("\x1b[13;{modifier}u").into_bytes(),
        KeyCode::Enter => "\r".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::F(n @ 1..=4) => {
            return if modifier == 1 {
                format!("\x1bO{}", char::from(b'P' + n - 1))
            } else {
                format!("\x1b[1;{modifier}{}", char::from(b'P' + n - 1))
            }
            .into_bytes();
        }
        _ => return Vec::new(),
    };
    if alt {
        format!("\x1b{text}").into_bytes()
    } else {
        text.into_bytes()
    }
}

/// Console writes require complete UTF-8; PTY reads can split a character across packets.
#[derive(Default)]
pub(crate) struct Output {
    pending: Vec<u8>,
}
impl Output {
    pub(crate) fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        self.pending.extend_from_slice(bytes);
        let mut output = std::io::stdout().lock();
        let mut offset = 0;
        while offset < self.pending.len() {
            let rest = &self.pending[offset..];
            match std::str::from_utf8(rest) {
                Ok(_) => {
                    output.write_all(rest)?;
                    offset = self.pending.len();
                }
                Err(error) => {
                    output.write_all(&rest[..error.valid_up_to()])?;
                    offset += error.valid_up_to();
                    let Some(invalid) = error.error_len() else {
                        break;
                    };
                    output.write_all("\u{fffd}".as_bytes())?;
                    offset += invalid;
                }
            }
        }
        self.pending.drain(..offset);
        output.flush()
    }
}
