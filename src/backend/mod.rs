//! Backend modules: terminal model, SSH transport, ANSI parsing.
//!
//! These modules are UI-toolkit-neutral. Frontends (`src/ui/egui/`,
//! future `src/ui/gpui/`) consume them through the types re-exported here.

// Submodules are added in subsequent tasks (B2..B6, C1..C4, E2).
pub mod cell;
pub mod terminal;
