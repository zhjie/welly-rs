# Phase 1: Backend Boundary & main.rs Slimming — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize `welly-rs` source into `src/backend/` (UI-neutral logic), `src/ui/egui/` (egui-specific code), and `src/app.rs` (event-loop glue), shrinking `src/main.rs` from 2455 to ≤200 lines without changing runtime behavior. This produces the `Backend` API surface that Phase 2's gpui frontend will later consume.

**Architecture:**
- All terminal model / SSH / parsing / attachment-detection code lives in `src/backend/`.
- Welly's UI-neutral input event types (`KeyEvent`, `MouseEvent`, `InputEvent`) live in `src/backend/input.rs`; the Welly-style key/mouse byte mappings live in `src/backend/keys.rs` + `src/backend/mouse.rs`. **All `InputEvent` → SSH byte translation lives inside `Backend::send_input`** — including text/IME GB18030 encoding and mouse-grid-click resolution. The egui frontend produces `InputEvent` values and does **not** call `backend::keys` / `backend::mouse` directly; this is what makes the boundary real for Phase 2's gpui frontend.
- Frontends consume terminal state through `Backend::with_snapshot(|s: &TerminalSnapshot| ...)`. Backend's `Arc<Mutex<Terminal>>` is a **private** field. The egui rendering code accepts `&TerminalSnapshot`, not `&Terminal`. This is the second half of the Phase 2-ready boundary.
- All egui-specific code (rendering, font setup, event translation, selection, URL detection, attachment button) lives in `src/ui/egui/`.
- `src/app.rs` holds the `App` struct (currently in `main.rs`) and `impl eframe::App for App`. `App::new(cc: &eframe::CreationContext<'_>)` builds the `Backend` with the real `egui::Context`-derived notify callback at startup — no two-phase init via `Default`. App is egui-coupled in Phase 1; Phase 2 will revisit when gpui frontend arrives.
- `src/main.rs` is reduced to: constants for window size, `main()` startup, and the eframe creation closure that builds `App::new(cc)`.

**Tech Stack:** Rust 2021, egui 0.29 / eframe, tokio, russh, encoding_rs (GB18030), unicode-width, fontdb, crossbeam-channel. No new dependencies.

**Risk control:**
- Each task = one git commit. After every task: `cargo build`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings` must pass.
- After Stage D and Stage E human runs `cargo run` and confirms BBS login + screen + Welly key navigation works.
- No-behavior-change commits use commit subjects starting with `refactor:`.
- `git mv` (not delete-and-add) for every file relocation so `git log --follow` keeps history.

---

## Pre-flight (do this once, before Task 1)

- [ ] **Step 1: Confirm clean working tree**

```bash
git status
```
Expected: `nothing to commit, working tree clean` (the spec doc commit `6c693af` is already in.)

- [ ] **Step 2: Establish green baseline**

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build
```
Expected: all four pass. `cargo test` reports `66 passed`.

- [ ] **Step 3: Manual smoke (optional, human only)**

Run `cargo run`, log into bbs.newsmth.net, confirm the screen renders, arrow keys / Alt-arrows / mouse wheel / selection / Cmd+C / double-click URL all work as today. Capture a screenshot for visual reference.

If any step above fails, STOP and report — do not start the refactor on a broken baseline.

---

## Stage A — Decouple `cell.rs` and `ssh.rs` from egui (in place, no moves yet)

### Task A1: Replace `Color::egui_color()` with UI-neutral `Color::rgb()`

**Files:**
- Modify: `src/cell.rs`
- Modify: `src/main.rs` (call sites)

`cell.rs` currently returns `egui::Color32` directly, which prevents moving it under `src/backend/`. Strip the egui dependency by returning `(u8, u8, u8)` and let `main.rs` convert.

- [ ] **Step 1: Rewrite `cell.rs::Color` methods to UI-neutral form**

In `src/cell.rs`, replace the entire `impl Color { ... }` block (lines 51–112) with:

```rust
impl Color {
    /// Returns the 8-bit RGB triple this color renders as. UI-toolkit-neutral.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Default => (255, 255, 255),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::White => (229, 229, 229),
            Color::BrightBlack => (127, 127, 127),
            Color::BrightRed => (255, 0, 0),
            Color::BrightGreen => (0, 255, 0),
            Color::BrightYellow => (255, 255, 0),
            Color::BrightBlue => (92, 92, 255),
            Color::BrightMagenta => (255, 0, 255),
            Color::BrightCyan => (0, 255, 255),
            Color::BrightWhite => (255, 255, 255),
            Color::Indexed(i) => Self::indexed_rgb(i),
            Color::Rgb(r, g, b) => (r, g, b),
        }
    }

    fn indexed_rgb(index: u8) -> (u8, u8, u8) {
        match index {
            0..=15 => {
                let colors = [
                    Color::Black,
                    Color::Red,
                    Color::Green,
                    Color::Yellow,
                    Color::Blue,
                    Color::Magenta,
                    Color::Cyan,
                    Color::White,
                    Color::BrightBlack,
                    Color::BrightRed,
                    Color::BrightGreen,
                    Color::BrightYellow,
                    Color::BrightBlue,
                    Color::BrightMagenta,
                    Color::BrightCyan,
                    Color::BrightWhite,
                ];
                colors[index as usize].rgb()
            }
            16..=231 => {
                let idx = index - 16;
                let r = (idx / 36) * 51;
                let g = ((idx % 36) / 6) * 51;
                let b = (idx % 6) * 51;
                (r, g, b)
            }
            232..=255 => {
                let gray = (index - 232) * 10 + 8;
                (gray, gray, gray)
            }
        }
    }
}
```

This file should now compile without any `egui::` references. Run `grep -n "egui" src/cell.rs` — expect zero matches.

- [ ] **Step 2: Add `color_to_egui` adapter in main.rs**

In `src/main.rs`, near the top after the `use` block (around line 80), add:

```rust
fn color_to_egui(color: cell::Color) -> egui::Color32 {
    let (r, g, b) = color.rgb();
    egui::Color32::from_rgb(r, g, b)
}
```

- [ ] **Step 3: Update call sites in main.rs**

Replace every occurrence of `.egui_color()` with `color_to_egui(...)`. The current call sites are in `cell_foreground_color`, `foreground_color`, `background_color` (around lines 2211–2236). Use Edit with `replace_all` to change `.egui_color()` → `).into()` is not feasible — do it manually:

Find in `src/main.rs`:
```rust
brighten(bg, cell.bold).egui_color()
```
Replace with:
```rust
color_to_egui(brighten(bg, cell.bold))
```

Find:
```rust
brighten(base, bold).egui_color()
```
Replace with:
```rust
color_to_egui(brighten(base, bold))
```

Find:
```rust
_ => color.egui_color(),
```
Replace with:
```rust
_ => color_to_egui(color),
```

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass, 66 tests still green.

- [ ] **Step 5: Commit**

```bash
git add src/cell.rs src/main.rs
git commit -m "refactor: replace Color::egui_color() with UI-neutral Color::rgb()

Prepares cell.rs to move under src/backend/ by removing its dependency on
egui::Color32. Egui conversion is done by main.rs::color_to_egui at the
single call site (paint code)."
```

---

### Task A2: Decouple `SshClient::connect` from `egui::Context`

**Files:**
- Modify: `src/ssh.rs`
- Modify: `src/main.rs:445-475` (`start_connect`)

`SshClient::connect` currently takes `eframe::egui::Context` and calls `ctx.request_repaint()` from the SSH read loop. Replace with a UI-neutral callback so `ssh.rs` can move under `src/backend/`.

- [ ] **Step 1: Change `SshClient::connect` signature**

In `src/ssh.rs`, replace the `pub async fn connect(...)` signature (line 21–26):

```rust
pub async fn connect(
    settings: ConnectionSettings,
    terminal: Arc<Mutex<Terminal>>,
    parser: Arc<Mutex<AnsiParser>>,
    notify: Arc<dyn Fn() + Send + Sync>,
) -> Result<Arc<Self>, russh::Error> {
```

Replace the two `ctx.request_repaint();` calls (lines 69 and 79) with `notify();`. The captured `notify` must be cloned for the tokio::spawn closure: rename the outer parameter to `notify` and clone it where needed. Specifically, in the `tokio::spawn(async move { ... })` block (currently captures `ctx` via `move`), capture `notify` instead. Where the loop body calls `ctx.request_repaint()`, call `notify()`. Since `notify` is `Arc<dyn Fn() + Send + Sync>`, it's `Clone` and moves cleanly into the closure.

Remove the `use eframe;` / `eframe::egui::Context` reference from the top of `src/ssh.rs`. Run `grep -n "egui\|eframe" src/ssh.rs` — expect zero matches.

