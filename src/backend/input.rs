#![allow(dead_code)]

//! UI-toolkit-neutral input events.
//!
//! Frontends translate their native events (`egui::Event`, `gpui::KeyEvent`,
//! …) into these types and hand them to `Backend::send_input`. Backend
//! converts to the byte stream expected by the BBS via `backend::keys` and
//! `backend::mouse`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Enter,
    Backspace,
    Delete,
    Tab,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    /// ASCII letter A–Z (uppercase form).
    Letter(char),
    /// Digit 0–9.
    Digit(u8),
    /// `[`, `]`, `\`, `-`, `=` — used by Ctrl-key sequences.
    OpenBracket,
    CloseBracket,
    Backslash,
    Minus,
    /// Catch-all for keys we don't translate (function keys, etc.).
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Ctrl modifier. On macOS this is the literal Control key; on
    /// Windows/Linux it overlaps with `command` per egui's convention.
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// macOS ⌘ (egui's `command`, only true on mac when literal Cmd is
    /// pressed; Windows/Linux egui sets `command == ctrl`).
    pub command: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelDir {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPoint {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    Wheel(WheelDir),
    /// Click at a terminal grid coordinate; backend decides if this is an
    /// entry-row click vs. background navigation.
    Click(GridPoint),
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    /// Already-decoded text from IME commit, plain typing, or clipboard
    /// paste. Backend encodes to GB18030 before sending.
    Paste(String),
    Resize {
        cols: u16,
        rows: u16,
    },
    Reconnect,
    Shutdown,
}
