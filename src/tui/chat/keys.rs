//! crossterm keys to the bytes a terminal program reads, after wsx's encoder.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Empty for keys with no encoding, and for ctrl-z and ctrl-d, which would suspend or end the agent.
pub fn encode_key(k: KeyEvent) -> Vec<u8> {
    use KeyCode::*;
    let bytes: &[u8] = match k.code {
        Char(c) if k.modifiers.contains(KeyModifiers::CONTROL) && c.is_ascii_alphabetic() => {
            return match c.to_ascii_lowercase() {
                'z' | 'd' => vec![],
                c => vec![c as u8 - b'a' + 1],
            };
        }
        Char(c) => {
            let mut out = if k.modifiers.contains(KeyModifiers::ALT) { vec![0x1b] } else { vec![] };
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            return out;
        }
        Enter => b"\r",
        Backspace => b"\x7f",
        Tab => b"\t",
        BackTab => b"\x1b[Z",
        Esc => b"\x1b",
        Left => b"\x1b[D",
        Right => b"\x1b[C",
        Up => b"\x1b[A",
        Down => b"\x1b[B",
        Home => b"\x1b[H",
        End => b"\x1b[F",
        PageUp => b"\x1b[5~",
        PageDown => b"\x1b[6~",
        Delete => b"\x1b[3~",
        _ => b"",
    };
    bytes.to_vec()
}

/// Bracketed paste, so the agent takes newlines as text rather than Enter.
pub fn wrap_paste(s: &str) -> Vec<u8> {
    [b"\x1b[200~".as_slice(), s.as_bytes(), b"\x1b[201~"].concat()
}