- [ ] **Step 2: Update `start_connect` in main.rs**

In `src/main.rs` (around lines 445–475), update the call site:

```rust
fn start_connect(&mut self, ctx: &egui::Context) {
    self.connected = false;
    self.auto_connect_attempted = true;
    self.ssh_client = None;
    self.terminal.lock().unwrap().clear_all();
    self.parser = Arc::new(Mutex::new(AnsiParser::new()));

    let terminal = Arc::clone(&self.terminal);
    let parser = Arc::clone(&self.parser);
    let settings = self.settings.clone();
    let ctx_for_notify = ctx.clone();
    let notify: Arc<dyn Fn() + Send + Sync> =
        Arc::new(move || ctx_for_notify.request_repaint());
    let (tx, rx): (ConnectSender, ConnectReceiver) = crossbeam_channel::bounded(1);
    self.connect_rx = Some(rx);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            match SshClient::connect(settings, terminal, parser, notify).await {
                Ok(client) => {
                    log::info!("SSH connected successfully");
                    let _ = tx.send(Ok(client));
                    std::future::pending::<()>().await;
                }
                Err(e) => {
                    log::error!("SSH error: {}", e);
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });
    });
}
```

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass, 66 tests still green.

- [ ] **Step 4: Commit**

```bash
git add src/ssh.rs src/main.rs
git commit -m "refactor: replace egui::Context with Arc<dyn Fn()> notify in SshClient

Removes the only remaining egui coupling in ssh.rs. main.rs constructs the
notify closure from a cloned egui::Context, keeping repaint behavior
identical."
```

---

## Stage B — Create `src/backend/` module and move files

After Stage B, the directory tree is:
```
src/
  main.rs
  config.rs
  backend/
    mod.rs
    cell.rs
    terminal.rs
    ansi_parser.rs
    attachment.rs
    ssh.rs
```

Each file move is its own commit so `git bisect` can pinpoint a regression to a specific module.

### Task B1: Create empty `src/backend/mod.rs`

**Files:**
- Create: `src/backend/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create backend module skeleton**

Create `src/backend/mod.rs` with:

```rust
//! Backend modules: terminal model, SSH transport, ANSI parsing.
//!
//! These modules are UI-toolkit-neutral. Frontends (`src/ui/egui/`,
//! future `src/ui/gpui/`) consume them through the types re-exported here.

// Submodules are added in subsequent tasks (B2..B6, C1..C4, E2).
```

- [ ] **Step 2: Register the module in main.rs**

In `src/main.rs`, replace the current six top-level `mod` lines (around lines 64–69):

```rust
mod ansi_parser;
mod attachment;
mod cell;
mod config;
mod ssh;
mod terminal;
```

with:

```rust
mod ansi_parser;
mod attachment;
mod backend;
mod cell;
mod config;
mod ssh;
mod terminal;
```

(Yes, both `mod backend;` and the existing six mods coexist for now. Files are moved one at a time in B2..B6.)

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass. New `mod backend;` is empty so this is a no-op build.

- [ ] **Step 4: Commit**

```bash
git add src/backend/mod.rs src/main.rs
git commit -m "refactor: add empty src/backend/ module skeleton

Prepares the directory for subsequent file moves (cell, terminal,
ansi_parser, attachment, ssh)."
```

---

### Task B2: Move `cell.rs` to `src/backend/cell.rs`

**Files:**
- Move: `src/cell.rs` → `src/backend/cell.rs`
- Modify: `src/backend/mod.rs`, `src/main.rs`, `src/terminal.rs`

- [ ] **Step 1: git mv the file**

```bash
git mv src/cell.rs src/backend/cell.rs
```

- [ ] **Step 2: Register submodule in `src/backend/mod.rs`**

Add a line so `mod.rs` reads:

```rust
//! Backend modules: terminal model, SSH transport, ANSI parsing.
//!
//! These modules are UI-toolkit-neutral. Frontends (`src/ui/egui/`,
//! future `src/ui/gpui/`) consume them through the types re-exported here.

pub mod cell;
```

- [ ] **Step 3: Drop the top-level `mod cell;` in main.rs**

In `src/main.rs`, remove the line `mod cell;`. Update the existing `use` block (and any inline `cell::` references) so the path resolves through `backend`:

- Add `use backend::cell;` near the other use statements.
- The expression `cell::Color`, `cell::Cell`, etc. continues to compile because of that `use`.
- `font_for_cell(cell: &cell::Cell)` keeps working — it referenced the local `cell` module name, now re-bound by `use`.

If any code does `crate::cell::...`, change it to `crate::backend::cell::...`.

- [ ] **Step 4: Update `src/terminal.rs`'s import**

`src/terminal.rs` line 1 is `use crate::cell::{Cell, Color};`. Change to:

```rust
use crate::backend::cell::{Cell, Color};
```

- [ ] **Step 5: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass, 66 tests still green.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move cell.rs into src/backend/

git mv preserves history. terminal.rs and main.rs imports updated to the
new path."
```

---

### Task B3: Move `terminal.rs` to `src/backend/terminal.rs`

**Files:**
- Move: `src/terminal.rs` → `src/backend/terminal.rs`
- Modify: `src/backend/mod.rs`, `src/main.rs`, `src/ansi_parser.rs`, `src/ssh.rs`

- [ ] **Step 1: git mv**

```bash
git mv src/terminal.rs src/backend/terminal.rs
```

- [ ] **Step 2: Register submodule**

In `src/backend/mod.rs`, add `pub mod terminal;` under `pub mod cell;`. While here, switch the existing `use crate::backend::cell::...` inside the moved `terminal.rs` to a relative path: `use super::cell::{Cell, Color};` (cleaner inside a sibling module).

- [ ] **Step 3: Drop `mod terminal;` from main.rs, fix imports**

In `src/main.rs`:
- Remove the line `mod terminal;`.
- Change `use terminal::Terminal;` to `use backend::terminal::Terminal;`.

- [ ] **Step 4: Fix imports in remaining backend siblings**

In `src/ansi_parser.rs` (still at top level for now), change any `use crate::terminal::...` to `use crate::backend::terminal::...`. Same in `src/ssh.rs`: `use crate::terminal::Terminal;` → `use crate::backend::terminal::Terminal;`.

- [ ] **Step 5: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: move terminal.rs into src/backend/

Updates ansi_parser.rs, ssh.rs, and main.rs imports to the new path."
```

---

### Task B4: Move `ansi_parser.rs` to `src/backend/ansi_parser.rs`

**Files:**
- Move: `src/ansi_parser.rs` → `src/backend/ansi_parser.rs`
- Modify: `src/backend/mod.rs`, `src/main.rs`, `src/ssh.rs`

- [ ] **Step 1: git mv**

```bash
git mv src/ansi_parser.rs src/backend/ansi_parser.rs
```

- [ ] **Step 2: Register submodule, fix internal imports**

In `src/backend/mod.rs`, add `pub mod ansi_parser;`. Inside the moved `ansi_parser.rs`, change `use crate::backend::terminal::...` to `use super::terminal::...`. Same for any reference to `crate::backend::cell::...` → `super::cell::...`.

- [ ] **Step 3: Drop `mod ansi_parser;` from main.rs, fix imports**

In `src/main.rs`:
- Remove `mod ansi_parser;`.
- Change `use ansi_parser::AnsiParser;` to `use backend::ansi_parser::AnsiParser;`.

In `src/ssh.rs`: `use crate::ansi_parser::AnsiParser;` → `use crate::backend::ansi_parser::AnsiParser;`.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move ansi_parser.rs into src/backend/

Updates ssh.rs and main.rs imports."
```

---

### Task B5: Move `attachment.rs` to `src/backend/attachment.rs`

**Files:**
- Move: `src/attachment.rs` → `src/backend/attachment.rs`
- Modify: `src/backend/mod.rs`, `src/main.rs`

`attachment.rs` has no internal `crate::` references, so this is a pure move + path fixup in main.

- [ ] **Step 1: git mv**

```bash
git mv src/attachment.rs src/backend/attachment.rs
```

- [ ] **Step 2: Register submodule**

In `src/backend/mod.rs`, add `pub mod attachment;`.

- [ ] **Step 3: Fix main.rs imports**

In `src/main.rs`:
- Remove `mod attachment;`.
- Change `use attachment::{parse_image_attachments, ImageAttachment};` to `use backend::attachment::{parse_image_attachments, ImageAttachment};`.

