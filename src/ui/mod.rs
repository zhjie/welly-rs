//! Frontend (UI) modules.
//!
//! Each frontend translates user input into [`backend::input::InputEvent`]
//! and renders [`backend::snapshot::TerminalSnapshot`]. The egui frontend
//! is the default; future frontends (gpui) will live alongside.

pub mod egui;
