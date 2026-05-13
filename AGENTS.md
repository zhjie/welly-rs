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

## Known Bug Investigation Log

### 状态栏行消失（2025-05-12）

**现象**：主选单和帖子正文页面的 row=0（版面/时间信息）和 row=23（用户名/时间/信件状态）有时不显示，时间越长越容易触发，具有 heisenbug 特征（加了诊断代码后反而不出现）。

**BBS 协议分析**：通过原始数据 dump（`src/ssh.rs` 写入 `%TEMP%\welly-rs-raw.bin`，分析脚本 `scripts/analyze_dump.py`）确认：
- newsmth BBS **不使用 DECSTBM** 滚动区域，scroll_top/scroll_bottom 在正常会话中始终为默认值（0/23）。
- BBS 更新 row=0 的典型模式：绝对定位到 row=23（`ESC[24;NH]`）→ 相对上移（`ESC[23A]` 或组合上移）→ `CR` → `ESC[K`（清行）→ 写内容 → `LF`。
- 相对上移（`ESC[nA`）是关键——它依赖终端的 `cursor_row` 与 BBS 预期完全一致；若 `cursor_row` 偏大，上移后落在错误行，状态栏被写到错误位置。
- BBS 在弹出框场景下会清空 row=0/row=1 而**不立即重写**（`ESC[2;25H]` → `ESC[A]` → `CR` → `ESC[K]` → `LF` → `ESC[K]`），这是正常行为，弹出框关闭后会恢复。

**已修复的 VT100 规范问题**（可能间接相关）：
1. `DECSTBM`（`ESC[r]`）处理：设置滚动区域后，normal mode 应将光标 home 到 (0,0)，origin mode 应 home 到 scroll_top。之前的代码无论哪种模式都 home 到 scroll_top。
2. `line_feed`：光标在 scroll_bottom 以下时不应触发 scroll，之前会在任何位置 LF 都可能 scroll。
3. `put_char` autowrap：同上，wrap 后移行只在 `cursor_row == scroll_bottom` 时才 scroll。
4. `set_scroll_region`：参数无效时重置为全屏，之前可能设置 top >= bottom 的非法区域。

**heisenbug 假说**：BBS 的"清行 → 写内容"可能被拆成两个 TCP 包。`ssh.rs` 每收到一个 `ChannelMsg::Data` 就立即 `ctx.request_repaint()`，若两包之间恰好触发渲染，屏幕会短暂显示清空后的空行。是否"卡住"取决于第二包是否正常到达。加了磁盘 dump 后写入延迟可能让两包合并，解释了为何加诊断后 bug 消失。

**2026-05-13 复现观察**：
- 截图 `QQ20260513-012630.png` 显示 row=23 的静态标签仍在，但动态字段 `时间[...]` 和 `使用者[...]` 为空，说明不是整行完全消失，也不像单纯字体渲染问题。
- 开启 raw dump 后现象消失，用户判断很可能是写盘导致性能下降或时序延迟，从而掩盖 bug；不要把“开 dump 正常”当作修复证据。
- 曾临时尝试将数据包后的 repaint 合并为约 8ms 延迟，未开 raw dump 时重启后暂时正常；但该方案目前按用户要求已关闭，不作为当前修复。
- 当前保留的改动是对齐 Welly 参考实现：支持 DA/DSR 终端查询响应（如 `ESC[0c`、`ESC[5n`、`ESC[6n`）并通过 SSH channel 回写；这属于兼容性修复，不能单独证明状态栏 heisenbug 已根治。

**当前诊断方式**：诊断代码默认关闭。需要抓证据时，用环境变量显式开启：
`WELLY_RS_RAW_DUMP=/private/tmp/welly-rs-raw.bin WELLY_RS_TRACE_DUMP=/private/tmp/welly-rs-trace.txt cargo run`。
注意：raw/trace 写盘本身可能改变时序并掩盖问题。若 dump 下无法复现，优先考虑低开销环形内存 trace 或仅在异常检测时落盘，而不是继续依赖持续写盘。

**待确认**：需要在 bug 实际出现且尽量不扰动时序的情况下捕获证据，确认哪次 `ERASE LINE row=0/23` 之后没有对应写入，或确认服务端动态字段是否依赖 DA/DSR/CPR 响应。

Pull requests should include a short summary, test results (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`), and screenshots or recordings for visible UI changes. Link related issues when available and call out any networking, authentication, or platform-specific assumptions.