A test inside main's tests module references `crate::attachment::ImageAttachment` — update to `crate::backend::attachment::ImageAttachment`.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move attachment.rs into src/backend/"
```

---

### Task B6: Move `ssh.rs` to `src/backend/ssh.rs`

**Files:**
- Move: `src/ssh.rs` → `src/backend/ssh.rs`
- Modify: `src/backend/mod.rs`, `src/main.rs`

- [ ] **Step 1: git mv**

```bash
git mv src/ssh.rs src/backend/ssh.rs
```

- [ ] **Step 2: Register submodule, fix internal imports**

In `src/backend/mod.rs`, add `pub mod ssh;`. Inside the moved `ssh.rs`:
- `use crate::backend::ansi_parser::AnsiParser;` → `use super::ansi_parser::AnsiParser;`
- `use crate::backend::terminal::Terminal;` → `use super::terminal::Terminal;`
- `use crate::config::ConnectionSettings;` stays as is (config is still at top level).

- [ ] **Step 3: Fix main.rs imports**

In `src/main.rs`:
- Remove `mod ssh;`.
- Change `use ssh::{is_channel_closed_error, SshClient};` to `use backend::ssh::{is_channel_closed_error, SshClient};`.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move ssh.rs into src/backend/

All five non-config backend files now live under src/backend/. main.rs
imports them through that path."
```

After B6 the structure is:
```
src/
  main.rs        # still ~2400 lines, unchanged content
  config.rs
  backend/
    mod.rs
    cell.rs
    terminal.rs
    ansi_parser.rs
    attachment.rs
    ssh.rs
```

Manual smoke (recommended): run `cargo run` and confirm the BBS screen + keyboard + mouse still behave identically to baseline. Stage C–E will not change `main.rs` line count until Stage D — but we now have a clean boundary to start defining the backend API surface.

---

## Stage C — Define UI-neutral types in `src/backend/`

### Task C1: Create `src/backend/input.rs` with UI-neutral event types

**Files:**
- Create: `src/backend/input.rs`
- Modify: `src/backend/mod.rs`

These types describe input *after* the egui-specific layer has translated it, and *before* `backend::keys` converts them to SSH bytes. They are deliberately small — only what current welly-rs key/mouse logic actually needs.

- [ ] **Step 1: Write the new module**

Create `src/backend/input.rs`:

```rust
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

#[derive(Clone, Debug)]
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
```

- [ ] **Step 2: Register module**

In `src/backend/mod.rs`, add `pub mod input;`.

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass. `cargo build` will emit `unused` warnings *only if* clippy is not given `--all-targets`; with `--all-targets --all-features` the new types are considered library-public and don't warn. If a warning appears (`dead_code`), add `#[allow(dead_code)]` at the module top — they'll be wired up in C2 and beyond.

- [ ] **Step 4: Commit**

```bash
git add src/backend/input.rs src/backend/mod.rs
git commit -m "feat(backend): add UI-neutral input event types

KeyEvent / MouseEvent / InputEvent are the boundary every frontend (egui
today, gpui later) translates its native events into. No call sites yet;
keys.rs in next commit consumes them."
```

---

### Task C2: Create `src/backend/keys.rs` (Welly key escape mapping)

**Files:**
- Create: `src/backend/keys.rs`
- Modify: `src/backend/mod.rs`

Lifts the Welly-style key→SSH-bytes table out of `main.rs::key_event_to_bytes` etc. into a backend-owned module that consumes `backend::input::KeyEvent`. The egui-side `terminal_event_to_bytes` (Task D2) becomes a thin adapter: `egui::Event` → `KeyEvent` → `keys::bytes_for_key`.

- [ ] **Step 1: Write the keys module**

Create `src/backend/keys.rs`:

```rust
//! Welly-style key → byte-stream mapping.
//!
//! Translates UI-neutral [`KeyEvent`](super::input::KeyEvent) values into
//! the byte sequence expected by Welly / newsmth BBS. Frontends should
//! produce `KeyEvent` and call `bytes_for_key`; they should not duplicate
//! this table.

use super::input::{Key, KeyEvent, Modifiers};

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
```

- [ ] **Step 2: Register module**

In `src/backend/mod.rs`, add `pub mod keys;`.

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass, test count goes from 66 → 70 (four new tests in keys::tests). The original egui-coupled tests in `main.rs` are still there too — they will move to `ui/egui/input.rs` tests in Task D2.

- [ ] **Step 4: Commit**

```bash
git add src/backend/keys.rs src/backend/mod.rs
git commit -m "feat(backend): add keys.rs with Welly-style KeyEvent→bytes mapping

Ports the table from main.rs::key_event_to_bytes to operate on UI-neutral
KeyEvent. main.rs still has the egui-coupled version; Task D2 will replace
it with a thin egui→KeyEvent adapter."
```

---

### Task C3: Create `src/backend/mouse.rs` (mouse helpers)

**Files:**
- Create: `src/backend/mouse.rs`
- Modify: `src/backend/mod.rs`

Move `mouse_wheel_to_bytes`, `mouse_entry_click_to_bytes`, `mouse_background_navigation_bytes`, `is_mouse_entry_click_point` into the backend, retyped against `WheelDir` / `GridPoint` from `backend::input`. The egui frontend will provide a `WheelDir`/`GridPoint` and call these.

- [ ] **Step 1: Write the mouse module**

Create `src/backend/mouse.rs`:

```rust
//! Mouse → byte-stream helpers for Welly-style BBS navigation.

use super::input::{GridPoint, WheelDir};
use super::terminal::Terminal;

const TERMINAL_ROWS: usize = 24;

pub fn bytes_for_wheel(dir: WheelDir) -> Vec<u8> {
    match dir {
        WheelDir::Up => b"\x1b[A".to_vec(),
        WheelDir::Down => b"\x1b[B".to_vec(),
        WheelDir::Left => b"\x1b[D".to_vec(),
        WheelDir::Right => b"\x1b[C".to_vec(),
    }
}

pub fn bytes_for_entry_click(cursor_row: usize, target_row: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    if target_row > cursor_row {
        for _ in cursor_row..target_row {
            bytes.extend_from_slice(b"\x1b[B");
        }
    } else {
        for _ in target_row..cursor_row {
            bytes.extend_from_slice(b"\x1b[A");
        }
    }
    bytes.push(b'\r');
    bytes
}

pub fn bytes_for_background_navigation(point: GridPoint) -> Option<Vec<u8>> {
    if point.col == 0 && (3..TERMINAL_ROWS.saturating_sub(1)).contains(&point.row) {
        return Some(b"\x1b[D".to_vec());
    }

    if point.col >= 20 {
        if point.row < TERMINAL_ROWS / 2 {
            return Some(b"\x1b[5~".to_vec());
        }
        return Some(b"\x1b[6~".to_vec());
    }

    None
}

/// True if `point` lies on a visible BBS entry row (rows 3..bottom-1,
/// columns covering the visible non-blank range, padded to at least the
/// first 30 columns).
pub fn is_entry_click_point(term: &Terminal, point: GridPoint) -> bool {
    if !(3..term.rows.saturating_sub(1)).contains(&point.row) || point.col < 2 {
        return false;
    }

    let row = &term.grid[point.row];
    let Some(start) = row
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(col, cell)| (cell.width != 0 && cell.ch != ' ').then_some(col))
    else {
        return false;
    };

    let Some(end) = row.iter().enumerate().rev().find_map(|(col, cell)| {
        (cell.width != 0 && cell.ch != ' ' && cell.ch != '\0').then_some(col)
    }) else {
        return false;
    };

    let click_end = end
        .max(start.saturating_add(29))
        .min(term.cols.saturating_sub(1));
    (start..=click_end).contains(&point.col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_directions_match_welly_arrows() {
        assert_eq!(bytes_for_wheel(WheelDir::Up), b"\x1b[A".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Down), b"\x1b[B".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Left), b"\x1b[D".to_vec());
        assert_eq!(bytes_for_wheel(WheelDir::Right), b"\x1b[C".to_vec());
    }

    #[test]
    fn entry_click_moves_cursor_to_row_and_enters() {
        assert_eq!(bytes_for_entry_click(3, 6), b"\x1b[B\x1b[B\x1b[B\r".to_vec());
        assert_eq!(bytes_for_entry_click(6, 3), b"\x1b[A\x1b[A\x1b[A\r".to_vec());
    }

    #[test]
    fn background_areas_map_to_welly_navigation_keys() {
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 8, col: 0 }),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 4, col: 30 }),
            Some(b"\x1b[5~".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 18, col: 30 }),
            Some(b"\x1b[6~".to_vec())
        );
        assert_eq!(
            bytes_for_background_navigation(GridPoint { row: 8, col: 10 }),
            None
        );
    }

    #[test]
    fn entry_click_point_uses_visible_text_range() {
        let mut terminal = Terminal::new(24, 80);
        terminal.set_cursor(5, 12);
        for ch in "Re: title".chars() {
            terminal.put_char(ch);
        }

        assert!(is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 12 }
        ));
        assert!(is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 38 }
        ));
        assert!(!is_entry_click_point(
            &terminal,
            GridPoint { row: 5, col: 60 }
        ));
        assert!(!is_entry_click_point(
            &terminal,
            GridPoint { row: 2, col: 12 }
        ));
    }
}
```

