#![allow(dead_code)]

//! Welly-style key → byte-stream mapping.
//!
//! Translates UI-neutral [`KeyEvent`](super::input::KeyEvent) values into
//! the byte sequence expected by Welly / newsmth BBS. Frontends should
//! produce `KeyEvent` and call `bytes_for_key`; they should not duplicate
//! this table.

use super::input::{Key, KeyEvent};

/// Returns the SSH byte sequence Welly produces for this key event, or
/// `None` if the key should be swallowed by the frontend (e.g. macOS Cmd
/// shortcuts).
pub fn bytes_for_key(event: KeyEvent) -> Option<Vec<u8>> {
    let m = event.modifiers;

    // macOS Cmd (without Ctrl) is a host shortcut — never forward.
    if m.command && !m.ctrl {
        return None;
    }

    if m.ctrl && !m.alt {
        return ctrl_key_bytes(event.key);
    }

    if m.alt {
        return alt_key_bytes(event.key);
    }

    match event.key {
        Key::Enter => Some(vec![b'\r']),
        Key::Backspace => Some(vec![0x7f]),
        Key::Delete => Some(b"\x1b[3~".to_vec()),
        Key::Tab => Some(vec![b'\t']),
        Key::Escape => Some(vec![0x1b]),
        Key::ArrowUp => Some(b"\x1b[A".to_vec()),
        Key::ArrowDown => Some(b"\x1b[B".to_vec()),
        Key::ArrowRight => Some(b"\x1b[C".to_vec()),
        Key::ArrowLeft => Some(b"\x1b[D".to_vec()),
        Key::Home => Some(b"\x1b[1~".to_vec()),
        Key::End => Some(b"\x1b[4~".to_vec()),
        Key::PageUp => Some(b"\x1b[5~".to_vec()),
        Key::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}

fn ctrl_key_bytes(key: Key) -> Option<Vec<u8>> {
    let byte = match key {
        Key::Letter('A') => 0x01,
        Key::Letter('B') => 0x02,
        Key::Letter('C') => 0x03,
        Key::Letter('D') => 0x04,
        Key::Letter('E') => 0x05,
        Key::Letter('F') => 0x06,
        Key::Letter('G') => 0x07,
        Key::Letter('H') | Key::Backspace => 0x08,
        Key::Letter('I') | Key::Tab => 0x09,
        Key::Letter('J') => 0x0a,
        Key::Letter('K') => 0x0b,
        Key::Letter('L') => 0x0c,
        Key::Letter('M') | Key::Enter => 0x0d,
        Key::Letter('N') => 0x0e,
        Key::Letter('O') => 0x0f,
        Key::Letter('P') => 0x10,
        Key::Letter('Q') => 0x11,
        Key::Letter('R') => 0x12,
        Key::Letter('S') => 0x13,
        Key::Letter('T') => 0x14,
        Key::Letter('U') => 0x15,
        Key::Letter('V') => 0x16,
        Key::Letter('W') => 0x17,
        Key::Letter('X') => 0x18,
        Key::Letter('Y') => 0x19,
        Key::Letter('Z') => 0x1a,
        Key::OpenBracket | Key::Escape => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        Key::Digit(6) => 0x1e,
        Key::Minus => 0x1f,
        _ => return None,
    };
    Some(vec![byte])
}

fn alt_key_bytes(key: Key) -> Option<Vec<u8>> {
    match key {
        Key::ArrowUp => Some(b"\x1b[5~".to_vec()),
        Key::ArrowDown => Some(b"\x1b[6~".to_vec()),
        Key::ArrowRight => Some(b"\x1b[4~".to_vec()),
        Key::ArrowLeft => Some(b"\x1b[1~".to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::input::Modifiers;

    fn kev(key: Key, modifiers: Modifiers) -> KeyEvent {
        KeyEvent { key, modifiers }
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }
    }

    fn alt() -> Modifiers {
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    }

    fn cmd() -> Modifiers {
        Modifiers {
            command: true,
            ..Modifiers::default()
        }
    }

    #[test]
    fn arrows_map_to_welly_csi_sequences() {
        assert_eq!(
            bytes_for_key(kev(Key::ArrowUp, Modifiers::default())),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            bytes_for_key(kev(Key::ArrowLeft, Modifiers::default())),
            Some(b"\x1b[D".to_vec())
        );
    }

    #[test]
    fn alt_arrows_map_to_welly_navigation() {
        assert_eq!(
            bytes_for_key(kev(Key::ArrowUp, alt())),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            bytes_for_key(kev(Key::ArrowDown, alt())),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            bytes_for_key(kev(Key::ArrowRight, alt())),
            Some(b"\x1b[4~".to_vec())
        );
        assert_eq!(
            bytes_for_key(kev(Key::ArrowLeft, alt())),
            Some(b"\x1b[1~".to_vec())
        );
    }

    #[test]
    fn ctrl_letter_sends_ascii_control_byte() {
        assert_eq!(
            bytes_for_key(kev(Key::Letter('G'), ctrl())),
            Some(vec![0x07])
        );
        assert_eq!(
            bytes_for_key(kev(Key::Letter('K'), ctrl())),
            Some(vec![0x0b])
        );
        assert_eq!(bytes_for_key(kev(Key::Enter, ctrl())), Some(vec![0x0d]));
    }

    #[test]
    fn command_only_returns_none() {
        assert_eq!(bytes_for_key(kev(Key::Letter('G'), cmd())), None);
    }
}
