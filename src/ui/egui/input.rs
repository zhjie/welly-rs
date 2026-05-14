//! egui → backend input translation.

use crate::backend::input::{InputEvent, Key, KeyEvent, Modifiers, MouseEvent, WheelDir};
use crate::backend::{keys, mouse};
use eframe::egui;
use encoding_rs::GB18030;

/// **Transitional** (kept until E2): translate an egui event directly
/// into the bytes that should be sent to SSH. Internally routes through
/// `input_event_for_egui_event`; existing App call sites use this
/// wrapper. After E2 lands, App calls `input_event_for_egui_event` and
/// hands the result to `Backend::send_input`, and this wrapper is
/// deleted.
pub fn bytes_for_egui_event(event: &egui::Event) -> Option<Vec<u8>> {
    let input = input_event_for_egui_event(event)?;
    match input {
        InputEvent::Key(k) => keys::bytes_for_key(k),
        InputEvent::Mouse(MouseEvent::Wheel(d)) => Some(mouse::bytes_for_wheel(d)),
        InputEvent::Mouse(MouseEvent::Click(_)) => None, // App handles clicks separately
        InputEvent::Paste(text) => {
            if text.is_empty() || text.chars().any(char::is_control) {
                None
            } else {
                let (b, _, _) = GB18030.encode(&text);
                Some(b.into_owned())
            }
        }
        _ => None,
    }
}

/// Translate an egui event to the corresponding `InputEvent`, or `None`
/// if it isn't a forwardable input. Mouse clicks are NOT handled here —
/// they require cell-pixel-size context the egui layer holds, so call
/// `mouse_click_event` for those.
pub fn input_event_for_egui_event(event: &egui::Event) -> Option<InputEvent> {
    match event {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => Some(InputEvent::Key(KeyEvent {
            key: translate_key(*key),
            modifiers: translate_modifiers(*modifiers),
        })),
        egui::Event::Text(text) => paste_event(text),
        egui::Event::Ime(egui::ImeEvent::Commit(text)) => paste_event(text),
        egui::Event::MouseWheel { delta, .. } => {
            wheel_dir_for_delta(*delta).map(|d| InputEvent::Mouse(MouseEvent::Wheel(d)))
        }
        _ => None,
    }
}

/// Build an `InputEvent::Mouse(Click)` from a grid-space click. The egui
/// caller is responsible for converting screen → grid (it owns the cell
/// pixel size); backend then resolves the click against terminal state.
#[allow(dead_code)]
pub fn mouse_click_event(grid: crate::backend::input::GridPoint) -> InputEvent {
    InputEvent::Mouse(MouseEvent::Click(grid))
}

fn paste_event(text: &str) -> Option<InputEvent> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(InputEvent::Paste(text.to_owned()))
}

fn wheel_dir_for_delta(delta: egui::Vec2) -> Option<WheelDir> {
    if delta.y.abs() >= delta.x.abs() && delta.y != 0.0 {
        Some(if delta.y > 0.0 {
            WheelDir::Up
        } else {
            WheelDir::Down
        })
    } else if delta.x != 0.0 {
        Some(if delta.x > 0.0 {
            WheelDir::Left
        } else {
            WheelDir::Right
        })
    } else {
        None
    }
}

pub fn translate_modifiers(m: egui::Modifiers) -> Modifiers {
    Modifiers {
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
        command: m.command,
    }
}