- [ ] **Step 2: Register module**

In `src/backend/mod.rs`, add `pub mod mouse;`.

- [ ] **Step 3: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass, four new tests. The existing main.rs tests still exist alongside.

- [ ] **Step 4: Commit**

```bash
git add src/backend/mouse.rs src/backend/mod.rs
git commit -m "feat(backend): add mouse.rs with Welly-style mouse→bytes helpers

Ports mouse_wheel_to_bytes / mouse_entry_click_to_bytes /
mouse_background_navigation_bytes / is_mouse_entry_click_point from
main.rs to operate on UI-neutral types."
```

---

### Task C4: Create `src/backend/snapshot.rs` with `TerminalSnapshot<'a>`

**Files:**
- Create: `src/backend/snapshot.rs`
- Modify: `src/backend/mod.rs`, `src/backend/terminal.rs`

Introduces the read-only view type that frontends consume.

- [ ] **Step 1: Write snapshot.rs**

Create `src/backend/snapshot.rs`:

```rust
//! Read-only view of a `Terminal` for frontends to render.
//!
//! Borrows from the underlying [`Terminal`](super::terminal::Terminal);
//! lives only as long as the lock guard that produced it. UI-neutral —
//! no egui / gpui types in this module.

use super::cell::Cell;

pub struct TerminalSnapshot<'a> {
    pub rows: &'a [Vec<Cell>],
    pub cols: usize,
    pub row_count: usize,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl<'a> TerminalSnapshot<'a> {
    /// Returns the cell at `(row, col)` or `None` if out of bounds.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.rows.get(row)?.get(col)
    }
}
```

- [ ] **Step 2: Add `Terminal::snapshot` method**

In `src/backend/terminal.rs`, add a method inside the existing `impl Terminal` block (after `set_cursor`, before `move_cursor_up`):

```rust
pub fn snapshot(&self) -> super::snapshot::TerminalSnapshot<'_> {
    super::snapshot::TerminalSnapshot {
        rows: &self.grid,
        cols: self.cols,
        row_count: self.rows,
        cursor_row: self.cursor_row,
        cursor_col: self.cursor_col,
    }
}
```

- [ ] **Step 3: Register module**

In `src/backend/mod.rs`, add `pub mod snapshot;` and `pub use snapshot::TerminalSnapshot;` for ergonomic access.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass. No new tests yet; consumers come in Stage D.

- [ ] **Step 5: Commit**

```bash
git add src/backend/snapshot.rs src/backend/mod.rs src/backend/terminal.rs
git commit -m "feat(backend): add TerminalSnapshot<'a> read-only view

Borrows from Terminal under the existing lock. Frontends will render
through this view instead of holding a raw &Terminal."
```

---

## Stage D — Create `src/ui/egui/` and extract egui-specific code from `main.rs`

After Stage D the file tree includes `src/ui/egui/{fonts,input,selection,render}.rs`, and `main.rs` shrinks substantially. The eframe::App impl still lives in `main.rs` after Stage D — Stage E moves it.

### Task D1: Create `src/ui/` and `src/ui/egui/` skeletons; move font setup

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/egui/mod.rs`, `src/ui/egui/fonts.rs`
- Modify: `src/main.rs`

The `ui` parent module groups frontends; `ui/egui/` holds the current egui frontend. Move font loading first because it's self-contained.

- [ ] **Step 1: Create the module tree**

Create `src/ui/mod.rs`:

```rust
//! Frontend (UI) modules.
//!
//! Each frontend translates user input into [`backend::input::InputEvent`]
//! and renders [`backend::snapshot::TerminalSnapshot`]. The egui frontend
//! is the default; future frontends (gpui) will live alongside.

pub mod egui;
```

Create `src/ui/egui/mod.rs`:

```rust
//! egui / eframe frontend.

pub mod fonts;
```

- [ ] **Step 2: Move font code to `src/ui/egui/fonts.rs`**

Cut the following items from `src/main.rs` (currently around lines 27–295):
- constants: `ENGLISH_FONT_NAME`, `CHINESE_FONT_NAME`, `ENGLISH_FONT_CANDIDATES`, `CHINESE_FONT_CANDIDATES`, `CHINESE_FONT_SIZE`, `ENGLISH_FONT_SIZE`, `CHINESE_LEFT_MARGIN`, `CHINESE_TOP_MARGIN`, `ENGLISH_LEFT_MARGIN`, `ENGLISH_TOP_MARGIN`
- structs: `FontCandidate`, `LoadedFont`
- functions: `font_for_cell`, `configure_fonts`, `load_system_font_db`, `choose_font_candidate`, `load_font_candidate`, `load_candidate_font_data`, `query_font_family`

Paste them into a new file `src/ui/egui/fonts.rs`. Make the following adjustments:

- Mark `pub`: `ENGLISH_FONT_NAME`, `CHINESE_FONT_NAME`, `CHINESE_FONT_SIZE`, `ENGLISH_FONT_SIZE`, `CHINESE_LEFT_MARGIN`, `CHINESE_TOP_MARGIN`, `ENGLISH_LEFT_MARGIN`, `ENGLISH_TOP_MARGIN`, `font_for_cell`, `configure_fonts`, `choose_font_candidate`, `ENGLISH_FONT_CANDIDATES`, `CHINESE_FONT_CANDIDATES`, `FontCandidate` (rendering code in render.rs and main.rs needs them).
- Add at top:
  ```rust
  use crate::backend::cell::Cell;
  use eframe::egui;
  use egui::{FontData, FontDefinitions, FontFamily};
  use std::borrow::Cow;
  ```
- Replace `cell::Cell` in `font_for_cell`'s parameter with `Cell` (we just imported the name).

The tests for fonts (font candidate ordering tests in main.rs's tests module, currently around lines 1678–1719) need to move with the code. Locate these tests inside `mod tests` in main.rs:
  - `chinese_font_candidates_prefer_heiti_sc`
  - `chinese_font_candidates_use_shared_heiti_order`
  - `english_font_candidates_use_shared_monospace_order`
  - `english_font_candidates_do_not_include_consolas`
  - `choose_font_candidate_returns_first_available_candidate`
  - `font_sizes_follow_welly_default_proportions`
  - `ascii_cells_use_english_font_even_when_cell_is_wide`
  - `chinese_cells_use_chinese_font`
  - `text_position_uses_cell_top_margin`

Move them into a `#[cfg(test)] mod tests { ... }` block inside `src/ui/egui/fonts.rs`. Update references: `super::*` and `crate::cell::Cell` → `crate::backend::cell::Cell`. The `text_position_uses_cell_top_margin` test references `super::text_paint_position` and `super::CHINESE_TOP_MARGIN` etc. — leave the body untouched, just ensure the `use super::*;` resolves. Note: `text_paint_position` itself lives in render.rs not fonts.rs; if that test fails to compile here, KEEP IT IN MAIN.RS for now and move it in D4 with the rest of render.

- [ ] **Step 3: Wire up in main.rs**

In `src/main.rs`, add module declarations near the existing `mod backend;`:

```rust
mod ui;
```

