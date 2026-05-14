//! Backend modules: terminal model, SSH transport, ANSI parsing.
//!
//! These modules are UI-toolkit-neutral. Frontends (`src/ui/egui/`,
//! future `src/ui/gpui/`) consume them through the types re-exported here.

// Submodules are added in subsequent tasks (B2..B6, C1..C4, E2).
pub mod ansi_parser;
pub mod attachment;
pub mod cell;
pub mod input;
pub mod keys;
pub mod mouse;
pub mod snapshot;
pub mod ssh;
pub mod terminal;

#[allow(unused_imports)]
pub use snapshot::TerminalSnapshot;