pub fn translate_key(key: egui::Key) -> Key {
    use egui::Key as EK;
    match key {
        EK::Enter => Key::Enter,
        EK::Backspace => Key::Backspace,
        EK::Delete => Key::Delete,
        EK::Tab => Key::Tab,
        EK::Escape => Key::Escape,
        EK::ArrowUp => Key::ArrowUp,
        EK::ArrowDown => Key::ArrowDown,
        EK::ArrowLeft => Key::ArrowLeft,
        EK::ArrowRight => Key::ArrowRight,
        EK::Home => Key::Home,
        EK::End => Key::End,
        EK::PageUp => Key::PageUp,
        EK::PageDown => Key::PageDown,
        EK::OpenBracket => Key::OpenBracket,
        EK::CloseBracket => Key::CloseBracket,
        EK::Backslash => Key::Backslash,
        EK::Minus => Key::Minus,
        EK::A => Key::Letter('A'),
        EK::B => Key::Letter('B'),
        EK::C => Key::Letter('C'),
        EK::D => Key::Letter('D'),
        EK::E => Key::Letter('E'),
        EK::F => Key::Letter('F'),
        EK::G => Key::Letter('G'),
        EK::H => Key::Letter('H'),
        EK::I => Key::Letter('I'),
        EK::J => Key::Letter('J'),
        EK::K => Key::Letter('K'),
        EK::L => Key::Letter('L'),
        EK::M => Key::Letter('M'),
        EK::N => Key::Letter('N'),
        EK::O => Key::Letter('O'),
        EK::P => Key::Letter('P'),
        EK::Q => Key::Letter('Q'),
        EK::R => Key::Letter('R'),
        EK::S => Key::Letter('S'),
        EK::T => Key::Letter('T'),
        EK::U => Key::Letter('U'),
        EK::V => Key::Letter('V'),
        EK::W => Key::Letter('W'),
        EK::X => Key::Letter('X'),
        EK::Y => Key::Letter('Y'),
        EK::Z => Key::Letter('Z'),
        EK::Num0 => Key::Digit(0),
        EK::Num1 => Key::Digit(1),
        EK::Num2 => Key::Digit(2),
        EK::Num3 => Key::Digit(3),
        EK::Num4 => Key::Digit(4),
        EK::Num5 => Key::Digit(5),
        EK::Num6 => Key::Digit(6),
        EK::Num7 => Key::Digit(7),
        EK::Num8 => Key::Digit(8),
        EK::Num9 => Key::Digit(9),
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::input::{InputEvent, MouseEvent};

    fn key_event(ev: &egui::Event) -> Option<KeyEvent> {
        match input_event_for_egui_event(ev) {
            Some(InputEvent::Key(k)) => Some(k),
            _ => None,
        }
    }

    fn paste_text(ev: &egui::Event) -> Option<String> {
        match input_event_for_egui_event(ev) {
            Some(InputEvent::Paste(t)) => Some(t),
            _ => None,
        }
    }

    fn wheel_dir(ev: &egui::Event) -> Option<WheelDir> {
        match input_event_for_egui_event(ev) {
            Some(InputEvent::Mouse(MouseEvent::Wheel(d))) => Some(d),
            _ => None,
        }
    }

    #[test]
    fn ime_commit_emits_paste_event() {
        let event = egui::Event::Ime(egui::ImeEvent::Commit("中文".to_owned()));
        assert_eq!(paste_text(&event), Some("中文".to_owned()));
    }

    #[test]
    fn text_with_control_chars_is_dropped() {
        assert_eq!(
            input_event_for_egui_event(&egui::Event::Text("\u{0b}".to_owned())),
            None
        );
        assert_eq!(
            input_event_for_egui_event(&egui::Event::Ime(egui::ImeEvent::Commit(
                "\u{0b}".to_owned()
            ))),
            None
        );
    }

    #[test]
    fn ctrl_letter_translates_to_key_event_with_ctrl_modifier() {
        let event = egui::Event::Key {
            key: egui::Key::G,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        let k = key_event(&event).unwrap();
        assert_eq!(k.key, Key::Letter('G'));
        assert!(k.modifiers.ctrl);
    }

    #[test]
    fn alt_arrow_translates_to_key_event_with_alt_modifier() {
        let make = |k: egui::Key| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        let k = key_event(&make(egui::Key::ArrowUp)).unwrap();
        assert_eq!(k.key, Key::ArrowUp);
        assert!(k.modifiers.alt);
    }

    #[test]
    fn vertical_wheel_maps_to_wheel_dir() {
        assert_eq!(
            wheel_dir(&egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 12.0),
                modifiers: egui::Modifiers::default(),
            }),
            Some(WheelDir::Up)
        );
        assert_eq!(
            wheel_dir(&egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -12.0),
                modifiers: egui::Modifiers::default(),
            }),
            Some(WheelDir::Down)
        );
    }
}