Replace the call site `configure_fonts(&cc.egui_ctx);` with `ui::egui::fonts::configure_fonts(&cc.egui_ctx);`. Replace any reference to `font_for_cell` (in render code that's still in main.rs) with `ui::egui::fonts::font_for_cell`. Replace `CHINESE_TOP_MARGIN`, `ENGLISH_TOP_MARGIN`, etc. references with `ui::egui::fonts::CHINESE_TOP_MARGIN`, etc. (or `use ui::egui::fonts::*;` near the top of main.rs).

Top-level constants that remain in main.rs after this task: `CELL_WIDTH`, `CELL_HEIGHT`, `TERMINAL_COLS`, `TERMINAL_ROWS`, `MIN_ZOOM`, `MAX_ZOOM`, `ZOOM_STEP`, `APP_ICON_RGBA`, `ENGLISH_FONT_NAME`, `CHINESE_FONT_NAME` — wait: move `ENGLISH_FONT_NAME` / `CHINESE_FONT_NAME` to fonts.rs too. They're font-machinery-specific.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass. Tests that moved into fonts.rs should still run and pass (now under `welly_rs::ui::egui::fonts::tests`).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: extract font setup into src/ui/egui/fonts.rs

Moves all font candidate machinery (configure_fonts, FontCandidate,
font_for_cell, font sizes, margins) out of main.rs. Tests for font
ordering move with the code."
```

---

### Task D2: Extract egui event → InputEvent translation to `src/ui/egui/input.rs`

**Files:**
- Create: `src/ui/egui/input.rs`
- Modify: `src/ui/egui/mod.rs`, `src/main.rs`

The egui frontend's job in this layer: translate native `egui::Event::Key/Text/Ime/MouseWheel` into UI-neutral `backend::input::InputEvent` values. The eventual public API is `input_event_for_egui_event(&egui::Event) -> Option<InputEvent>`; gpui will write a sibling that produces the same type.

**However**, D2 runs before E2. App at D2 time still owns the SSH client and calls `send_bytes` directly — there's no `Backend::send_input` yet. So D2 also exports a transitional wrapper `bytes_for_egui_event(&egui::Event) -> Option<Vec<u8>>` that internally goes `egui::Event → InputEvent → bytes` (using `backend::keys`, `backend::mouse`, and inline GB18030 for paste). App's call sites continue to use `bytes_for_egui_event`; E2 deletes the wrapper and switches App to `input_event_for_egui_event + backend.send_input`.

Mouse click → `GridPoint` conversion stays in egui because it requires the cell pixel size (an egui rendering concern). The grid coordinate is then carried in `InputEvent::Mouse(MouseEvent::Click(grid))` and Backend (E2) resolves the bytes against the current cursor.

- [ ] **Step 1: Write `src/ui/egui/input.rs`**

```rust
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
```

The `egui::Event::MouseWheel { unit, delta, modifiers }` shape above matches egui 0.29.x.

- [ ] **Step 2: Register module**

In `src/ui/egui/mod.rs`, add `pub mod input;`.

- [ ] **Step 3: Delete the old code in main.rs and update call sites**

Remove from `src/main.rs`:
- functions: `mouse_wheel_to_bytes`, `terminal_event_to_bytes`, `text_to_bytes`, `key_event_to_bytes`, `control_key_to_bytes`, `alt_key_to_bytes`
- tests inside `mod tests` that target these functions: `mouse_wheel_vertical_maps_to_welly_arrows`, `mouse_wheel_horizontal_maps_to_welly_arrows`, `control_letter_sends_ascii_control_code`, `alt_arrows_match_welly_navigation_shortcuts`, `command_shortcuts_are_not_sent_to_bbs`, `ime_commit_sends_committed_text`, `text_events_do_not_send_control_characters` — byte-level assertions live in `backend::keys::tests` (C2) and `backend::mouse::tests` (C3); translation-only tests live in `ui::egui::input::tests`.

Update call sites in main.rs's `handle_keyboard` (these continue to take the bytes path through the transitional wrapper; E2 will rewrite them to InputEvent + backend.send_input):
- `mouse_wheel_to_bytes(delta)` → there's no longer a separate wheel function; `bytes_for_egui_event` handles `egui::Event::MouseWheel` directly. Adjust the call site to feed the entire `egui::Event` instead of a delta.
- `terminal_event_to_bytes(&event)` → `ui::egui::input::bytes_for_egui_event(&event)`

Also remove `use encoding_rs::GB18030;` from main.rs — `bytes_for_egui_event` owns paste encoding now.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass. Test count rebalances: 7 byte-level tests removed from main, ~5 translation tests added in `ui/egui/input.rs`, byte tests in `backend::keys` / `backend::mouse` already exist from C2/C3.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: introduce egui InputEvent translation

ui::egui::input now exposes input_event_for_egui_event as the future
frontend boundary. Because Backend::send_input does not exist until E2,
this commit keeps a transitional bytes_for_egui_event wrapper so the
current App can still call send_bytes without changing behavior.

Byte-level tests live with the byte conversion code in backend::keys /
backend::mouse; ui::egui::input tests cover translation only."
```

---

### Task D3: Extract selection / URL detection / `terminal_screen_text` to `src/ui/egui/selection.rs`

**Files:**
- Create: `src/ui/egui/selection.rs`
- Modify: `src/ui/egui/mod.rs`, `src/main.rs`

These are UI-side concerns (mouse-driven selection, double-click-to-open) that read backend state. They live in `ui/egui/` because Phase 2's gpui frontend may need a different selection model.

- [ ] **Step 1: Write `src/ui/egui/selection.rs`**

Move the following items from `src/main.rs` into this new file:
- structs: `GridPoint`, `Selection`, `VisibleCharCell`
- functions: `grid_index`, `selected_text`, `terminal_screen_text`, `url_at_grid_point`, `http_url_starts`, `is_trailing_url_punctuation`, `normalize_selected_url_for_open`, `trim_url_trailing_punctuation`, `is_scheme_url`, `looks_like_scheme_less_url`, `is_valid_domain_label`, `pos_to_grid_point`

Adapt signatures: functions that previously took `&Terminal` now take `&TerminalSnapshot<'_>`. Specifically:
- `selected_text(snapshot: &TerminalSnapshot<'_>, selection: &Selection) -> String`
- `terminal_screen_text(snapshot: &TerminalSnapshot<'_>) -> String`
- `url_at_grid_point(snapshot: &TerminalSnapshot<'_>, point: GridPoint) -> Option<String>`

Make them `pub` as needed (whatever main.rs's App impl ends up calling). Update the file head:

```rust
use crate::backend::snapshot::TerminalSnapshot;
use eframe::egui;

// Terminal grid dimensions are duplicated from main.rs constants to keep
// this module from depending on main. They are an invariant of the Welly
// experience, not a tunable.
const TERMINAL_COLS: usize = 80;
```

Replace any `TERMINAL_COLS` reference inside the moved code with the local copy. Tests in this module construct a `Terminal` to drive snapshot creation: `let term = Terminal::new(24, 80); /* mutate */; let snap = term.snapshot(); selected_text(&snap, &sel)`.

Move the corresponding tests too:
- `selection_extracts_single_line_text`
- `selection_extracts_multiline_text_and_trims_right_spaces`
- `selection_skips_double_width_continuation_cells`
- `url_at_grid_point_detects_http_url_on_same_line`
- `url_at_grid_point_trims_trailing_sentence_punctuation`
- `url_at_grid_point_ignores_non_url_cells`
- `selected_url_without_scheme_gets_https_scheme`
- `selected_url_keeps_existing_http_scheme`
- `selected_url_rejects_plain_words_and_email_addresses`

Inside `#[cfg(test)] mod tests` of the new file. Adjust `use crate::terminal::Terminal;` → `use crate::backend::terminal::Terminal;` and any `super::` references stay.

- [ ] **Step 2: Register module**

In `src/ui/egui/mod.rs`, add `pub mod selection;`.

- [ ] **Step 3: Update call sites in main.rs**

Where main.rs's App impl used `Selection`, `GridPoint`, `selected_text`, etc., qualify through `ui::egui::selection::*`. Add `use ui::egui::selection::{Selection, GridPoint, selected_text, terminal_screen_text, url_at_grid_point, normalize_selected_url_for_open, pos_to_grid_point};` near the top of main.rs.

Delete the moved items and their tests from main.rs.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move selection + URL detection to ui/egui/selection.rs

Selection, GridPoint, selected_text, url_at_grid_point and friends now
live with the egui frontend. Tests move with the code."
```

---

### Task D4: Extract rendering to `src/ui/egui/render.rs`

**Files:**
- Create: `src/ui/egui/render.rs`
- Modify: `src/ui/egui/mod.rs`, `src/main.rs`

This is the largest task. Move all paint code plus the color helpers and Welly box-art renderer.

**Important boundary change**: render functions take `&TerminalSnapshot`, not `&Terminal` and not `Arc<Mutex<Terminal>>`. The lock is acquired by the caller (App) once per frame via `backend.with_snapshot(|s| render_terminal(ui, s, ...))`. After this task, `src/ui/egui/render.rs` does **not** import `Terminal` — only `TerminalSnapshot` and `Cell`. This is the second half of the Phase 2-ready boundary.

- [ ] **Step 1: Write `src/ui/egui/render.rs`**

Move from `src/main.rs`:
- structs: `TerminalResponse`, `TerminalPaintGeometry`
- functions: `render_terminal`, `paint_terminal`, `paint_selection`, `paint_terminal_edge_bleed`, `visible_cell_at`, `cursor_underline_rect`, `text_paint_position`, `cell_background_color`, `cell_foreground_color`, `foreground_color`, `background_color`, `brighten`, `terminal_render_scale`, `color_to_egui`, `draw_welly_box_char`

Adapt signatures so they read from `&TerminalSnapshot` instead of `&Terminal`:
- `render_terminal(ui: &mut egui::Ui, snapshot: &TerminalSnapshot<'_>, selection: Option<&Selection>, zoom: f32) -> TerminalResponse`
- `paint_terminal(painter: &egui::Painter, snapshot: &TerminalSnapshot<'_>, geometry: TerminalPaintGeometry, ...)`
- `visible_cell_at(snapshot: &TerminalSnapshot<'_>, row, col) -> Option<&Cell>`

`Selection` and `GridPoint` continue to live in `ui/egui/selection.rs` (they're an egui-layer concern, not backend state). `terminal_screen_text` (selection.rs) is also retyped to take `&TerminalSnapshot` in Task D3 — adjust D3's import list if D4 land first.

Top of file:

```rust
use crate::backend::cell::{self, Cell};
use crate::backend::snapshot::TerminalSnapshot;
use crate::ui::egui::fonts::{
    self, font_for_cell, CHINESE_LEFT_MARGIN, CHINESE_TOP_MARGIN, ENGLISH_LEFT_MARGIN,
    ENGLISH_TOP_MARGIN,
};
use crate::ui::egui::selection::{GridPoint, Selection, pos_to_grid_point};
use eframe::egui;
use egui::FontFamily;

pub const CELL_WIDTH: f32 = 18.0;
pub const CELL_HEIGHT: f32 = 35.0;
pub const TERMINAL_COLS: usize = 80;
pub const TERMINAL_ROWS: usize = 24;
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 3.0;
```

Move these constants from `main.rs` and mark them `pub`. main.rs will re-import from `ui::egui::render`.

Note: `std::sync::{Arc, Mutex}` is **no longer** imported here — render no longer touches the terminal lock.

The `TerminalResponse` struct's `interact_grid_point` / `hover_grid_point` use `pos_to_grid_point` which is in `selection.rs` — fine, imported above.

Move tests with the code:
- `cursor_rect_is_bottom_underline_not_full_cell`
- `default_colors_reverse_to_visible_black_on_light_background`
- `terminal_render_scale_tracks_available_size`

Adjust their `super::` and `crate::` paths.

- [ ] **Step 2: Register module**

In `src/ui/egui/mod.rs`, add `pub mod render;`.

- [ ] **Step 3: Update main.rs**

Replace any `render_terminal(...)`, `paint_*`, `TerminalResponse`, color helpers, `color_to_egui`, etc. with `ui::egui::render::*`. Add `use ui::egui::render::{render_terminal, TerminalResponse, CELL_WIDTH, CELL_HEIGHT, TERMINAL_COLS, TERMINAL_ROWS, MIN_ZOOM, MAX_ZOOM};` and remove the local copies of those constants.

`handle_zoom_shortcut`, `terminal_size_for_zoom`, `terminal_aspect_fit_size` — keep these in `main.rs` for now (they'll move with App in Task E1).

Delete the moved items from main.rs.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move all egui rendering to ui/egui/render.rs

paint_terminal, paint_selection, draw_welly_box_char, color helpers,
TerminalResponse, geometry, and the cell-size constants live with the
egui frontend now. Tests move with the code."
```

After D4: `wc -l src/main.rs` should be roughly 900–1100 lines (the App impl, the open_url + attachment helpers, and a thin `main()`).

---

## Stage E — Extract App into `src/app.rs`, introduce `Backend` struct

### Task E1: Move `App` and `impl eframe::App` to `src/app.rs`

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

Goal: leave `main.rs` with only `main()`, the eframe options builder, the app icon, and the `mod` declarations.

**Notable shape change**: `App` does NOT implement `Default`. Instead, `App::new(cc: &eframe::CreationContext<'_>) -> Self`. The eframe creation closure in `main.rs` does `Ok(Box::new(App::new(cc)))`. This is required for Phase 1 because in E2 the `Backend` needs the real `egui::Context`-derived notify callback at construction time — a stub notify followed by re-creation on the first frame would lose any state the SSH read loop wrote between the two events.

In E1 the App still holds the existing three Arcs (terminal/parser/ssh_client) directly — `Backend` consolidation happens in E2. But the `App::new(cc)` plumbing is set up here so E2's only delta is "replace the three Arc fields with a single Backend field".

- [ ] **Step 1: Write `src/app.rs`**

Move from `src/main.rs`:
- type aliases: `ConnectResult`, `ConnectSender`, `ConnectReceiver`
- struct: `App`
- replace `impl Default for App` with `impl App { pub fn new(cc: &eframe::CreationContext<'_>) -> Self { ... } }`. The body is the old `Default::default()` body, with one addition: capture `cc.egui_ctx.clone()` as a field (e.g. `pub egui_ctx: egui::Context`) — E2 uses it to build the notify callback.
- `impl eframe::App for App`
- `impl App` (every method: `configure_viewport_once`, `start_connect`, `reconnect`, `render_login`, `render_attachment_button`, `handle_keyboard`, `handle_terminal_selection`, `handle_terminal_url_click`, `handle_terminal_mouse_click`, `copy_selection`, `open_selected_url`, `send_bytes`, `sync_window_size_to_terminal`)
- helpers used only by App: `attachment_button_label`, `open_image_attachments`, `OpenUrlCommand`, `open_url_command`, `open_url`, `handle_zoom_shortcut`, `terminal_size_for_zoom`, `terminal_aspect_fit_size`

Move their tests too:
- `mouse_entry_click_moves_cursor_to_row_and_enters` (mouse helper test — actually now this references `backend::mouse::bytes_for_entry_click`; remove if redundant or rewrite to import directly)
- `mouse_background_areas_map_to_welly_navigation_keys` (same situation — redundant with backend/mouse tests)
- `mouse_entry_click_point_uses_visible_text_range` (redundant)
- `command_plus_minus_adjust_zoom_without_sending_to_bbs`
- `terminal_aspect_fit_size_preserves_terminal_ratio`
- `terminal_size_for_zoom_scales_full_window`
- `attachment_button_label_opens_all_detected_images`
- `open_url_command_uses_*_on_*` (cfg-target-specific)

Redundant tests with `backend::mouse::tests`: delete them outright. The backend version is the canonical one now.

Header of `src/app.rs`:

```rust
use crate::backend::ansi_parser::AnsiParser;
use crate::backend::attachment::{parse_image_attachments, ImageAttachment};
use crate::backend::ssh::{is_channel_closed_error, SshClient};
use crate::backend::terminal::Terminal;
use crate::config::ConnectionSettings;
use crate::ui::egui::input as egui_input;
use crate::ui::egui::render::{
    self, render_terminal, TerminalResponse, CELL_HEIGHT, CELL_WIDTH, MAX_ZOOM, MIN_ZOOM,
    TERMINAL_COLS, TERMINAL_ROWS,
};
use crate::ui::egui::selection::{
    self, normalize_selected_url_for_open, pos_to_grid_point, selected_text,
    terminal_screen_text, url_at_grid_point, GridPoint, Selection,
};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::process::Command;
use std::sync::{Arc, Mutex};
```

Function call adjustments inside the moved code:
- `mouse_entry_click_to_bytes(cursor_row, point.row)` → `crate::backend::mouse::bytes_for_entry_click(cursor_row, point.row)`
- `is_mouse_entry_click_point(&terminal, point)` → `crate::backend::mouse::is_entry_click_point(&terminal, point)`
- `mouse_background_navigation_bytes(point)` → `crate::backend::mouse::bytes_for_background_navigation(/* convert GridPoint */)`. Note: `selection::GridPoint` and `backend::input::GridPoint` are two structurally identical types. Either:
  - (a) Re-use `backend::input::GridPoint` in selection.rs (cleanest; do this — see step 2).
  - (b) Translate between the two at the call site.

Inside `handle_terminal_mouse_click`, the GridPoint comes from `pos_to_grid_point` (selection.rs). After step 2 of this task, that returns `backend::input::GridPoint` so the call sites just work.

- [ ] **Step 2: Consolidate `GridPoint`**

In `src/ui/egui/selection.rs`, delete the local `GridPoint` struct and replace with `pub use crate::backend::input::GridPoint;` at the top. Update `Selection { start: GridPoint, end: GridPoint }` to use the imported type. Update `grid_index(point: GridPoint)` signature unchanged.

- [ ] **Step 3: Register module in main.rs**

In `src/main.rs`:
- Add `mod app;`
- Add `use app::App;`
- Change the eframe creation closure body from `Ok(Box::new(App::default()))` to `Ok(Box::new(App::new(cc)))`. `configure_fonts(&cc.egui_ctx)` and `configure_terminal_view(&cc.egui_ctx)` continue to run before `App::new(cc)` since they configure egui's global font registry.

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: move App and event loop into src/app.rs

main.rs is now only startup + eframe options. impl eframe::App and the
App methods (start_connect, handle_keyboard, render_*, send_bytes, etc.)
live in app.rs. Tests redundant with backend::mouse removed."
```

After E1: `wc -l src/main.rs` should be roughly 150–220 lines.

---

### Task E2: Introduce `Backend` struct in `src/backend/mod.rs`

**Files:**
- Create: `src/backend/backend.rs`
- Modify: `src/backend/mod.rs`, `src/app.rs`

Consolidate the three Arcs (`terminal`, `parser`, `ssh_client`) currently held by `App` into one `Backend` struct exposing the API surface from spec §3. Backend owns **all** `InputEvent` → byte translation (keys, mouse-with-cursor-resolution, GB18030 paste encoding) and **all** terminal state — its fields are private; the only read path is `with_snapshot`.

- [ ] **Step 1: Write `src/backend/backend.rs`**

(`tokio::sync::watch` is available — Cargo.toml uses `tokio = { features = ["full"] }`. No dependency change needed.)

```rust
//! High-level backend API consumed by frontends.
//!
//! Spec §3 API. All InputEvent → bytes translation lives here so a future
//! gpui frontend can reuse the byte mappings without touching SSH or
//! terminal state directly.

use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;
use encoding_rs::GB18030;
use tokio::sync::watch;

use super::ansi_parser::AnsiParser;
use super::input::{InputEvent, MouseEvent};
use super::snapshot::TerminalSnapshot;
use super::ssh::{is_channel_closed_error, SshClient};
use super::terminal::Terminal;
use super::{keys, mouse};
use crate::config::ConnectionSettings;

type ConnectResult = Result<Arc<SshClient>, String>;

pub struct Backend {
    settings: Mutex<ConnectionSettings>,
    terminal: Arc<Mutex<Terminal>>,
    parser: Arc<Mutex<AnsiParser>>,
    client: Mutex<Option<Arc<SshClient>>>,
    connect_rx: Mutex<Option<Receiver<ConnectResult>>>,
    connection_error: Mutex<Option<String>>,
    notify: Arc<dyn Fn() + Send + Sync>,
    changes_tx: watch::Sender<()>,
}

impl Backend {
    pub fn new(
        config: ConnectionSettings,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (changes_tx, _rx) = watch::channel(());
        Self {
            settings: Mutex::new(config),
            terminal: Arc::new(Mutex::new(Terminal::new(24, 80))),
            parser: Arc::new(Mutex::new(AnsiParser::new())),
            client: Mutex::new(None),
            connect_rx: Mutex::new(None),
            connection_error: Mutex::new(None),
            notify: combined_notify(notify, changes_tx.clone()),
            changes_tx,
        }
        // No auto-connect. Call `reconnect()` to start the first
        // connection — App does this when settings look valid, or after
        // the user submits the login form.
    }

    /// Read-only snapshot of terminal state. The closure runs under the
    /// terminal lock; keep it short.
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&TerminalSnapshot<'_>) -> R) -> R {
        let t = self.terminal.lock().unwrap();
        let snap = t.snapshot();
        f(&snap)
    }

    /// Translate a high-level input event into bytes and forward to SSH.
    /// Owns text/IME GB18030 encoding, Welly key escape mapping, and the
    /// wheel/entry/background mouse resolution against current cursor state.
    pub fn send_input(&self, event: InputEvent) {
        let bytes_opt = match event {
            InputEvent::Key(k) => keys::bytes_for_key(k),
            InputEvent::Mouse(m) => self.bytes_for_mouse(m),
            InputEvent::Paste(text) => paste_bytes(&text),
            InputEvent::Resize { .. } => {
                // Welly is a fixed 24x80 BBS terminal. Resize is part of the
                // InputEvent surface (spec §5) so future frontends or future
                // resizable BBS profiles can route it through here, but the
                // current Terminal has no resize path. No-op for Phase 1.
                None
            }
            InputEvent::Reconnect => {
                self.reconnect();
                None
            }
            InputEvent::Shutdown => {
                self.shutdown();
                None
            }
        };
        if let Some(b) = bytes_opt {
            self.send_bytes(b);
        }
    }

    /// Subscribe to lightweight change notifications. Returns a watch
    /// receiver that fires whenever Backend or the SSH read loop calls the
    /// internal notify. This is not a full state log; consumers should
    /// re-read snapshot / connection state after receiving a change. Egui
    /// frontend uses the push notify (egui::Context repaint); a future gpui
    /// frontend can `await` this receiver.
    pub fn subscribe_changes(&self) -> watch::Receiver<()> {
        self.changes_tx.subscribe()
    }

    /// Drop current SSH client and start a fresh connection using the
    /// most recently configured settings.
    pub fn reconnect(&self) {
        *self.client.lock().unwrap() = None;
        self.spawn_connect();
        (self.notify)();
    }

    /// Request graceful teardown. Drops the SSH client; the background
    /// tokio runtime exits as the channel closes.
    pub fn shutdown(&self) {
        *self.client.lock().unwrap() = None;
        *self.connect_rx.lock().unwrap() = None;
        (self.notify)();
    }

    /// Update connection settings for future reconnects (login form).
    pub fn update_settings(&self, settings: ConnectionSettings) {
        *self.settings.lock().unwrap() = settings;
        (self.notify)();
    }

    pub fn is_connected(&self) -> bool {
        self.client
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.is_connected())
    }

    /// Drain the connect channel non-blockingly; promotes a successful
    /// connection into `self.client` and stores an error for the UI.
    /// Returns `Some(Ok(()))` exactly once on success, `Some(Err(msg))`
    /// once on failure, `None` while still pending.
    pub fn poll_connect_result(&self) -> Option<Result<(), String>> {
        let rx = self.connect_rx.lock().unwrap().clone()?;
        let result = rx.try_recv().ok()?;
        *self.connect_rx.lock().unwrap() = None;
        match result {
            Ok(client) => {
                *self.client.lock().unwrap() = Some(client);
                (self.notify)();
                Some(Ok(()))
            }
            Err(e) => {
                *self.connection_error.lock().unwrap() = Some(e.clone());
                Some(Err(e))
            }
        }
    }

    pub fn take_connection_error(&self) -> Option<String> {
        self.connection_error.lock().unwrap().take()
    }

    // ---- internals ----

    fn spawn_connect(&self) {
        self.terminal.lock().unwrap().clear_all();
        *self.parser.lock().unwrap() = AnsiParser::new();
        (self.notify)();

        let terminal = Arc::clone(&self.terminal);
        let parser = Arc::clone(&self.parser);
        let notify = Arc::clone(&self.notify);
        let settings = self.settings.lock().unwrap().clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        *self.connect_rx.lock().unwrap() = Some(rx);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match SshClient::connect(settings, terminal, parser, notify).await {
                    Ok(client) => {
                        log::info!("SSH connected successfully");
                        let _ = tx.send(Ok(client));
                        std::future::pending::<()>().await;
                    }
                    Err(e) => {
                        log::error!("SSH error: {}", e);
                        let _ = tx.send(Err(e.to_string()));
                    }
                }
            });
        });
    }

    fn send_bytes(&self, bytes: Vec<u8>) {
        let Some(client) = self.client.lock().unwrap().clone() else {
            return;
        };
        if !client.is_connected() {
            return;
        }
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                if let Err(e) = client.send_data(&bytes).await {
                    if is_channel_closed_error(&e) {
                        log::debug!("Ignoring send after SSH channel ended: {}", e);
                    } else {
                        log::error!("Send error: {}", e);
                    }
                }
            });
        });
    }

    fn bytes_for_mouse(&self, event: MouseEvent) -> Option<Vec<u8>> {
        match event {
            MouseEvent::Wheel(d) => Some(mouse::bytes_for_wheel(d)),
            MouseEvent::Click(point) => {
                let term = self.terminal.lock().unwrap();
                if mouse::is_entry_click_point(&term, point) {
                    Some(mouse::bytes_for_entry_click(term.cursor_row, point.row))
                } else {
                    mouse::bytes_for_background_navigation(point)
                }
            }
        }
    }
}

