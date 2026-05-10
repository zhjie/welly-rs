# Repository Guidelines

## Project Goal

`welly-rs` is a native Rust/egui SSH client for `bbs.newsmth.net` that aims to feel comfortable in a Welly-like terminal experience. Current work centers on terminal fidelity: GB18030 streaming decode, ANSI/VT100 parser correctness, double-width cell behavior, Welly-like font metrics, keyboard navigation, SSH channel lifecycle, and faithful rendering of BBS screens.

## Project Structure

This is a single Rust binary crate built with `eframe`/`egui`, `tokio`, and `russh`.

- `src/main.rs` owns app startup, font setup, event handling, selection/copy, attachment shortcuts, window sizing, and terminal rendering.
- `src/ssh.rs` manages SSH connection, authentication, channel setup, incoming data, outgoing keyboard data, and anti-idle traffic.
- `src/terminal.rs` stores terminal state, cursor/screen operations, scrolling, attributes, and double-width cell invariants.
- `src/ansi_parser.rs` decodes GB18030 text and applies ANSI/CSI/VT100 control sequences.
- `src/cell.rs` defines terminal cells and color conversion.
- `src/config.rs` reads connection settings from defaults and SSH config.
- `src/attachment.rs` detects NewSMTH image attachment links from visible terminal text.

## Build, Test, and Development Commands

- `cargo build` compiles the application in debug mode.
- `cargo run` launches the native GUI client locally.
- `cargo test` runs the unit test suite.
- `cargo fmt --check` verifies Rust formatting without changing files.
- `cargo fmt` formats Rust files.
- `cargo clippy --all-targets --all-features -- -D warnings` runs lint checks and treats warnings as failures.

Run `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings` before considering code complete. Use `cargo build` as an additional smoke check for GUI or SSH changes.

## Connection Behavior

The default target is `bbs.newsmth.net:22`. `ConnectionSettings::load_default()` reads a matching `Host bbs.newsmth.net` block from `~/.ssh/config`, currently honoring `Port`, `User`, and `IdentityFile`. If no username is configured, the app shows the login panel. Passwords are only supplied from the runtime login UI.

The app auto-connects on startup when a username is available. Keyboard input is sent directly to SSH. Cmd+R reconnects. Anti-idle sends NUL bytes after the Welly idle window and should stop quietly after the SSH channel ends. Expected channel shutdown should not be logged as an error.

## Coding Style

Use Rust 2021 style and `rustfmt` defaults: 4-space indentation, `snake_case` functions/modules, `PascalCase` types, and `SCREAMING_SNAKE_CASE` constants.

Keep modules aligned with existing responsibilities: parsing in `ansi_parser`, screen state in `terminal`, transport in `ssh`, rendering and UI orchestration in `main`. Avoid broad refactors while changing terminal parsing or rendering behavior.

Prefer explicit error logging at async or thread boundaries, but do not log expected channel shutdown as an error. Treat macOS IMK messages such as `IMKCFRunLoopWakeUpReliable` as system/input-method noise unless there is a matching user-visible input bug.

## Terminal Model Notes

The terminal is a fixed 24x80 grid. `Cell.width` tracks single-width, double-width, and continuation cells:

- Main double-width cell stores the visible character and `width = 2`.
- Continuation cell stores `ch = '\0'` and `width = 0`.
- Rendering and selection skip continuation cells.
- Any write, erase, delete, line clear, or screen clear that touches half of a double-width character must clear or preserve the whole character consistently.

Prioritize unit tests for terminal invariants because they can reproduce rendering bugs without opening the GUI or network. Recent stale-character bugs came from treating half of a double-width character as independently writable.

## Parsing Notes

`ansi_parser` uses streaming `encoding_rs::GB18030` decoding. Keep GB18030 input as byte slices in tests. The parser handles cursor movement, clearing, colors, attributes, scrolling, LF/NEL behavior, and VT100 special graphics.

VT100 `ESC ( 0` special graphics mapping is important for Welly screens. Common mappings include `j/k/l/m/n/q/t/u/v/w/x` to box drawing characters and symbols such as `◆`, `▒`, `°`, `±`, `π`, `≠`, `£`, and `·`.

## Rendering Notes

The UI uses Welly-like terminal metrics:

- `CELL_WIDTH = 18.0`
- `CELL_HEIGHT = 35.0`
- `TERMINAL_COLS = 80`
- `TERMINAL_ROWS = 24`

Configured fonts:

- English: resolved by `fontdb` from the shared candidate list, preferring `Monaco`, then mono-style Cascadia/Caskaydia fonts.
- Chinese: resolved by `fontdb` from the shared candidate list, preferring `Heiti SC`, then mono-style SimHei/Noto/Sarasa fallbacks.

Named egui font families are bound for both fonts with cross-fallback. Text is positioned from the cell top margin so mixed Chinese/ASCII status text such as `在线/最高:1524/9994` has stable top spacing.

Many Welly line/block characters are drawn with egui primitives instead of font glyphs to better match Welly's thicker box art. If line art changes, compare screenshots and tune geometry in `src/main.rs` rather than changing parser mappings first.

## Keyboard Notes

Arrow keys use Welly-compatible escape sequences:

- Up: `\x1b[A`
- Down: `\x1b[B`
- Right: `\x1b[C`
- Left: `\x1b[D`

Alt-arrow shortcuts map to Welly navigation sequences:

- Alt-Up: `\x1b[5~`
- Alt-Down: `\x1b[6~`
- Alt-Right: `\x1b[4~`
- Alt-Left: `\x1b[1~`

Command shortcuts should be handled locally and not sent to BBS.

## Testing Guidelines

Add focused unit tests near the code they exercise using `#[cfg(test)] mod tests`. Prioritize parser and terminal state transitions. Assert final cursor position, cell contents, widths, colors, attributes, and selected text.

For GUI, font, or SSH changes, document manual verification in the PR or final notes, including the command used and the connection path tested. Screenshots are useful for visible UI changes, but there are no committed reference screenshots at the moment.

## Visual Regression Workflow

When checking visual fidelity:

1. Run `cargo run`.
2. Capture fresh screenshots for the current scenario.
3. Compare against Welly or another trusted live/reference capture.
4. Focus on ASCII/VT100 art thickness and alignment, row/column spacing, cursor placement, reverse-video/background spans, mixed Chinese/ASCII top alignment, missing glyphs or unexpected `?`, and double-width overwrite/clearing behavior.

The Welly source reference lives at `/Users/zhjie/workspace/src/welly` when available locally. Useful reference files include:

- `Welly/WLTerminalFeeder.m` for parser and cursor behavior
- `Welly/WLTerminalView.m` for keyboard/rendering behavior
- `Welly/CommonType.h` for key escape constants

## Commit and Pull Request Guidelines

Use concise imperative commit subjects, for example `Add parser tests for erase sequences`.

Pull requests should include a short summary, test results (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`), and screenshots or recordings for visible UI changes. Link related issues when available and call out any networking, authentication, or platform-specific assumptions.