fn paste_bytes(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    let (b, _, _) = GB18030.encode(text);
    Some(b.into_owned())
}

fn combined_notify(
    user: Arc<dyn Fn() + Send + Sync>,
    tx: watch::Sender<()>,
) -> Arc<dyn Fn() + Send + Sync> {
    Arc::new(move || {
        user();
        tx.send_replace(());
    })
}
```

- [ ] **Step 2: Register**

In `src/backend/mod.rs`, add:

```rust
mod backend;
pub use backend::Backend;
```

(The file is `backend.rs` inside `src/backend/`; the `mod backend` declaration is non-public so callers reach the struct via `crate::backend::Backend`.)

- [ ] **Step 3: Migrate `App` to hold `Backend`**

In `src/app.rs`:

- Replace the four fields (`terminal`, `parser`, `ssh_client`, `connect_rx`) with a single `backend: crate::backend::Backend`. Also delete `auto_connect_attempted` if it existed — `App::new` performs the single initial connection decision explicitly after constructing Backend.
- `App::new(cc)` constructs the backend directly with the real notify:

  ```rust
  impl App {
      pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
          let settings = ConnectionSettings::load_default();
          let login_host = settings.host.clone();
          let login_port = settings.port.to_string();
          let login_username = settings.username.clone().unwrap_or_default();
          let ctx = cc.egui_ctx.clone();
          let notify: Arc<dyn Fn() + Send + Sync> =
              Arc::new(move || ctx.request_repaint());
          let backend = Backend::new(settings, notify);
          Self {
              backend,
              login_host,
              login_port,
              login_username,
              // ... other UI-only fields
          }
      }
  }
  ```

- Replace every read of terminal state with `self.backend.with_snapshot(|snap| ...)`. There is no public `terminal` field anymore. For render: `self.backend.with_snapshot(|snap| render_terminal(ui, snap, selection.as_ref(), self.zoom))`. For selection / URL detection: similarly take a snapshot then call into `ui::egui::selection`.
- Every keyboard/mouse handler routes through `self.backend.send_input(...)`. Delete App's local `send_bytes`. The only path left for raw bytes is paste-from-clipboard, which becomes `InputEvent::Paste(string)`.
- App's `reconnect` becomes a thin wrapper: `self.backend.update_settings(self.current_login_settings()); self.backend.reconnect();`. Or it can stay if the login form mutates settings via a separate codepath.
- Login form mutates a local copy and calls `self.backend.update_settings(new_settings)` before triggering reconnect.
- Polling in `update`:

  ```rust
  match self.backend.poll_connect_result() {
      Some(Ok(())) => { self.connected = true; }
      Some(Err(e)) => { self.connection_error = Some(e); self.connected = false; }
      None => {}
  }
  ```

- Initial auto-connect: in `App::new`, after building `Backend`, call `backend.reconnect()` iff settings have host + username (mirrors the old `auto_connect_attempted` gate in `default()/update`). If settings are incomplete, leave Backend idle — the login form path triggers `backend.reconnect()` on submit. (Note: this is the only call site of `reconnect()` for the "initial" case; spec calls it `reconnect` because Backend's lifecycle treats new connections and reconnects uniformly — there is no distinct "connect" verb.)

- [ ] **Step 4: Verify**

```bash
cargo build
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```
Expected: all pass.

- [ ] **Step 5: Manual smoke (human)**

Run `cargo run`. Confirm the BBS still loads, login screen appears if no SSH user is set, arrow keys / Alt-arrows / Cmd+R / mouse wheel / selection / Cmd+C / double-click URL all behave identically to the baseline captured in Pre-flight Step 3.

If anything regresses, STOP and report. Do not move on to F1.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(backend): deliver spec §3 Backend API

Backend owns terminal/parser/SshClient + notify + watch::Sender; exposes
new(config, notify), with_snapshot, send_input, subscribe_changes,
reconnect, shutdown — the surface Phase 2's gpui frontend consumes.

send_input owns all InputEvent→bytes translation: Welly key escapes via
backend::keys, mouse-with-cursor-resolution via backend::mouse, paste
GB18030 encoding inline. ui::egui::input only translates egui::Event
into InputEvent; no byte mapping leaks into the egui layer.

App holds a single private Backend, accesses terminal state only via
with_snapshot. Three Arc fields collapse into one. App::new(cc) builds
Backend with the real egui::Context-derived notify at startup — no
two-phase re-init."
```

---

## Stage F — Final verification

### Task F1: Confirm Phase 1 acceptance criteria

**Files:** none (verification only)

- [ ] **Step 1: `main.rs` line count**

```bash
wc -l src/main.rs
```
Expected: ≤ 200. If higher: review what's still in `main.rs` — likely candidates to move are the eframe options builder (consider extracting `fn run_egui_app() -> eframe::Result` into `src/ui/egui/mod.rs::run`), the app icon loader, or constants. Make one more `refactor:` commit to bring it under the limit.

- [ ] **Step 2: Full test/lint sweep**

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```
Expected: all pass. Total test count should be in the high 60s / low 70s (a couple of tests were consolidated into `backend::mouse::tests`, several moved into `ui::egui` submodules).

- [ ] **Step 3: Visual regression (human)**

Run `cargo run`. Walk through:
1. Login (auto-connect with SSH config, or manual via login panel)
2. Main menu screen — rows 0 and 23 status bars present
3. Open a board, scroll with arrows + Alt-arrows + mouse wheel
4. Click left edge → back; click right upper/lower → page up/down
5. Open a post with image attachment → bottom-right button appears → clicking opens browser
6. Select a URL, double-click → browser opens
7. Cmd+C copies selection
8. Cmd+R reconnects
9. Cmd+Plus / Cmd+Minus / Cmd+0 zoom

Compare against Pre-flight screenshot. Any pixel-level mismatch is a regression — stop and report which task it traces back to via `git bisect` over the Stage A–E commits.

- [ ] **Step 4: Mark Phase 1 done**

```bash
git log --oneline ^master HEAD
```
Expected: the new commits from Stage A through F1 in order. Tag if desired:

```bash
git tag phase-1-complete
```

Update root `todo.md`: replace the old migration plan section with a one-line pointer to this completed plan, or just delete `todo.md` (it's superseded by `docs/superpowers/specs/...` already).

Phase 2 (gpui prototype) gets its own spec + plan; do not start it from this plan.

---

## Conformance to spec

Phase 1 delivers spec §3's Backend API verbatim:

```rust
impl Backend {
    pub fn new(config: ConnectionSettings, notify: Arc<dyn Fn() + Send + Sync>) -> Self;
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&TerminalSnapshot<'_>) -> R) -> R;
    pub fn send_input(&self, event: InputEvent);
    pub fn subscribe_changes(&self) -> tokio::sync::watch::Receiver<()>;
    pub fn reconnect(&self);
    pub fn shutdown(&self);
}
```

Two implementation details worth flagging (not spec deviations):

- **`subscribe_changes` is built on top of a notify callback.** Backend stores `Arc<dyn Fn() + Send + Sync>` (the push side, used by `SshClient::connect` to wake the egui frame loop) and a `watch::Sender<()>`. Every notify invocation also pings the watch sender, so pull-style consumers (future gpui frontend) can `await` the receiver and then re-read snapshot / connection state. This is a lightweight invalidation signal, not a full state log. The egui frontend in Phase 1 only uses the notify path through `cc.egui_ctx.request_repaint`; it does not subscribe.
- **Connection async/sync handoff stays internal.** Backend kicks off SSH connection on a background tokio thread inside `new()` (or `reconnect()`) and uses an internal `crossbeam-channel` to expose connection state. The App sees connection state through `Backend::is_connected()` / `Backend::take_connection_error()` accessors, not the channel itself. App does not know about tokio.

These are implementation details under spec's API, not departures from it.

---

## Plan summary table

| Stage | Task | Effect | Commit subject prefix |
|-------|------|--------|----------------------|
| A | A1 | Color::egui_color → Color::rgb | `refactor:` |
| A | A2 | SshClient takes notify, not egui::Context | `refactor:` |
| B | B1 | Empty `src/backend/mod.rs` | `refactor:` |
| B | B2 | git mv cell.rs → backend/ | `refactor:` |
| B | B3 | git mv terminal.rs → backend/ | `refactor:` |
| B | B4 | git mv ansi_parser.rs → backend/ | `refactor:` |
| B | B5 | git mv attachment.rs → backend/ | `refactor:` |
| B | B6 | git mv ssh.rs → backend/ | `refactor:` |
| C | C1 | backend/input.rs (UI-neutral types) | `feat(backend):` |
| C | C2 | backend/keys.rs (Welly key→bytes) | `feat(backend):` |
| C | C3 | backend/mouse.rs (Welly mouse→bytes) | `feat(backend):` |
| C | C4 | backend/snapshot.rs (TerminalSnapshot) | `feat(backend):` |
| D | D1 | ui/egui/fonts.rs | `refactor:` |
| D | D2 | ui/egui/input.rs (egui::Event → InputEvent) | `refactor:` |
| D | D3 | ui/egui/selection.rs (selection, URL; consumes &TerminalSnapshot) | `refactor:` |
| D | D4 | ui/egui/render.rs (paint, box art; consumes &TerminalSnapshot) | `refactor:` |
| E | E1 | src/app.rs (App + event loop, App::new(cc)) | `refactor:` |
| E | E2 | Backend struct with spec §3 API + App migration | `feat(backend):` |
| F | F1 | Verify ≤200 lines + acceptance | (no commit unless cleanup needed) |

19 commits total. Each is bisectable; each leaves the app functionally identical to baseline.
